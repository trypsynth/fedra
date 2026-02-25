use std::cell::Cell;

use wxdragon::prelude::*;

use crate::{
	AppState, UiCommand,
	config::{AutoloadMode, ConfigStore, SortOrder},
	mastodon::{Poll, Status},
	network::{NetworkCommand, NetworkResponse, TimelineData},
	streaming,
	timeline::{TimelineEntry, TimelineType},
	ui::{
		dialogs::{self, UserLookupAction},
		menu::update_menu_labels,
		timeline_view::{sync_timeline_selection_from_list, update_active_timeline_ui},
	},
	ui_wake::UiCommandSender,
};

fn summarize_api_error(err: &anyhow::Error) -> String {
	for cause in err.chain().skip(1) {
		let mut message = cause.to_string();
		if message.trim().is_empty() {
			continue;
		}
		if message.starts_with("HTTP status")
			&& let Some((head, _)) = message.split_once(" for url")
		{
			message = head.to_string();
		}
		return message;
	}
	err.to_string()
}

fn spoken_failure(prefix: &str, err: &anyhow::Error) -> String {
	format!("{prefix}: {}", summarize_api_error(err))
}

fn merge_status_snapshot_by_id(state: &mut AppState, status_id: &str, snapshot: &Status) -> bool {
	let mut updated = false;
	for timeline in state.timeline_manager.iter_mut() {
		for entry in &mut timeline.entries {
			if let Some(status) = entry.as_status_mut() {
				if status.id == status_id {
					*status = snapshot.clone();
					updated = true;
				}
				if let Some(ref mut reblog) = status.reblog
					&& reblog.id == status_id
				{
					**reblog = snapshot.clone();
					updated = true;
				}
			}
		}
	}
	updated
}

fn merge_status_snapshot(state: &mut AppState, snapshot: &Status) -> bool {
	let mut updated = merge_status_snapshot_by_id(state, &snapshot.id, snapshot);
	if let Some(reblog) = &snapshot.reblog {
		updated |= merge_status_snapshot_by_id(state, &reblog.id, reblog);
	}
	updated
}

/// Processes streaming events from WebSocket connections.
pub fn process_stream_events(
	state: &mut AppState,
	timeline_list: ListBox,
	suppress_selection: &Cell<bool>,
	frame: &Frame,
) {
	let active_type = state.timeline_manager.active().map(|t| t.timeline_type.clone());
	let mut active_needs_update = false;
	let mut processed_notification_ids = std::collections::HashSet::new();
	let mut status_snapshots: Vec<Status> = Vec::new();

	for timeline in state.timeline_manager.iter_mut() {
		let Some(handle) = &timeline.stream_handle else { continue };
		let events = handle.drain();
		let is_active = active_type.as_ref() == Some(&timeline.timeline_type);
		if is_active {
			let effective_sort_order = if state.config.preserve_thread_order
				&& matches!(timeline.timeline_type, TimelineType::Thread { .. })
			{
				SortOrder::OldestToNewest
			} else {
				state.config.sort_order
			};
			sync_timeline_selection_from_list(timeline, timeline_list, effective_sort_order);
		}
		for event in events {
			match event {
				streaming::StreamEvent::Update { timeline_type, status } => {
					status_snapshots.push((*status).clone());
					if timeline.timeline_type == timeline_type
						&& !status.should_hide(&timeline.timeline_type.filter_context())
					{
						timeline.entries.insert(0, TimelineEntry::Status(*status));
						if is_active {
							active_needs_update = true;
						}
					}
				}
				streaming::StreamEvent::Delete { timeline_type, id } => {
					if timeline.timeline_type == timeline_type {
						timeline.entries.retain(|entry| entry.as_status().is_none_or(|s| s.id != id));
						if is_active {
							active_needs_update = true;
						}
					}
				}
				streaming::StreamEvent::Notification { timeline_type, notification } => {
					if let Some(status) = notification.status.as_deref() {
						status_snapshots.push(status.clone());
					}
					if timeline.timeline_type == timeline_type {
						if !processed_notification_ids.contains(&notification.id) {
							let pref = state.config.notification_preference;
							match pref {
								crate::config::NotificationPreference::Classic => {
									if let Some(app_shell) = &state.app_shell {
										crate::notifications::show_notification(app_shell, &notification);
									}
								}
								crate::config::NotificationPreference::SoundOnly => {
									if let Some(mc) = &state.media_ctrl {
										mc.stop();
										mc.play();
									}
								}
								crate::config::NotificationPreference::Disabled => {}
							}
							processed_notification_ids.insert(notification.id.clone());
						}
						if notification
							.status
							.as_ref()
							.is_none_or(|s| !s.should_hide(&timeline.timeline_type.filter_context()))
						{
							timeline.entries.insert(0, TimelineEntry::Notification(*notification));
							if is_active {
								active_needs_update = true;
							}
						}
					}
				}
				streaming::StreamEvent::Conversation { timeline_type, conversation } => {
					if timeline.timeline_type == timeline_type
						&& let Some(status) = conversation.last_status
						&& !status.should_hide(&timeline.timeline_type.filter_context())
					{
						status_snapshots.push(status.clone());
						timeline.entries.insert(0, TimelineEntry::Status(status));
						if is_active {
							active_needs_update = true;
						}
					}
				}
				streaming::StreamEvent::Connected(timeline_type)
				| streaming::StreamEvent::Disconnected(timeline_type) => {
					let _ = timeline_type;
				}
			}
		}
	}
	let mut merged_any = false;
	for snapshot in &status_snapshots {
		if merge_status_snapshot(state, snapshot) {
			merged_any = true;
		}
	}
	if merged_any {
		active_needs_update = true;
	}
	let view_options = state.timeline_manager.active().map(|a| state.timeline_view_options_for(&a.timeline_type));
	if active_needs_update
		&& let Some(view_options) = view_options
		&& let Some(active) = state.timeline_manager.active_mut()
	{
		update_active_timeline_ui(timeline_list, active, suppress_selection, &view_options, &state.cw_expanded);
		if let Some(mb) = frame.get_menu_bar() {
			update_menu_labels(&mb, state);
		}
	}
}

