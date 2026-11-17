//! Commands that open the options, shortcut, filter, and list dialogs.

use wxdragon::prelude::*;

use super::{UiCommand, UiCommandContext, deferred};
use crate::{
	AppState,
	accounts::{apply_refresh_interval, apply_streaming_setting, update_window_title},
	config::ContentWarningDisplay,
	responses::{fetch_notification_policy, lists, timelines},
	timeline::{TimelineTextOptions, TimelineType},
	ui::{dialogs, menu::relabel_menus, timeline_list::TimelineList},
	ui_wake::UiCommandSender,
};

pub(super) fn show_options(ctx: &mut UiCommandContext<'_>) {
	let initial = dialogs::OptionsDialogResult::load(&ctx.state.config);
	deferred::dialog_some(ctx, move |frame| dialogs::prompt_for_options(frame, initial), apply_options);
}

fn apply_options(ctx: &mut UiCommandContext<'_>, options: dialogs::OptionsDialogResult) {
	let state = &mut *ctx.state;
	let frame = ctx.frame;
	let before = dialogs::OptionsDialogResult::load(&state.config);
	if before == options {
		return;
	}
	let needs_refresh =
		before.display != options.display || before.templates != options.templates || before.filters != options.filters;
	let streaming_changed = before.timelines.streaming != options.timelines.streaming;
	let refresh_changed = before.timelines.refresh_minutes != options.timelines.refresh_minutes;
	let hotkeys_changed = before.invisible.global_keys != options.invisible.global_keys;
	if before.advanced.log_level != options.advanced.log_level {
		tracing::info!(from = %before.advanced.log_level, to = %options.advanced.log_level, "log level changed");
		logkeeper::set_level(options.advanced.log_level);
	}
	options.save(&mut state.config);
	if streaming_changed {
		apply_streaming_setting(state);
	}
	if refresh_changed {
		apply_refresh_interval(state);
	}
	crate::ui::spell_check::configure(
		state.config.spell_check,
		state.config.spell_check_announce,
		&state.config.spell_language,
	);
	update_window_title(state, frame);
	if state.config.content_warning_display != ContentWarningDisplay::WarningOnly {
		state.cw_expanded.clear();
	}
	ctx.quick_action_keys_enabled.set(state.config.quick_action_keys);
	ctx.autoload_mode.set(state.config.autoload);
	ctx.sort_order_cell.set(state.config.sort_order);
	if let Some(mb) = frame.get_menu_bar() {
		relabel_menus(&mb, state);
	}
	if hotkeys_changed {
		register_hotkeys(state, ctx.ui_tx, ctx.live_region);
	}
	if let Err(err) = state.save_config() {
		deferred::error(ctx, err);
	}
	if needs_refresh {
		super::timeline::refresh_active_timeline(ctx);
	}
}

pub(super) fn customize_shortcuts(ctx: &mut UiCommandContext<'_>) {
	let initial = ctx.state.config.shortcuts.clone();
	deferred::dialog_some(
		ctx,
		move |frame| dialogs::prompt_for_shortcuts(frame, &initial),
		|ctx, new_shortcuts| {
			let state = &mut *ctx.state;
			let frame = ctx.frame;
			let hotkeys_changed = state.config.shortcuts.global != new_shortcuts.global;
			state.config.shortcuts = new_shortcuts;
			if hotkeys_changed {
				register_hotkeys(state, ctx.ui_tx, ctx.live_region);
			}
			*ctx.shortcuts_cell.borrow_mut() = state.config.shortcuts.clone();
			if let Some(mb) = frame.get_menu_bar() {
				relabel_menus(&mb, state);
			}
			if let Err(err) = state.save_config() {
				deferred::error(ctx, err);
			}
		},
	);
}

