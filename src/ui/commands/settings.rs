//! Commands that open the options, shortcut, filter, and list dialogs.

use super::{UiCommand, UiCommandContext};
use crate::{
	accounts::update_window_title,
	config,
	config::ContentWarningDisplay,
	network::NetworkCommand,
	ui::{dialogs, menu::update_menu_labels, timeline_view::update_active_timeline_ui},
};

pub(super) fn show_options(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	let frame = ctx.frame;
	let timeline_list = &ctx.timeline_list;
	let suppress_selection = ctx.suppress_selection;
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
			content_warning_display: state.config.content_warning_display,
			display_name_emoji_mode: state.config.display_name_emoji_mode,
			sort_order: state.config.sort_order,
			preserve_thread_order: state.config.preserve_thread_order,
			default_timelines: state.config.default_timelines.clone(),
			restore_open_timelines: state.config.restore_open_timelines,
			notification_preference: state.config.notification_preference,
			sounds: state.config.sounds.clone(),
			hotkey: state.config.hotkey.clone(),
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
			content_warning_display,
			display_name_emoji_mode,
			sort_order,
			preserve_thread_order,
			default_timelines,
			restore_open_timelines,
			notification_preference,
			sounds,
			hotkey,
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
		let hotkey_changed = state.config.hotkey != hotkey;
		state.config.enter_to_send = enter_to_send;
		state.config.always_show_link_dialog = always_show_link_dialog;
		state.config.show_link_previews = show_link_previews;
		state.config.strip_tracking = strip_tracking;
		state.config.quick_action_keys = quick_action_keys;
		state.config.check_for_updates_on_startup = check_for_updates;
		state.config.update_channel = update_channel;
		state.config.autoload = autoload;
		state.config.fetch_limit = fetch_limit;
		state.config.content_warning_display = content_warning_display;
		state.config.display_name_emoji_mode = display_name_emoji_mode;
		state.config.sort_order = sort_order;
		state.config.preserve_thread_order = preserve_thread_order;
		state.config.default_timelines = default_timelines;
		state.config.restore_open_timelines = restore_open_timelines;
		state.config.notification_preference = notification_preference;
		// Rebuild the cached media controls so changed files and volume take effect immediately.
		let sounds_changed = state.config.sounds != sounds;
		state.config.sounds = sounds;
		if let Some(player) = &state.sound_player {
			player.set_volume(state.config.sounds.volume);
		}
		if sounds_changed {
			// Loading a whole pack takes long enough to be heard as a stall, so let the dialog
			// finish closing first and do it on the next pass through the command queue.
			let _ = ui_tx.send(UiCommand::ReloadSounds);
		}
		state.config.hotkey = hotkey;
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
		#[cfg(target_os = "windows")]
		if hotkey_changed && let Some(shell) = &state.app_shell {
			shell.re_register_hotkey(ui_tx.clone(), &state.config.hotkey);
		}
		let store = config::ConfigStore::new();
		if let Err(err) = store.save(&state.config) {
			dialogs::show_error(frame, &err);
		}
		if needs_refresh {
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
}

/// Reload every sound for the active pack.
///
/// Deferred out of the options dialog so closing it stays responsive; by the time this runs the
/// main window is back, and no notification can arrive in between.
pub(super) fn reload_sounds(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	if let Some(player) = &state.sound_player {
		player.preload(&state.config.sounds);
	}
}

pub(super) fn customize_shortcuts(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	let frame = ctx.frame;
	let shortcuts_cell = ctx.shortcuts_cell;
	if let Some(new_shortcuts) = dialogs::prompt_for_shortcuts(frame, &state.config.shortcuts) {
		state.config.shortcuts = new_shortcuts;
		*shortcuts_cell.borrow_mut() = state.config.shortcuts.clone();
		let store = config::ConfigStore::new();
		if let Err(err) = store.save(&state.config) {
			dialogs::show_error(frame, &err);
		}
		if let Some(mb) = frame.get_menu_bar() {
			update_menu_labels(&mb, state);
		}
	}
}

pub(super) fn manage_lists_dialog_closed(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	state.manage_lists_dialog = None;
}

pub(super) fn manage_list_members_dialog_closed(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	state.manage_list_members_dialog = None;
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
