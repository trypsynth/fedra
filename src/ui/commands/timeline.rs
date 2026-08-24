//! Commands that open, close, navigate, and refresh timelines.

use std::{
	cell::Cell,
	time::{Duration, Instant},
};

use wxdragon::prelude::*;

use super::{
	UiCommand, UiCommandContext, handle_ui_command,
	selection::{foreign_url, get_selected_entry, get_selected_status, paging_max_id},
};
use crate::{
	AppState,
	accounts::{start_streaming_for_timeline, update_window_title},
	config::SortOrder,
	mastodon::Status,
	network::NetworkCommand,
	timeline::{TimelineEntry, TimelineType},
	ui::{
		dialogs,
		menu::update_menu_labels,
		timeline_view::{
			list_index_to_entry_index, sync_timeline_selection_from_list, update_active_timeline_ui,
			with_suppressed_selection,
		},
	},
};

/// Refreshes the current timeline by re-fetching from the network.
pub(super) fn refresh_timeline(state: &AppState, live_region: &crate::ui::timeline_list::TimelineList) {
	let timeline_type = match state.timeline_manager.active() {
		Some(t) => t.timeline_type.clone(),
		None => return,
	};
	match &state.network_handle {
		Some(handle) => {
			handle.send(NetworkCommand::FetchTimeline { timeline_type, limit: Some(40), max_id: None });
		}
		None => {
			live_region.announce("Network not available");
		}
	}
}

pub(super) fn poll_non_streaming_timelines(state: &AppState) {
	let Some(handle) = &state.network_handle else { return };
	for timeline in state.timeline_manager.timelines() {
		if timeline.stream_handle.is_none() && timeline.timeline_type.stream_params().is_some() {
			handle.send(NetworkCommand::FetchTimeline {
				timeline_type: timeline.timeline_type.clone(),
				limit: Some(40),
				max_id: None,
			});
		}
	}
}

pub(super) fn open_timeline(
	state: &mut AppState,
	selector: ListBox,
	timeline_list: &crate::ui::timeline_list::TimelineList,
	timeline_type: &TimelineType,
	suppress_selection: &Cell<bool>,
	live_region: &crate::ui::timeline_list::TimelineList,
	frame: &Frame,
) {
	if matches!(timeline_type, TimelineType::User { .. } | TimelineType::Thread { .. }) {
		state.timeline_manager.snapshot_active_to_history();
	}

	if !state.timeline_manager.open(timeline_type.clone()) {
		if let Some(index) = state.timeline_manager.index_of(timeline_type) {
			state.timeline_manager.set_active(index);
			update_window_title(state, frame);
			with_suppressed_selection(suppress_selection, || {
				selector.set_selection(u32::try_from(index).unwrap(), true);
			});
			{
				let view_options =
					state.timeline_manager.active().map(|a| state.timeline_view_options_for(&a.timeline_type));
				let active_index = state.timeline_manager.active_index();
				if let Some(view_options) = view_options
					&& let Some(active) = state.timeline_manager.active_mut()
				{
					update_active_timeline_ui(
						timeline_list,
						active,
						suppress_selection,
						&view_options,
						&state.cw_expanded,
						active_index,
					);
				}
			}
		}
		if let Some(mb) = frame.get_menu_bar() {
			update_menu_labels(&mb, state);
		}
		live_region.announce("Timeline already open");
		return;
	}
	selector.append(&timeline_type.display_name());
	let new_index = state.timeline_manager.len() - 1;
	state.timeline_manager.set_active(new_index);
	update_window_title(state, frame);
	with_suppressed_selection(suppress_selection, || {
		selector.set_selection(u32::try_from(new_index).unwrap(), true);
	});
	if !matches!(timeline_type, TimelineType::Thread { .. } | TimelineType::Search { .. }) {
		if let Some(handle) = &state.network_handle {
			handle.send(NetworkCommand::FetchTimeline {
				timeline_type: timeline_type.clone(),
				limit: Some(40),
				max_id: None,
			});
		}
		start_streaming_for_timeline(state, timeline_type);
	}
	with_suppressed_selection(suppress_selection, || {
		timeline_list.clear();
	});
	if let Some(mb) = frame.get_menu_bar() {
		update_menu_labels(&mb, state);
	}
}

