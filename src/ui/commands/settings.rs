//! Commands that open the options, shortcut, filter, and list dialogs.

use super::{UiCommand, UiCommandContext};
use crate::{
	AppState,
	accounts::{apply_refresh_interval, apply_streaming_setting, update_window_title},
	config::ContentWarningDisplay,
	network::NetworkCommand,
	timeline::{TimelineTextOptions, TimelineType},
	ui::{dialogs, menu::update_menu_labels, timeline_list::TimelineList},
	ui_wake::UiCommandSender,
};

pub(super) fn show_options(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	let frame = ctx.frame;
	let quick_action_keys_enabled = ctx.quick_action_keys_enabled;
	let autoload_mode = ctx.autoload_mode;
	let sort_order_cell = ctx.sort_order_cell;
	let shortcuts_cell = ctx.shortcuts_cell;
	let ui_tx = ctx.ui_tx;
	if let Some(options) = dialogs::prompt_for_options(
		frame,
		dialogs::OptionsDialogInput {
			enter_to_send: state.config.enter_to_send,
			always_show_link_dialog: state.config.always_show_link_dialog,
			show_link_previews: state.config.show_link_previews,
			strip_tracking: state.config.strip_tracking,
			quick_action_keys: state.config.quick_action_keys,
			check_for_updates: state.config.check_for_updates_on_startup,
			update_channel: state.config.update_channel,
			autoload: state.config.autoload,
			fetch_limit: state.config.fetch_limit,
			streaming: state.config.streaming,
			refresh_minutes: state.config.refresh_minutes,
			content_warning_display: state.config.content_warning_display,
			display_name_emoji_mode: state.config.display_name_emoji_mode,
			sort_order: state.config.sort_order,
			preserve_thread_order: state.config.preserve_thread_order,
			default_timelines: state.config.default_timelines.clone(),
			restore_open_timelines: state.config.restore_open_timelines,
			sync_read_position: state.config.sync_read_position,
			load_older_to_restore: state.config.load_older_to_restore,
			notification_preference: state.config.notification_preference,
			disabled_notification_types: state.config.disabled_notification_types.clone(),
			silent_notification_types: state.config.silent_notification_types.clone(),
			global_keys: state.config.global_keys,
			repeat_at_timeline_edges: state.config.repeat_at_timeline_edges,
			shortcuts: state.config.shortcuts.clone(),
			templates: state.config.templates.clone(),
			filters: state.config.filters.clone(),
			find_loading_mode: state.config.find_loading_mode,
			window_title_template: state.config.window_title_template.clone(),
		},
	) {
		let dialogs::OptionsDialogResult {
			enter_to_send,
			always_show_link_dialog,
			show_link_previews,
			strip_tracking,
			quick_action_keys,
			check_for_updates,
			update_channel,
			autoload,
			fetch_limit,
			streaming,
			refresh_minutes,
			content_warning_display,
			display_name_emoji_mode,
			sort_order,
			preserve_thread_order,
			default_timelines,
			restore_open_timelines,
			sync_read_position,
			load_older_to_restore,
			notification_preference,
			disabled_notification_types,
			silent_notification_types,
			global_keys,
			repeat_at_timeline_edges,
			shortcuts,
			templates,
			filters,
			find_loading_mode,
			window_title_template,
		} = options;
		let needs_refresh = state.config.sort_order != sort_order
			|| state.config.content_warning_display != content_warning_display
			|| state.config.display_name_emoji_mode != display_name_emoji_mode
			|| state.config.preserve_thread_order != preserve_thread_order
			|| state.config.show_link_previews != show_link_previews
			|| state.config.templates != templates
			|| state.config.filters != filters
			|| state.config.window_title_template != window_title_template;
		let streaming_changed = state.config.streaming != streaming;
		let refresh_changed = state.config.refresh_minutes != refresh_minutes;
		let hotkeys_changed =
			state.config.global_keys != global_keys || state.config.shortcuts.global != shortcuts.global;
		state.config.enter_to_send = enter_to_send;
		state.config.always_show_link_dialog = always_show_link_dialog;
		state.config.show_link_previews = show_link_previews;
		state.config.strip_tracking = strip_tracking;
		state.config.quick_action_keys = quick_action_keys;
		state.config.check_for_updates_on_startup = check_for_updates;
		state.config.update_channel = update_channel;
		state.config.autoload = autoload;
		state.config.fetch_limit = fetch_limit;
		state.config.streaming = streaming;
		state.config.refresh_minutes = refresh_minutes;
		if streaming_changed {
			apply_streaming_setting(state);
		}
		if refresh_changed {
			apply_refresh_interval(state);
		}
		state.config.content_warning_display = content_warning_display;
		state.config.display_name_emoji_mode = display_name_emoji_mode;
		state.config.sort_order = sort_order;
		state.config.preserve_thread_order = preserve_thread_order;
		state.config.default_timelines = default_timelines;
		state.config.restore_open_timelines = restore_open_timelines;
		state.config.sync_read_position = sync_read_position;
		state.config.load_older_to_restore = load_older_to_restore;
		state.config.notification_preference = notification_preference;
		state.config.disabled_notification_types = disabled_notification_types;
		state.config.silent_notification_types = silent_notification_types;
		state.config.global_keys = global_keys;
		state.config.repeat_at_timeline_edges = repeat_at_timeline_edges;
		state.config.shortcuts = shortcuts;
		*shortcuts_cell.borrow_mut() = state.config.shortcuts.clone();
		state.config.templates = templates;
		state.config.filters = filters;
		state.config.find_loading_mode = find_loading_mode;
		state.config.window_title_template = window_title_template;
		update_window_title(state, frame);
		if state.config.content_warning_display != ContentWarningDisplay::WarningOnly {
			state.cw_expanded.clear();
		}
		quick_action_keys_enabled.set(quick_action_keys);
		autoload_mode.set(autoload);
		sort_order_cell.set(sort_order);
		if let Some(mb) = frame.get_menu_bar() {
			update_menu_labels(&mb, state);
		}
		state.config.sort_order = sort_order;
		state.config.preserve_thread_order = preserve_thread_order;
		if hotkeys_changed {
			register_hotkeys(state, ui_tx, ctx.live_region);
		}
		if let Err(err) = state.save_config() {
			dialogs::show_error(frame, &err);
		}
		if needs_refresh {
			super::timeline::refresh_active_timeline(ctx);
		}
	}
}