/// Processes network responses from the background network thread.
pub struct NetworkResponseContext<'a> {
	pub frame: &'a Frame,
	pub state: &'a mut AppState,
	pub timelines_selector: ListBox,
	pub timeline_list: ListBox,
	pub suppress_selection: &'a Cell<bool>,
	pub live_region: StaticText,
	pub quick_action_keys_enabled: &'a Cell<bool>,
	pub autoload_mode: &'a Cell<AutoloadMode>,
	pub sort_order_cell: &'a Cell<SortOrder>,
	pub tray_hidden: &'a Cell<bool>,
	pub ui_tx: &'a UiCommandSender,
}

/// Processes network responses from the background network thread.
#[allow(clippy::too_many_lines)]
pub fn process_network_responses(ctx: &mut NetworkResponseContext<'_>) {
	let frame = ctx.frame;
	let state = &mut *ctx.state;
	let timelines_selector = ctx.timelines_selector;
	let timeline_list = ctx.timeline_list;
	let suppress_selection = ctx.suppress_selection;
	let live_region = ctx.live_region;
	let quick_action_keys_enabled = ctx.quick_action_keys_enabled;
	let autoload_mode = ctx.autoload_mode;
	let sort_order_cell = ctx.sort_order_cell;
	let tray_hidden = ctx.tray_hidden;
	let ui_tx = ctx.ui_tx;
	let Some(handle) = &state.network_handle else { return };
	let active_type = state.timeline_manager.active().map(|t| t.timeline_type.clone());
	macro_rules! dispatch_ui_command {
		($cmd:expr) => {{
			let mut ctx = crate::commands::UiCommandContext {
				state,
				frame,
				timelines_selector,
				timeline_list,
				suppress_selection,
				live_region,
				quick_action_keys_enabled,
				autoload_mode,
				sort_order_cell,
				tray_hidden,
				ui_tx,
			};
			crate::commands::handle_ui_command($cmd, &mut ctx);
		}};
	}
	for response in handle.drain() {
		match response {
			NetworkResponse::TimelineLoaded { timeline_type, result: Ok(data), max_id } => {
				let mut should_find_next = false;
				let is_active = active_type.as_ref() == Some(&timeline_type);
				let mut status_snapshots: Vec<Status> = Vec::new();
				let view_options = state.timeline_view_options_for(&timeline_type);
				let text_options = &view_options.text_options;
				if let Some(timeline) = state.timeline_manager.get_mut(&timeline_type) {
					if is_active {
						let effective_sort_order = if state.config.preserve_thread_order
							&& matches!(timeline.timeline_type, TimelineType::Thread { .. })
						{
							SortOrder::OldestToNewest
						} else {
							state.config.sort_order
						};
						sync_timeline_selection_from_list(timeline, timeline_list, effective_sort_order);
					}
					let filter_context = timeline_type.filter_context();
					let (new_entries, next_max_id): (Vec<TimelineEntry>, Option<String>) = match data {
						TimelineData::Statuses(statuses, next) => (
							statuses
								.into_iter()
								.filter(|s| !s.should_hide(&filter_context))
								.map(TimelineEntry::Status)
								.collect(),
							next,
						),
						TimelineData::Notifications(notifications, next) => (
							notifications
								.into_iter()
								.filter(|n| n.status.as_ref().is_none_or(|s| !s.should_hide(&filter_context)))
								.map(TimelineEntry::Notification)
								.collect(),
							next,
						),
						TimelineData::Conversations(conversations, next) => (
							conversations
								.into_iter()
								.filter_map(|c| {
									c.last_status.filter(|s| !s.should_hide(&filter_context)).map(TimelineEntry::Status)
								})
								.collect(),
							next,
						),
					};
					for entry in &new_entries {
						if let Some(status) = entry.as_status() {
							status_snapshots.push(status.clone());
						}
					}

					if max_id.is_some() {
						let existing_ids: std::collections::HashSet<&str> =
							timeline.entries.iter().map(super::timeline::TimelineEntry::id).collect();
						let filtered: Vec<TimelineEntry> =
							new_entries.into_iter().filter(|entry| !existing_ids.contains(entry.id())).collect();
						if filtered.is_empty() {
							live_region::announce(live_region, "No more posts");
						} else {
							timeline.entries.extend(filtered.clone());
						}

						if is_active {
							if state.config.sort_order == SortOrder::NewestToOldest {
								let entries_to_append = if filtered.is_empty() { &[][..] } else { &filtered };
								for entry in entries_to_append {
									let is_expanded = state.cw_expanded.contains(entry.id());
									timeline_list.append(&entry.display_text(text_options, is_expanded));
								}
							} else {
								update_active_timeline_ui(
									timeline_list,
									timeline,
									suppress_selection,
									&view_options,
									&state.cw_expanded,
								);
							}
						}
					} else {
						timeline.entries = new_entries;
						if is_active {
							update_active_timeline_ui(
								timeline_list,
								timeline,
								suppress_selection,
								&view_options,
								&state.cw_expanded,
							);
						}
					}
					timeline.next_max_id = next_max_id;
					timeline.loading_more = false;
					if is_active && timeline.pending_find_next {
						timeline.pending_find_next = false;
						should_find_next = true;
					}
				}
				if !status_snapshots.is_empty() {
					let mut merged_any = false;
					for snapshot in &status_snapshots {
						if merge_status_snapshot(state, snapshot) {
							merged_any = true;
						}
					}
					if merged_any {
						let view_options =
							state.timeline_manager.active().map(|a| state.timeline_view_options_for(&a.timeline_type));
						if let Some(view_options) = view_options
							&& let Some(active) = state.timeline_manager.active_mut()
						{
							update_active_timeline_ui(
								timeline_list,
								active,
								suppress_selection,
								&view_options,
								&state.cw_expanded,
							);
						}
					}
				}
				if should_find_next {
					dispatch_ui_command!(crate::commands::UiCommand::FindNext);
				}
			}
			NetworkResponse::TimelineLoaded { timeline_type, result: Err(ref err), max_id } => {
				if let Some(timeline) = state.timeline_manager.get_mut(&timeline_type) {
					timeline.loading_more = false;
				}
				if max_id.is_some() {
					live_region::announce(live_region, "Failed to load more posts");
				} else {
					live_region::announce(live_region, &spoken_failure("Failed to load timeline", err));
				}
			}
			NetworkResponse::AccountLookupResult { handle: _, result: Ok(account) } => {
				let action = state.pending_user_lookup_action.take().unwrap_or(UserLookupAction::Timeline);
				match action {
					UserLookupAction::Profile => {
						if let Some(net) = &state.network_handle {
							net.send(NetworkCommand::FetchRelationship { account_id: account.id.clone() });
							let net_tx = net.command_tx.clone();
							let ui_tx_timeline = ui_tx.clone();
							let timeline_type = TimelineType::User {
								id: account.id.clone(),
								name: account.display_name_or_username().to_string(),
							};
							let ui_tx_close = ui_tx.clone();

							let dlg = dialogs::ProfileDialog::new(
								frame,
								account.clone(),
								net_tx,
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
							live_region::announce(live_region, "Network not available");
						}
					}
					UserLookupAction::Timeline => {
						let timeline_type = TimelineType::User {
							id: account.id.clone(),
							name: account.display_name_or_username().to_string(),
						};
						dispatch_ui_command!(UiCommand::OpenTimeline(timeline_type));
					}
				}
			}
			NetworkResponse::AccountLookupResult { handle, result: Err(err) } => {
				state.pending_user_lookup_action = None;
				live_region::announce(
					live_region,
					&format!("Failed to find user {handle}: {}", summarize_api_error(&err)),
				);
			}
			NetworkResponse::PostComplete(Ok(status)) => {
				live_region::announce(live_region, "Posted");
				if state.pending_thread_continuation {
					state.pending_thread_continuation = false;
					dispatch_ui_command!(UiCommand::ContinueThread(Box::new(status)));
				}
			}
			NetworkResponse::PostComplete(Err(ref err)) => {
				state.pending_thread_continuation = false;
				live_region::announce(live_region, &spoken_failure("Failed to post", err));
			}
			NetworkResponse::Favorited { status_id, result: Ok(status) } => {
				update_status_in_timelines(state, &status_id, |s| {
					s.favourited = status.favourited;
					s.favourites_count = status.favourites_count;
				});
				if let Some(mb) = frame.get_menu_bar() {
					update_menu_labels(&mb, state);
				}
				live_region::announce(live_region, "Favorited");
			}
			NetworkResponse::Favorited { result: Err(ref err), .. } => {
				live_region::announce(live_region, &spoken_failure("Failed to favorite", err));
			}
			NetworkResponse::Bookmarked { status_id, result: Ok(status) } => {
				update_status_in_timelines(state, &status_id, |s| {
					s.bookmarked = status.bookmarked;
				});
				if let Some(mb) = frame.get_menu_bar() {
					update_menu_labels(&mb, state);
				}
				live_region::announce(live_region, "Bookmarked");
			}
			NetworkResponse::Bookmarked { result: Err(ref err), .. } => {
				live_region::announce(live_region, &spoken_failure("Failed to bookmark", err));
			}
			NetworkResponse::Unfavorited { status_id, result: Ok(status) } => {
				update_status_in_timelines(state, &status_id, |s| {
					s.favourited = status.favourited;
					s.favourites_count = status.favourites_count;
				});
				if let Some(mb) = frame.get_menu_bar() {
					update_menu_labels(&mb, state);
				}
				live_region::announce(live_region, "Unfavorited");
			}
			NetworkResponse::Unfavorited { result: Err(ref err), .. } => {
				live_region::announce(live_region, &spoken_failure("Failed to unfavorite", err));
			}
			NetworkResponse::Unbookmarked { status_id, result: Ok(status) } => {
				update_status_in_timelines(state, &status_id, |s| {
					s.bookmarked = status.bookmarked;
				});
				if let Some(mb) = frame.get_menu_bar() {
					update_menu_labels(&mb, state);
				}
				live_region::announce(live_region, "Unbookmarked");
			}
			NetworkResponse::Unbookmarked { result: Err(ref err), .. } => {
				live_region::announce(live_region, &spoken_failure("Failed to unbookmark", err));
			}
			NetworkResponse::Boosted { status_id, result: Ok(status) } => {
				// The returned status is the reblog wrapper, get the inner status
				if let Some(inner) = &status.reblog {
					update_status_in_timelines(state, &status_id, |s| {
						s.reblogged = inner.reblogged;
						s.reblogs_count = inner.reblogs_count;
					});
				}
				if let Some(mb) = frame.get_menu_bar() {
					update_menu_labels(&mb, state);
				}
				live_region::announce(live_region, "Boosted");
			}
			NetworkResponse::Boosted { result: Err(ref err), .. } => {
				live_region::announce(live_region, &spoken_failure("Failed to boost", err));
			}
			NetworkResponse::Unboosted { status_id, result: Ok(status) } => {
				update_status_in_timelines(state, &status_id, |s| {
					s.reblogged = status.reblogged;
					s.reblogs_count = status.reblogs_count;
				});
				if let Some(mb) = frame.get_menu_bar() {
					update_menu_labels(&mb, state);
				}
				live_region::announce(live_region, "Unboosted");
			}
			NetworkResponse::Unboosted { result: Err(ref err), .. } => {
				live_region::announce(live_region, &spoken_failure("Failed to unboost", err));
			}
			NetworkResponse::Replied(Ok(status)) => {
				live_region::announce(live_region, "Reply sent");
				if state.pending_thread_continuation {
					state.pending_thread_continuation = false;
					dispatch_ui_command!(UiCommand::ContinueThread(Box::new(status)));
				}
			}
			NetworkResponse::Replied(Err(ref err)) => {
				state.pending_thread_continuation = false;
				live_region::announce(live_region, &spoken_failure("Failed to reply", err));
			}
			NetworkResponse::StatusDeleted { status_id, result: Ok(()) } => {
				remove_status_from_timelines(state, &status_id);
				{
					let view_options =
						state.timeline_manager.active().map(|a| state.timeline_view_options_for(&a.timeline_type));
					if let Some(view_options) = view_options
						&& let Some(active) = state.timeline_manager.active_mut()
					{
						update_active_timeline_ui(
							timeline_list,
							active,
							suppress_selection,
							&view_options,
							&state.cw_expanded,
						);
					}
				}
				live_region::announce(live_region, "Deleted");
			}
			NetworkResponse::StatusDeleted { result: Err(ref err), .. } => {
				live_region::announce(live_region, &spoken_failure("Failed to delete", err));
			}
			NetworkResponse::StatusEdited { _status_id: _, result: Ok(status) } => {
				let status_clone = status.clone();
				update_status_in_timelines(state, &status.id, move |s| *s = status_clone.clone());
				{
					let view_options =
						state.timeline_manager.active().map(|a| state.timeline_view_options_for(&a.timeline_type));
					if let Some(view_options) = view_options
						&& let Some(active) = state.timeline_manager.active_mut()
					{
						update_active_timeline_ui(
							timeline_list,
							active,
							suppress_selection,
							&view_options,
							&state.cw_expanded,
						);
					}
				}
				live_region::announce(live_region, "Edited");
			}
			NetworkResponse::StatusEdited { result: Err(ref err), .. } => {
				live_region::announce(live_region, &spoken_failure("Failed to edit", err));
			}
			NetworkResponse::TagFollowed { name, result: Ok(_) } => {
				update_tag_in_timelines(state, &name, true);
				if let Some(dlg) = &state.hashtag_dialog {
					dlg.update_tag(&name, true);
				}
				live_region::announce(live_region, &format!("Followed #{name}"));
			}
			NetworkResponse::TagFollowed { name, result: Err(err) } => {
				live_region::announce(live_region, &format!("Failed to follow #{name}: {}", summarize_api_error(&err)));
			}
			NetworkResponse::TagUnfollowed { name, result: Ok(_) } => {
				update_tag_in_timelines(state, &name, false);
				if let Some(dlg) = &state.hashtag_dialog {
					dlg.update_tag(&name, false);
				}
				live_region::announce(live_region, &format!("Unfollowed #{name}"));
			}
			NetworkResponse::TagUnfollowed { name, result: Err(err) } => {
				live_region::announce(
					live_region,
					&format!("Failed to unfollow #{name}: {}", summarize_api_error(&err)),
				);
			}
			NetworkResponse::TagsInfoFetched { result: Ok(tags) } => {
				if let Some(handle) = &state.network_handle {
					let net_tx = handle.command_tx.clone();
					let ui_tx_dlg = ui_tx.clone();
					let dlg = dialogs::HashtagDialog::new(frame, tags, net_tx, move || {
						let _ = ui_tx_dlg.send(UiCommand::HashtagDialogClosed);
					});
					dlg.show();
					state.hashtag_dialog = Some(dlg);
				}
			}
			NetworkResponse::TagsInfoFetched { result: Err(err) } => {
				live_region::announce(live_region, &spoken_failure("Failed to load hashtags", &err));
			}
			NetworkResponse::RebloggedByLoaded { result: Ok(accounts), .. } => {
				if let Some((account, action)) =
					dialogs::prompt_for_account_list(frame, "Boosts", "Users who boosted this post", &accounts)
				{
					match action {
						UserLookupAction::Profile => {
							if let Some(net) = &state.network_handle {
								net.send(NetworkCommand::FetchRelationship { account_id: account.id.clone() });
								let net_tx = net.command_tx.clone();
								let ui_tx_timeline = ui_tx.clone();
								let timeline_type = TimelineType::User {
									id: account.id.clone(),
									name: account.display_name_or_username().to_string(),
								};
								let ui_tx_close = ui_tx.clone();
								let dlg = dialogs::ProfileDialog::new(
									frame,
									account.clone(),
									net_tx,
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
								live_region::announce(live_region, "Network not available");
							}
						}
						UserLookupAction::Timeline => {
							let timeline_type = TimelineType::User {
								id: account.id.clone(),
								name: account.display_name_or_username().to_string(),
							};
							dispatch_ui_command!(UiCommand::OpenTimeline(timeline_type));
						}
					}
				}
			}
			NetworkResponse::RebloggedByLoaded { result: Err(err), .. } => {
				live_region::announce(live_region, &spoken_failure("Failed to load boosts", &err));
			}
			NetworkResponse::FavoritedByLoaded { result: Ok(accounts), .. } => {
				if let Some((account, action)) =
					dialogs::prompt_for_account_list(frame, "Favorites", "Users who favorited this post", &accounts)
				{
					match action {
						UserLookupAction::Profile => {
							if let Some(net) = &state.network_handle {
								net.send(NetworkCommand::FetchRelationship { account_id: account.id.clone() });
								let net_tx = net.command_tx.clone();
								let ui_tx_timeline = ui_tx.clone();
								let timeline_type = TimelineType::User {
									id: account.id.clone(),
									name: account.display_name_or_username().to_string(),
								};
								let ui_tx_close = ui_tx.clone();
								let dlg = dialogs::ProfileDialog::new(
									frame,
									account.clone(),
									net_tx,
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
								live_region::announce(live_region, "Network not available");
							}
						}
						UserLookupAction::Timeline => {
							let timeline_type = TimelineType::User {
								id: account.id.clone(),
								name: account.display_name_or_username().to_string(),
							};
							dispatch_ui_command!(UiCommand::OpenTimeline(timeline_type));
						}
					}
				}
			}
			NetworkResponse::FavoritedByLoaded { result: Err(err), .. } => {
				live_region::announce(live_region, &spoken_failure("Failed to load favorites", &err));
			}
			NetworkResponse::FollowersLoaded { result: Ok(accounts), .. } => {
				if accounts.is_empty() {
					live_region::announce(live_region, "No followers found");
					continue;
				}
				if let Some(account) =
					dialogs::prompt_for_follow_list(frame, "Followers", "Users who follow this person:", &accounts)
				{
					let timeline_type = TimelineType::User {
						id: account.id.clone(),
						name: account.display_name_or_username().to_string(),
					};
					dispatch_ui_command!(UiCommand::OpenTimeline(timeline_type));
				}
			}
			NetworkResponse::FollowersLoaded { result: Err(err), .. } => {
				live_region::announce(live_region, &spoken_failure("Failed to load followers", &err));
			}
			NetworkResponse::FollowingLoaded { result: Ok(accounts), .. } => {
				if accounts.is_empty() {
					live_region::announce(live_region, "No following found");
					continue;
				}
				if let Some(account) =
					dialogs::prompt_for_follow_list(frame, "Following", "Users this person follows:", &accounts)
				{
					let timeline_type = TimelineType::User {
						id: account.id.clone(),
						name: account.display_name_or_username().to_string(),
					};
					dispatch_ui_command!(UiCommand::OpenTimeline(timeline_type));
				}
			}
			NetworkResponse::FollowingLoaded { result: Err(err), .. } => {
				live_region::announce(live_region, &spoken_failure("Failed to load following", &err));
			}
			NetworkResponse::RelationshipUpdated { _account_id: _, target_name, action, result } => match result {
				Ok(rel) => {
					if let Some(dlg) = &state.profile_dialog {
						dlg.update_relationship(&rel);
					}
					let msg = match action {
						crate::network::RelationshipAction::Follow => format!("Followed {target_name}"),
						crate::network::RelationshipAction::Unfollow => format!("Unfollowed {target_name}"),
						crate::network::RelationshipAction::Block => format!("Blocked {target_name}"),
						crate::network::RelationshipAction::Unblock => format!("Unblocked {target_name}"),
						crate::network::RelationshipAction::Mute => format!("Muted {target_name}"),
						crate::network::RelationshipAction::Unmute => format!("Unmuted {target_name}"),
						crate::network::RelationshipAction::ShowBoosts => {
							format!("Showing boosts from {target_name}")
						}
						crate::network::RelationshipAction::HideBoosts => format!("Hiding boosts from {target_name}"),
					};
					live_region::announce(live_region, &msg);
				}
				Err(err) => {
					live_region::announce(live_region, &spoken_failure("Failed to update relationship", &err));
				}
			},
			NetworkResponse::RelationshipLoaded { _account_id: _, result } => {
				if let Ok(rel) = result
					&& let Some(dlg) = &state.profile_dialog
				{
					dlg.update_relationship(&rel);
				}
			}
			NetworkResponse::AccountFetched { result } => {
				if let Ok(account) = result
					&& let Some(dlg) = &state.profile_dialog
				{
					dlg.update_account(&account);
				}
			}
			NetworkResponse::PollVoted { result } => match result {
				Ok(poll) => {
					update_poll_in_timelines(state, &poll);
					let view_options =
						state.timeline_manager.active().map(|a| state.timeline_view_options_for(&a.timeline_type));
					if let Some(view_options) = view_options
						&& let Some(active) = state.timeline_manager.active_mut()
					{
						update_active_timeline_ui(
							timeline_list,
							active,
							suppress_selection,
							&view_options,
							&state.cw_expanded,
						);
					}
					live_region::announce(live_region, "Vote submitted");
				}
				Err(err) => {
					live_region::announce(live_region, &spoken_failure("Failed to vote", &err));
				}
			},
			NetworkResponse::CredentialsFetched { result: Ok(account) } => {
				if let Some(update) = dialogs::prompt_for_profile_edit(frame, &account)
					&& let Some(handle) = &state.network_handle
				{
					handle.send(NetworkCommand::UpdateProfile { update });
				}
			}
			NetworkResponse::CredentialsFetched { result: Err(err) } => {
				live_region::announce(live_region, &spoken_failure("Failed to fetch profile", &err));
			}
			NetworkResponse::ProfileUpdated { result: Ok(account) } => {
				live_region::announce(live_region, "Profile updated");
				if let Some(active) = state.active_account_mut() {
					active.default_post_visibility = account.source.and_then(|s| s.privacy);
				}
				let _ = ConfigStore::new().save(&state.config);
				let _ = ui_tx.send(UiCommand::Refresh);
			}
			NetworkResponse::ProfileUpdated { result: Err(err) } => {
				live_region::announce(live_region, &spoken_failure("Failed to update profile", &err));
			}
			NetworkResponse::SearchLoaded { query, search_type, result: Ok(results), offset } => {
				if let Some(dlg) = &state.manage_list_members_dialog
					&& matches!(search_type, crate::mastodon::SearchType::Accounts)
				{
					let labels: Vec<String> = results
						.accounts
						.iter()
						.map(|a| format!("{}: @{}", a.display_name_or_username(), a.acct))
						.collect();
					let label_refs: Vec<&str> = labels.iter().map(String::as_str).collect();
					let accounts_ref: Vec<&crate::mastodon::Account> = results.accounts.iter().collect();

					if let Some(account) = dialogs::prompt_for_account_choice(frame, &accounts_ref, &label_refs)
						&& let Some(handle) = &state.network_handle
					{
						handle.send(NetworkCommand::AddListAccount {
							list_id: dlg.get_list_id().to_string(),
							account_id: account.id,
						});
					}
					return;
				}

				let timeline_type = TimelineType::Search { query: query.clone(), search_type };
				let is_active = active_type.as_ref() == Some(&timeline_type);
				let mut status_snapshots: Vec<Status> = Vec::new();
				let view_options = state.timeline_view_options_for(&timeline_type);
				let text_options = &view_options.text_options;
				if let Some(timeline) = state.timeline_manager.get_mut(&timeline_type) {
					if is_active {
						let effective_sort_order = if state.config.preserve_thread_order
							&& matches!(timeline.timeline_type, TimelineType::Thread { .. })
						{
							SortOrder::OldestToNewest
						} else {
							state.config.sort_order
						};
						sync_timeline_selection_from_list(timeline, timeline_list, effective_sort_order);
					}
					let mut new_entries: Vec<TimelineEntry> = Vec::new();
					for account in results.accounts {
						new_entries.push(TimelineEntry::Account(account));
					}
					for hashtag in results.hashtags {
						new_entries.push(TimelineEntry::Hashtag(hashtag));
					}
					for status in results.statuses {
						new_entries.push(TimelineEntry::Status(status));
					}
					for entry in &new_entries {
						if let Some(status) = entry.as_status() {
							status_snapshots.push(status.clone());
						}
					}
					let is_load_more = offset.is_some() && offset.unwrap_or(0) > 0;
					if is_load_more {
						if new_entries.is_empty() {
							live_region::announce(live_region, "No more results");
						} else {
							timeline.entries.extend(new_entries.clone());
							if is_active {
								for entry in &new_entries {
									let is_expanded = state.cw_expanded.contains(entry.id());
									timeline_list.append(&entry.display_text(text_options, is_expanded));
								}
							}
						}
					} else {
						timeline.entries = new_entries;
						if is_active {
							update_active_timeline_ui(
								timeline_list,
								timeline,
								suppress_selection,
								&view_options,
								&state.cw_expanded,
							);
						}
					}
					timeline.loading_more = false;
				}
				if !status_snapshots.is_empty() {
					let mut merged_any = false;
					for snapshot in &status_snapshots {
						if merge_status_snapshot(state, snapshot) {
							merged_any = true;
						}
					}
					if merged_any {
						let view_options =
							state.timeline_manager.active().map(|a| state.timeline_view_options_for(&a.timeline_type));
						if let Some(view_options) = view_options
							&& let Some(active) = state.timeline_manager.active_mut()
						{
							update_active_timeline_ui(
								timeline_list,
								active,
								suppress_selection,
								&view_options,
								&state.cw_expanded,
							);
						}
					}
				}
			}
			NetworkResponse::SearchLoaded { query, search_type, result: Err(ref err), .. } => {
				let timeline_type = TimelineType::Search { query: query.clone(), search_type };
				if let Some(timeline) = state.timeline_manager.get_mut(&timeline_type) {
					timeline.loading_more = false;
				}
				live_region::announce(
					live_region,
					&format!("Search for '{query}' failed: {}", summarize_api_error(err)),
				);
			}
			NetworkResponse::ListsFetched { result: Ok(lists) } => {
				if let Some(dlg) = &state.manage_lists_dialog {
					dlg.update_lists(lists);
				} else if lists.is_empty() {
					live_region::announce(live_region, "No lists found");
				} else if let Some(list) = dialogs::prompt_for_list_selection(frame, &lists) {
					let timeline_type = TimelineType::List { id: list.id, title: list.title };
					dispatch_ui_command!(UiCommand::OpenTimeline(timeline_type));
				}
			}
			NetworkResponse::ListsFetched { result: Err(err) } => {
				live_region::announce(live_region, &spoken_failure("Failed to fetch lists", &err));
			}
			NetworkResponse::ListCreated { result: Ok(list) } => {
				live_region::announce(live_region, &format!("List '{}' created", list.title));
				if let Some(handle) = &state.network_handle {
					handle.send(NetworkCommand::FetchLists);
				}
			}
			NetworkResponse::ListCreated { result: Err(err) }
			| NetworkResponse::ListUpdated { result: Err(err) }
			| NetworkResponse::ListDeleted { result: Err(err), .. }
			| NetworkResponse::ListAccountsFetched { result: Err(err), .. }
			| NetworkResponse::ListAccountAdded { result: Err(err), .. }
			| NetworkResponse::ListAccountRemoved { result: Err(err), .. } => {
				dialogs::show_error(frame, &err);
			}
			NetworkResponse::ListUpdated { result: Ok(list) } => {
				live_region::announce(live_region, &format!("List '{}' updated", list.title));
				if let Some(handle) = &state.network_handle {
					handle.send(NetworkCommand::FetchLists);
				}
			}

			NetworkResponse::ListDeleted { id: _, result: Ok(()) } => {
				live_region::announce(live_region, "List deleted");
				if let Some(handle) = &state.network_handle {
					handle.send(NetworkCommand::FetchLists);
				}
			}

			NetworkResponse::ListAccountsFetched { list_id, result: Ok(members) } => {
				if let Some(dlg) = &state.manage_lists_dialog {
					let list_title = dlg.get_list_title(&list_id).unwrap_or_default();
					if let Some(handle) = &state.network_handle {
						let net_tx = handle.command_tx.clone();
						let ui_tx_dlg = ui_tx.clone();
						let members_dlg = dialogs::ManageListMembersDialog::new(
							frame,
							list_id,
							&list_title,
							members,
							net_tx,
							move || {
								let _ = ui_tx_dlg.send(UiCommand::ManageListMembersDialogClosed);
							},
						);
						members_dlg.show();
						state.manage_list_members_dialog = Some(members_dlg);
					}
				}
			}

			NetworkResponse::ListAccountAdded { result: Ok(()), .. } => {
				live_region::announce(live_region, "Member added");
				if let Some(dlg) = &state.manage_list_members_dialog
					&& let Some(handle) = &state.network_handle
				{
					handle.send(NetworkCommand::FetchListAccounts { list_id: dlg.get_list_id().to_string() });
				}
			}

			NetworkResponse::ListAccountRemoved { result: Ok(()), .. } => {
				live_region::announce(live_region, "Member removed");
				if let Some(dlg) = &state.manage_list_members_dialog
					&& let Some(handle) = &state.network_handle
				{
					handle.send(NetworkCommand::FetchListAccounts { list_id: dlg.get_list_id().to_string() });
				}
			}
		}
	}
	let _ = frame;
}

pub fn update_poll_in_timelines(state: &mut AppState, poll: &Poll) {
	for timeline in state.timeline_manager.iter_mut() {
		for entry in &mut timeline.entries {
			if let Some(status) = entry.as_status_mut() {
				if let Some(p) = &mut status.poll
					&& p.id == poll.id
				{
					*p = poll.clone();
				}
				if let Some(reblog) = &mut status.reblog
					&& let Some(p) = &mut reblog.poll
					&& p.id == poll.id
				{
					*p = poll.clone();
				}
			}
		}
	}
}

/// Removes a status from all timelines.
pub fn remove_status_from_timelines(state: &mut AppState, status_id: &str) {
	for timeline in state.timeline_manager.iter_mut() {
		timeline.entries.retain(|entry| {
			if let Some(status) = entry.as_status() {
				if status.id == status_id {
					return false;
				}
				if let Some(reblog) = &status.reblog
					&& reblog.id == status_id
				{
					return false;
				}
			}
			true
		});
	}
}

/// Updates a status in all timelines where it appears.
pub fn update_status_in_timelines<F>(state: &mut AppState, status_id: &str, updater: F)
where
	F: Fn(&mut Status),
{
	for timeline in state.timeline_manager.iter_mut() {
		for entry in &mut timeline.entries {
			if let Some(status) = entry.as_status_mut() {
				// Check the status itself
				if status.id == status_id {
					updater(status);
				}
				// Check if it's a reblog of the target
				if let Some(ref mut reblog) = status.reblog
					&& reblog.id == status_id
				{
					updater(reblog);
				}
			}
		}
	}
}

/// Updates the following state of a tag in all timelines.
pub fn update_tag_in_timelines(state: &mut AppState, tag_name: &str, following: bool) {
	for timeline in state.timeline_manager.iter_mut() {
		for entry in &mut timeline.entries {
			if let Some(status) = entry.as_status_mut() {
				let check_status = |s: &mut Status| {
					for tag in &mut s.tags {
						if tag.name.eq_ignore_ascii_case(tag_name) {
							tag.following = following;
						}
					}
				};
				check_status(status);
				if let Some(ref mut reblog) = status.reblog {
					check_status(reblog);
				}
			}
		}
	}
}