/// Closes the current timeline if it's closeable.
pub(super) fn close_timeline(
	state: &mut AppState,
	selector: ListBox,
	timeline_list: &crate::ui::timeline_list::TimelineList,
	suppress_selection: &Cell<bool>,
	live_region: &crate::ui::timeline_list::TimelineList,
	use_history: bool,
	frame: &Frame,
) {
	let active_type = match state.timeline_manager.active() {
		Some(t) => t.timeline_type.clone(),
		None => return,
	};
	if state.timeline_manager.len() <= 1 {
		live_region.announce("Cannot close the only open timeline");
		return;
	}
	if !state.timeline_manager.close(&active_type, use_history) {
		return;
	}
	let active_index = state.timeline_manager.active_index();
	let active_name = state.timeline_manager.display_names().get(active_index).cloned();
	if let Some(name) = &active_name {
		live_region.announce(name);
	}

	selector.clear();
	for name in state.timeline_manager.display_names() {
		selector.append(&name);
	}
	with_suppressed_selection(suppress_selection, || {
		selector.set_selection(u32::try_from(active_index).unwrap(), true);
	});
	{
		let view_options = state.timeline_manager.active().map(|a| state.timeline_view_options_for(&a.timeline_type));
		let active_index = state.timeline_manager.active_index();
		if let Some(view_options) = view_options
			&& let Some(active) = state.timeline_manager.active_mut()
		{
			update_active_timeline_ui(
				timeline_list,
				active,
				suppress_selection,
				&view_options,
				&state.cw_expanded,
				active_index,
			);
		}
	}
	update_window_title(state, frame);
}

pub(super) fn refresh(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	let live_region = ctx.live_region;
	refresh_timeline(state, live_region);
}

pub(super) fn poll_non_streaming(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	poll_non_streaming_timelines(state);
}

pub(super) fn open(ctx: &mut UiCommandContext<'_>, timeline_type: TimelineType) {
	let state = &mut *ctx.state;
	let frame = ctx.frame;
	let timelines_selector = ctx.timelines_selector;
	let timeline_list = &ctx.timeline_list;
	let suppress_selection = ctx.suppress_selection;
	let live_region = ctx.live_region;
	open_timeline(state, timelines_selector, timeline_list, &timeline_type, suppress_selection, live_region, frame);
}

pub(super) fn sent_timeline(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	let frame = ctx.frame;
	let timelines_selector = ctx.timelines_selector;
	let timeline_list = &ctx.timeline_list;
	let suppress_selection = ctx.suppress_selection;
	let live_region = ctx.live_region;
	let Some(id) = state.current_user_id.clone() else {
		live_region.announce("Account information not loaded yet");
		return;
	};
	let timeline_type = TimelineType::User { id, name: "Sent".to_string() };
	open_timeline(state, timelines_selector, timeline_list, &timeline_type, suppress_selection, live_region, frame);
}

pub(super) fn close(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	let frame = ctx.frame;
	let timelines_selector = ctx.timelines_selector;
	let timeline_list = &ctx.timeline_list;
	let suppress_selection = ctx.suppress_selection;
	let live_region = ctx.live_region;
	close_timeline(state, timelines_selector, timeline_list, suppress_selection, live_region, false, frame);
}

pub(super) fn load_more_background(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	if let Some(active) = state.timeline_manager.active_mut() {
		active.loading_more_in_background = true;
	}
	handle_ui_command(UiCommand::LoadMore, ctx);
}

pub(super) fn home_pressed(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	let timeline_list = &ctx.timeline_list;
	if timeline_list.get_selection() != Some(0) {
		if let Some(active) = state.timeline_manager.active_mut() {
			let effective_sort_order =
				if state.config.preserve_thread_order && matches!(active.timeline_type, TimelineType::Thread { .. }) {
					SortOrder::OldestToNewest
				} else {
					state.config.sort_order
				};
			let node_id =
				crate::ui::timeline_view::list_index_to_entry_index(0, active.entries.len(), effective_sort_order)
					.map(|entry_index| crate::ui::timeline_view::entry_id_to_node_id(active.entries[entry_index].id()));
			timeline_list.set_selection(node_id);

			sync_timeline_selection_from_list(active, timeline_list, effective_sort_order);
		}
	}
}