/// Registers the global shortcuts from the config, and says which couldn't be registered.
pub fn register_hotkeys(state: &AppState, ui_tx: &UiCommandSender, live_region: &TimelineList) {
	let Some(shell) = &state.app_shell else { return };
	let failed = shell.register_hotkeys(ui_tx.clone(), &state.config.shortcuts.global, state.config.global_keys);
	if !failed.is_empty() {
		tracing::warn!(shortcuts = failed.join(", "), "couldn't register global shortcuts another program is using");
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
	let live_region = ctx.live_region;
	let Some(client) = state.client.clone() else {
		live_region.announce("Network not available");
		return;
	};
	let Some(token) = state.access_token.clone() else {
		live_region.announce("Not logged in");
		return;
	};
	deferred::show(ctx, move |frame| manage_filters_dialog(frame, &client, &token));
}

/// Shows the filter list and acts on what's asked of it until it's closed.
fn manage_filters_dialog(frame: &Frame, client: &crate::mastodon::MastodonClient, token: &str) {
	let mut filters = match client.get_filters(token) {
		Ok(filters) => filters,
		Err(e) => {
			dialogs::show_error(frame, &e);
			return;
		}
	};
	loop {
		let result = dialogs::prompt_manage_filters(frame, &filters);
		match result {
			dialogs::ManageFiltersResult::Add => {
				if let Some(data) = dialogs::prompt_filter_edit(frame, None) {
					let keywords: Vec<(String, bool)> =
						data.keywords.iter().filter(|(_, _, _, d)| !*d).map(|(_, k, w, _)| (k.clone(), *w)).collect();
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
	}
}

pub(super) fn message_requests(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	let live_region = ctx.live_region.clone();
	let (Some(client), Some(token)) = (state.client.clone(), state.access_token.clone()) else {
		live_region.announce("Not logged in");
		return;
	};
	let options = TimelineTextOptions::from_config_default(&state.config);
	deferred::dialog_some(
		ctx,
		move |frame| {
			let mut selected = None;
			let mut accepted = false;
			loop {
				let requests = match client.get_notification_requests(&token) {
					Ok(requests) => requests,
					Err(err) if format!("{err:#}").contains("404") => {
						live_region.announce("Your server doesn't support message requests");
						return None;
					}
					Err(err) => {
						dialogs::show_error(frame, &err);
						return None;
					}
				};
				let (index, result) = match dialogs::prompt_message_requests(frame, &requests, &options, selected) {
					dialogs::MessageRequestAction::Accept(index) => {
						accepted = true;
						(index, client.accept_notification_request(&token, &requests[index].id).map(|()| "Accepted"))
					}
					dialogs::MessageRequestAction::Dismiss(index) => {
						(index, client.dismiss_notification_request(&token, &requests[index].id).map(|()| "Dismissed"))
					}
					dialogs::MessageRequestAction::Close => break,
				};
				match result {
					Ok(message) => live_region.announce(message),
					Err(err) => dialogs::show_error(frame, &err),
				}
				selected = Some(index);
			}
			Some(accepted)
		},
		|ctx, accepted| {
			let state = &mut *ctx.state;
			state.pending_message_requests = None;
			if let Some(requests) = state.requests() {
				fetch_notification_policy(&requests);
				// Accepted requests' notifications join the rest, so the timelines holding them refetch.
				if accepted {
					for timeline_type in [TimelineType::Notifications, TimelineType::Direct] {
						if state.timeline_manager.index_of(&timeline_type).is_some() {
							timelines::refresh(state, timeline_type);
						}
					}
				}
			}
		},
	);
}

pub(super) fn manage_lists(ctx: &mut UiCommandContext<'_>) {
	let Some(requests) = ctx.requests() else {
		ctx.live_region.announce("Network not available");
		return;
	};
	if let Some(dlg) = &ctx.state.manage_lists_dialog {
		dlg.show();
		return;
	}
	let ui_tx_close = ctx.ui_tx.clone();
	let dlg = dialogs::ManageListsDialog::new(ctx.frame, Vec::new(), requests.clone(), move || {
		let _ = ui_tx_close.send(UiCommand::ManageListsDialogClosed);
	});
	dlg.show();
	ctx.state.manage_lists_dialog = Some(dlg);
	lists::fetch(&requests);
}