pub(super) fn customize_shortcuts(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	let frame = ctx.frame;
	let shortcuts_cell = ctx.shortcuts_cell;
	if let Some(new_shortcuts) = dialogs::prompt_for_shortcuts(frame, &state.config.shortcuts) {
		let hotkeys_changed = state.config.shortcuts.global != new_shortcuts.global;
		state.config.shortcuts = new_shortcuts;
		if hotkeys_changed {
			register_hotkeys(state, ctx.ui_tx, ctx.live_region);
		}
		*shortcuts_cell.borrow_mut() = state.config.shortcuts.clone();
		if let Err(err) = state.save_config() {
			dialogs::show_error(frame, &err);
		}
		if let Some(mb) = frame.get_menu_bar() {
			update_menu_labels(&mb, state);
		}
	}
}

/// Registers the global shortcuts from the config, and says which couldn't be registered.
pub fn register_hotkeys(state: &AppState, ui_tx: &UiCommandSender, live_region: &TimelineList) {
	let Some(shell) = &state.app_shell else { return };
	let failed = shell.register_hotkeys(ui_tx.clone(), &state.config.shortcuts.global, state.config.global_keys);
	if !failed.is_empty() {
		live_region.announce(&format!("Another program is using {}", failed.join(", ")));
	}
}

pub(super) fn manage_lists_dialog_closed(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	if let Some(dialog) = state.manage_lists_dialog.take() {
		dialog.destroy();
	}
}

pub(super) fn manage_list_members_dialog_closed(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	if let Some(dialog) = state.manage_list_members_dialog.take() {
		dialog.destroy();
	}
}