pub(super) fn load_more(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	let live_region = ctx.live_region;
	if let Some(active) = state.timeline_manager.active_mut()
		&& !active.entries.is_empty()
		&& active.timeline_type.supports_paging()
	{
		if active.loading_more {
			if active.loading_more_in_background {
				active.loading_more_in_background = false;
			}
			return;
		}

		if state.config.sort_order == SortOrder::OldestToNewest {
			active.loading_more_in_background = true;
		}

		let now = Instant::now();
		let can_load = active.last_load_attempt.is_none_or(|last| now.duration_since(last) > Duration::from_secs(1));
		if can_load {
			active.loading_more = true;
			active.last_load_attempt = Some(now);
			if let Some(handle) = &state.network_handle {
				// Search timelines use offset-based pagination
				if let TimelineType::Search { ref query, search_type } = active.timeline_type {
					handle.send(NetworkCommand::Search {
						query: query.clone(),
						search_type,
						limit: Some(u32::from(state.config.fetch_limit)),
						offset: Some(u32::try_from(active.entries.len()).unwrap()),
					});
				} else {
					let max_id = active.next_max_id.clone().or_else(|| paging_max_id(&active.entries));
					if let Some(max_id) = max_id {
						// Regular timelines use max_id pagination
						handle.send(NetworkCommand::FetchTimeline {
							timeline_type: active.timeline_type.clone(),
							limit: Some(u32::from(state.config.fetch_limit)),
							max_id: Some(max_id),
						});
					} else {
						active.loading_more = false;
						live_region.announce("No more posts available");
					}
				}
			}
		}
	}
}

pub(super) fn switch_timeline_by_index(ctx: &mut UiCommandContext<'_>, index: usize) {
	let state = &mut *ctx.state;
	let live_region = ctx.live_region;
	if index < state.timeline_manager.len() {
		if let Some(name) = state.timeline_manager.display_names().get(index) {
			live_region.announce(name);
		}
		handle_ui_command(UiCommand::TimelineSelectionChanged(index), ctx);
	} else {
		live_region.announce("No timeline at this position");
	}
}

pub(super) fn timeline_selection_changed(ctx: &mut UiCommandContext<'_>, index: usize) {
	let state = &mut *ctx.state;
	let frame = ctx.frame;
	let timelines_selector = ctx.timelines_selector;
	let timeline_list = &ctx.timeline_list;
	let suppress_selection = ctx.suppress_selection;
	if index < state.timeline_manager.len() {
		if let Some(active) = state.timeline_manager.active_mut() {
			let effective_sort_order = active.effective_sort_order(&state.config);
			sync_timeline_selection_from_list(active, timeline_list, effective_sort_order);
		}
		state.timeline_manager.set_active(index);
		update_window_title(state, frame);
		let current_selection = timelines_selector.get_selection().map(|s| s as usize);
		if current_selection != Some(index) {
			with_suppressed_selection(suppress_selection, || {
				timelines_selector.set_selection(u32::try_from(index).unwrap(), true);
			});
		}
		{
			let view_options =
				state.timeline_manager.active().map(|a| state.timeline_view_options_for(&a.timeline_type));
			let active_index = state.timeline_manager.active_index();
			if let Some(view_options) = view_options
				&& let Some(active) = state.timeline_manager.active_mut()
			{
				update_active_timeline_ui(
					timeline_list,
					active,
					suppress_selection,
					&view_options,
					&state.cw_expanded,
					active_index,
				);
			}
		}
		if let Some(mb) = frame.get_menu_bar() {
			update_menu_labels(&mb, state);
		}
	}
}

