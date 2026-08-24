//! Application-level commands: window visibility, help, and shutdown.

use wxdragon::prelude::*;

use super::UiCommandContext;
use crate::{
	config,
	ui::{app_shell, dialogs, menu::update_menu_labels},
};

pub(super) fn toggle_window_visibility(ctx: &mut UiCommandContext<'_>) {
	let frame = ctx.frame;
	let tray_hidden = ctx.tray_hidden;
	app_shell::toggle_window_visibility(frame, tray_hidden);
}

pub(super) fn set_quick_action_keys_enabled(ctx: &mut UiCommandContext<'_>, enabled: bool) {
	let state = &mut *ctx.state;
	let frame = ctx.frame;
	let live_region = ctx.live_region;
	let quick_action_keys_enabled = ctx.quick_action_keys_enabled;
	state.config.quick_action_keys = enabled;
	quick_action_keys_enabled.set(enabled);
	let _ = config::ConfigStore::new().save(&state.config);
	let msg = if enabled { "Quick keys enabled" } else { "Quick keys disabled" };
	live_region.announce(msg);
	if let Some(mb) = frame.get_menu_bar() {
		update_menu_labels(&mb, state);
	}
}

pub(super) fn view_help(ctx: &mut UiCommandContext<'_>) {
	let frame = ctx.frame;
	let live_region = ctx.live_region;
	if let Ok(mut path) = std::env::current_exe() {
		path.pop();
		path.push("readme.html");
		if path.exists() {
			live_region.announce("Opening help");
			let _ = wxdragon::utils::launch_default_browser(
				&path.to_string_lossy(),
				wxdragon::utils::BrowserLaunchFlags::Default,
			);
		} else {
			live_region.announce("Help file not found");
			dialogs::show_error(frame, &anyhow::anyhow!("Help file (readme.html) not found in application directory."));
		}
	} else {
		live_region.announce("Could not determine help path");
	}
}

pub(super) fn check_for_updates(ctx: &mut UiCommandContext<'_>) {
	let frame = ctx.frame;
	crate::ui::update_check::run_update_check(*frame, false);
}

pub(super) fn app_closing(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	state.config.saved_timelines = state.timeline_manager.open_timeline_types();
	state.config.saved_active_timeline = state.timeline_manager.active().map(|t| t.timeline_type.clone());
	state.config.saved_selected_post_id = state.timeline_manager.active().and_then(|t| t.selected_id.clone());
	let _ = config::ConfigStore::new().save(&state.config);
	ctx.frame.destroy();
}

pub(super) fn exit_app(ctx: &mut UiCommandContext<'_>) {
	ctx.frame.close(true);
}