pub(super) fn manage_filters(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	let frame = ctx.frame;
	let live_region = ctx.live_region;
	let Some(client) = &state.client else {
		live_region.announce("Network not available");
		return;
	};
	let Some(token) = &state.access_token else {
		live_region.announce("Not logged in");
		return;
	};
	match client.get_filters(token) {
		Ok(mut filters) => loop {
			let result = dialogs::prompt_manage_filters(frame, &filters);
			match result {
				dialogs::ManageFiltersResult::Add => {
					if let Some(data) = dialogs::prompt_filter_edit(frame, None) {
						let keywords: Vec<(String, bool)> = data
							.keywords
							.iter()
							.filter(|(_, _, _, d)| !*d)
							.map(|(_, k, w, _)| (k.clone(), *w))
							.collect();
						match client.create_filter(
							token,
							&data.title,
							&data.contexts,
							&data.action,
							&keywords,
							data.expires_in,
						) {
							Ok(_) => {
								if let Ok(new_filters) = client.get_filters(token) {
									filters = new_filters;
								}
							}
							Err(e) => dialogs::show_error(frame, &e),
						}
					}
				}
				dialogs::ManageFiltersResult::Edit(id) => {
					if let Some(filter) = filters.iter().find(|f| f.id == id)
						&& let Some(data) = dialogs::prompt_filter_edit(frame, Some(filter))
					{
						let keywords_attrs: Vec<(&str, &str, bool, bool)> =
							data.keywords.iter().map(|(id, k, w, d)| (id.as_str(), k.as_str(), *w, *d)).collect();
						match client.update_filter(
							token,
							&id,
							&data.title,
							&data.contexts,
							&data.action,
							&keywords_attrs,
							data.expires_in,
						) {
							Ok(_) => {
								if let Ok(new_filters) = client.get_filters(token) {
									filters = new_filters;
								}
							}
							Err(e) => dialogs::show_error(frame, &e),
						}
					}
				}
				dialogs::ManageFiltersResult::Delete(id) => match client.delete_filter(token, &id) {
					Ok(()) => {
						if let Ok(new_filters) = client.get_filters(token) {
							filters = new_filters;
						}
					}
					Err(e) => dialogs::show_error(frame, &e),
				},
				dialogs::ManageFiltersResult::None => break,
			}
		},
		Err(e) => dialogs::show_error(frame, &e),
	}
}

pub(super) fn message_requests(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	let frame = ctx.frame;
	let live_region = ctx.live_region;
	let (Some(client), Some(token)) = (&state.client, &state.access_token) else {
		live_region.announce("Not logged in");
		return;
	};
	let options = TimelineTextOptions::from_config_default(&state.config);
	let mut selected = None;
	let mut accepted = false;
	loop {
		let requests = match client.get_notification_requests(token) {
			Ok(requests) => requests,
			Err(err) if format!("{err:#}").contains("404") => {
				live_region.announce("Your server doesn't support message requests");
				return;
			}
			Err(err) => {
				dialogs::show_error(frame, &err);
				return;
			}
		};
		let (index, result) = match dialogs::prompt_message_requests(frame, &requests, &options, selected) {
			dialogs::MessageRequestAction::Accept(index) => {
				accepted = true;
				(index, client.accept_notification_request(token, &requests[index].id).map(|()| "Accepted"))
			}
			dialogs::MessageRequestAction::Dismiss(index) => {
				(index, client.dismiss_notification_request(token, &requests[index].id).map(|()| "Dismissed"))
			}
			dialogs::MessageRequestAction::Close => break,
		};
		match result {
			Ok(message) => live_region.announce(message),
			Err(err) => dialogs::show_error(frame, &err),
		}
		selected = Some(index);
	}
	state.pending_message_requests = None;
	if let Some(handle) = &state.network_handle {
		handle.send(NetworkCommand::FetchNotificationPolicy);
		// Accepted requests' notifications join the rest, so the timelines holding them refetch.
		if accepted {
			for timeline_type in [TimelineType::Notifications, TimelineType::Direct] {
				if state.timeline_manager.index_of(&timeline_type).is_some() {
					handle.send(NetworkCommand::FetchTimeline {
						timeline_type,
						limit: Some(u32::from(state.config.fetch_limit)),
						max_id: None,
					});
				}
			}
		}
	}
}

pub(super) fn manage_lists(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	let frame = ctx.frame;
	let live_region = ctx.live_region;
	let ui_tx = ctx.ui_tx;
	if let Some(handle) = &state.network_handle {
		if let Some(dlg) = &state.manage_lists_dialog {
			dlg.show();
		} else {
			let net_tx = handle.command_tx.clone();
			let ui_tx_close = ui_tx.clone();
			let dlg = dialogs::ManageListsDialog::new(frame, Vec::new(), net_tx, move || {
				let _ = ui_tx_close.send(UiCommand::ManageListsDialogClosed);
			});
			dlg.show();
			state.manage_lists_dialog = Some(dlg);
			handle.send(NetworkCommand::FetchLists);
		}
	} else {
		live_region.announce("Network not available");
	}
}