pub(super) fn timeline_entry_selection_changed(ctx: &mut UiCommandContext<'_>, index: usize) {
	let state = &mut *ctx.state;
	let frame = ctx.frame;
	if let Some(active) = state.timeline_manager.active_mut() {
		let effective_sort_order = active.effective_sort_order(&state.config);
		active.selected_index = Some(index);
		active.selected_id = list_index_to_entry_index(index, active.entries.len(), effective_sort_order)
			.map(|entry_index| active.entries[entry_index].id().to_string());
	}
	if let Some(mb) = frame.get_menu_bar() {
		update_menu_labels(&mb, state);
	}
}

pub(super) fn switch_next_timeline(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	let live_region = ctx.live_region;
	if state.timeline_manager.len() <= 1 {
		return;
	}
	let current = state.timeline_manager.active_index();
	let next = (current + 1) % state.timeline_manager.len();
	if let Some(name) = state.timeline_manager.display_names().get(next) {
		live_region.announce(name);
	}
	handle_ui_command(UiCommand::TimelineSelectionChanged(next), ctx);
}

pub(super) fn switch_prev_timeline(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	let live_region = ctx.live_region;
	if state.timeline_manager.len() <= 1 {
		return;
	}
	let current = state.timeline_manager.active_index();
	let prev = (current + state.timeline_manager.len() - 1) % state.timeline_manager.len();
	if let Some(name) = state.timeline_manager.display_names().get(prev) {
		live_region.announce(name);
	}
	handle_ui_command(UiCommand::TimelineSelectionChanged(prev), ctx);
}

pub(super) fn move_timeline_left(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	let timelines_selector = ctx.timelines_selector;
	let suppress_selection = ctx.suppress_selection;
	let live_region = ctx.live_region;
	if let Some(new_index) = state.timeline_manager.move_active_left() {
		timelines_selector.clear();
		let display_names = state.timeline_manager.display_names();
		for name in &display_names {
			timelines_selector.append(name);
		}
		with_suppressed_selection(suppress_selection, || {
			timelines_selector.set_selection(u32::try_from(new_index).unwrap(), true);
		});
		if let Some(name) = display_names.get(new_index) {
			let msg = if new_index == 0 && display_names.len() > 1 {
				format!("Moved before {}", display_names[1])
			} else if new_index == display_names.len() - 1 && display_names.len() > 1 {
				format!("Moved after {}", display_names[new_index - 1])
			} else if new_index > 0 && new_index < display_names.len() - 1 {
				format!("Moved between {} and {}", display_names[new_index - 1], display_names[new_index + 1])
			} else {
				format!("Moved {name}")
			};
			live_region.announce(&msg);
		}
	} else {
		live_region.announce("Cannot move left");
	}
}

pub(super) fn move_timeline_right(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	let timelines_selector = ctx.timelines_selector;
	let suppress_selection = ctx.suppress_selection;
	let live_region = ctx.live_region;
	if let Some(new_index) = state.timeline_manager.move_active_right() {
		timelines_selector.clear();
		let display_names = state.timeline_manager.display_names();
		for name in &display_names {
			timelines_selector.append(name);
		}
		with_suppressed_selection(suppress_selection, || {
			timelines_selector.set_selection(u32::try_from(new_index).unwrap(), true);
		});
		if let Some(name) = display_names.get(new_index) {
			let msg = if new_index == 0 && display_names.len() > 1 {
				format!("Moved before {}", display_names[1])
			} else if new_index == display_names.len() - 1 && display_names.len() > 1 {
				format!("Moved after {}", display_names[new_index - 1])
			} else if new_index > 0 && new_index < display_names.len() - 1 {
				format!("Moved between {} and {}", display_names[new_index - 1], display_names[new_index + 1])
			} else {
				format!("Moved {name}")
			};
			live_region.announce(&msg);
		}
	} else {
		live_region.announce("Cannot move right");
	}
}

pub(super) fn open_instance_timeline_by_input(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	let frame = ctx.frame;
	let timelines_selector = ctx.timelines_selector;
	let timeline_list = &ctx.timeline_list;
	let suppress_selection = ctx.suppress_selection;
	let live_region = ctx.live_region;
	let dialog =
		TextEntryDialog::builder(frame, "Enter instance domain (e.g. mastodon.social):", "Open Instance Timeline")
			.build();
	if dialog.show_modal() == ID_OK {
		let input = dialog.get_value().unwrap_or_default().trim().to_string();
		if !input.is_empty() {
			let instance =
				input.trim_start_matches("https://").trim_start_matches("http://").trim_end_matches('/').to_string();
			let timeline_type = TimelineType::InstanceLocal { instance };
			open_timeline(
				state,
				timelines_selector,
				timeline_list,
				&timeline_type,
				suppress_selection,
				live_region,
				frame,
			);
		}
	}
}

pub(super) fn view_thread(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	let frame = ctx.frame;
	let timelines_selector = ctx.timelines_selector;
	let timeline_list = &ctx.timeline_list;
	let suppress_selection = ctx.suppress_selection;
	let live_region = ctx.live_region;
	let ui_tx = ctx.ui_tx;
	let entry = if let Some(e) = get_selected_entry(state) {
		e.clone()
	} else {
		live_region.announce("No item selected");
		return;
	};
	match &entry {
		TimelineEntry::Account(account) => {
			if let Some(url) = foreign_url(state, Some(&account.url)) {
				state.pending_user_lookup_action = Some(dialogs::UserLookupAction::Profile);
				if let Some(net) = &state.network_handle {
					net.send(NetworkCommand::ResolveAccount { url });
				}
				return;
			}

			if let Some(net) = &state.network_handle {
				net.send(NetworkCommand::FetchRelationship { account_id: account.id.clone() });
				net.send(NetworkCommand::FetchAccount { account_id: account.id.clone() });
				let net_tx = net.command_tx.clone();
				let ui_tx_timeline = ui_tx.clone();
				let timeline_type =
					TimelineType::User { id: account.id.clone(), name: account.display_name_or_username().to_string() };
				let ui_tx_close = ui_tx.clone();
				let dlg = dialogs::ProfileDialog::new(
					frame,
					account.clone(),
					state.current_user_id.as_deref(),
					net_tx,
					ui_tx.clone(),
					move || {
						let _ = ui_tx_timeline.send(UiCommand::OpenTimeline(timeline_type.clone()));
					},
					move || {
						let _ = ui_tx_close.send(UiCommand::ProfileDialogClosed);
					},
				);
				dlg.show();
				state.profile_dialog = Some(dlg);
			} else {
				live_region.announce("Network not available");
			}
		}
		TimelineEntry::Hashtag(tag) => {
			let timeline_type = TimelineType::Hashtag { name: tag.name.clone() };
			open_timeline(
				state,
				timelines_selector,
				timeline_list,
				&timeline_type,
				suppress_selection,
				live_region,
				frame,
			);
			if let Some(handle) = &state.network_handle {
				handle.send(NetworkCommand::FetchTimeline { timeline_type, limit: Some(40), max_id: None });
			}
		}
		TimelineEntry::Notification(notification) if notification.kind == "follow_request" => {
			let Some(handle) = &state.network_handle else {
				live_region.announce("Network not available");
				return;
			};
			let actor = notification.account.display_name_or_username().to_string();
			let prompt = format!(
				"{} (@{}) requested to follow you.\r\n\r\nYes: Accept\r\nNo: Reject\r\nCancel: Keep pending",
				actor, notification.account.acct
			);
			let dialog = MessageDialog::builder(frame, &prompt, "Follow Request")
				.with_style(MessageDialogStyle::YesNo | MessageDialogStyle::Cancel | MessageDialogStyle::IconQuestion)
				.build();
			match dialog.show_modal() {
				ID_YES => handle.send(NetworkCommand::AuthorizeFollowRequest {
					account_id: notification.account.id.clone(),
					target_name: actor,
				}),
				ID_NO => handle.send(NetworkCommand::RejectFollowRequest {
					account_id: notification.account.id.clone(),
					target_name: actor,
				}),
				_ => {
					live_region.announce("Follow request unchanged");
				}
			}
		}
		TimelineEntry::Status(_) | TimelineEntry::Notification(_) => {
			let Some(status) = entry.as_status() else {
				live_region.announce("No post to view");
				return;
			};
			let target = status.reblog.as_ref().map_or(status, std::convert::AsRef::as_ref);

			if let Some(url) = foreign_url(state, target.url.as_ref()) {
				if let Some(net) = &state.network_handle {
					net.send(NetworkCommand::ResolveStatusForThread { url });
				}
				return;
			}

			let name = format!("Thread: {}", target.account.display_name_or_username());
			let timeline_type = TimelineType::Thread { id: target.id.clone(), name };
			state.pending_restore_post_id = Some((timeline_type.clone(), target.id.clone()));
			open_timeline(
				state,
				timelines_selector,
				timeline_list,
				&timeline_type,
				suppress_selection,
				live_region,
				frame,
			);
			let Some(handle) = &state.network_handle else {
				live_region.announce("Network not available");
				return;
			};
			handle.send(NetworkCommand::FetchThread { timeline_type, focus: Box::new(target.clone()) });
		}
	}
}

pub(super) fn view_resolved_thread(ctx: &mut UiCommandContext<'_>, focus: Box<Status>) {
	let state = &mut *ctx.state;
	let frame = ctx.frame;
	let timelines_selector = ctx.timelines_selector;
	let timeline_list = &ctx.timeline_list;
	let suppress_selection = ctx.suppress_selection;
	let live_region = ctx.live_region;
	let name = format!("Thread: {}", focus.account.display_name_or_username());
	let timeline_type = TimelineType::Thread { id: focus.id.clone(), name };
	state.pending_restore_post_id = Some((timeline_type.clone(), focus.id.clone()));
	open_timeline(state, timelines_selector, timeline_list, &timeline_type, suppress_selection, live_region, frame);
	if let Some(handle) = &state.network_handle {
		handle.send(NetworkCommand::FetchThread { timeline_type, focus });
	}
}

pub(super) fn view_quoted_thread(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	let frame = ctx.frame;
	let timelines_selector = ctx.timelines_selector;
	let timeline_list = &ctx.timeline_list;
	let suppress_selection = ctx.suppress_selection;
	let live_region = ctx.live_region;
	let quoted_info = get_selected_status(state).map_or_else(
		|| {
			live_region.announce("No post selected");
			None
		},
		|status| {
			let target = status.reblog.as_ref().map_or(status, std::convert::AsRef::as_ref);
			if let Some(quote) = &target.quote
				&& let Some(quoted_status) = &quote.quoted_status
			{
				let name = format!("Thread: {}", quoted_status.account.display_name_or_username());
				let timeline_type = TimelineType::Thread { id: quoted_status.id.clone(), name };
				Some((timeline_type, *quoted_status.clone()))
			} else {
				live_region.announce("No quoted post");
				None
			}
		},
	);

	if let Some((timeline_type, focus_status)) = quoted_info {
		state.pending_restore_post_id = Some((timeline_type.clone(), focus_status.id.clone()));
		open_timeline(state, timelines_selector, timeline_list, &timeline_type, suppress_selection, live_region, frame);
		let Some(handle) = &state.network_handle else {
			live_region.announce("Network not available");
			return;
		};
		handle.send(NetworkCommand::FetchThread { timeline_type, focus: Box::new(focus_status) });
	}
}

pub(super) fn search(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	let frame = ctx.frame;
	let timelines_selector = ctx.timelines_selector;
	let timeline_list = &ctx.timeline_list;
	let suppress_selection = ctx.suppress_selection;
	let live_region = ctx.live_region;
	if let Some((query, search_type)) = dialogs::prompt_for_search(frame) {
		let timeline_type = TimelineType::Search { query: query.clone(), search_type };
		open_timeline(state, timelines_selector, timeline_list, &timeline_type, suppress_selection, live_region, frame);
		if let Some(handle) = &state.network_handle {
			handle.send(NetworkCommand::Search { query, search_type, limit: Some(40), offset: None });
		}
	}
}

pub(super) fn open_list(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	let live_region = ctx.live_region;
	if let Some(handle) = &state.network_handle {
		handle.send(NetworkCommand::FetchLists);
	} else {
		live_region.announce("Network not available");
	}
}
