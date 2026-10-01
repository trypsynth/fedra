//! Application-level commands: window visibility, help, and shutdown.

use wxdragon::prelude::*;

use super::UiCommandContext;
use crate::ui::{app_shell, dialogs, menu::update_menu_labels};

pub(super) fn toggle_window_visibility(ctx: &mut UiCommandContext<'_>) {
	let frame = ctx.frame;
	let tray_hidden = ctx.tray_hidden;
	app_shell::toggle_window_visibility(frame, tray_hidden);
	// Saved right away rather than on exit, since a Windows shutdown can end the process before
	// AppClosing runs, and that's the case where an autostarted Fedra needs to remember it.
	if ctx.state.config.saved_window_hidden != tray_hidden.get() {
		ctx.state.config.saved_window_hidden = tray_hidden.get();
		let _ = ctx.state.save_config();
	}
}

pub(super) fn set_quick_action_keys_enabled(ctx: &mut UiCommandContext<'_>, enabled: bool) {
	let state = &mut *ctx.state;
	let frame = ctx.frame;
	let live_region = ctx.live_region;
	let quick_action_keys_enabled = ctx.quick_action_keys_enabled;
	state.config.quick_action_keys = enabled;
	quick_action_keys_enabled.set(enabled);
	let _ = state.save_config();
	let msg = if enabled { "Quick keys enabled" } else { "Quick keys disabled" };
	live_region.announce(msg);
	if let Some(mb) = frame.get_menu_bar() {
		update_menu_labels(&mb, state);
	}
}

pub(super) fn view_help(ctx: &mut UiCommandContext<'_>) {
	let frame = ctx.frame;
	let live_region = ctx.live_region;
	let path = crate::resource_dir().join("readme.html");
	if path.exists() {
		live_region.announce("Opening help");
		let _ = wxdragon::utils::launch_default_browser(
			&path.to_string_lossy(),
			wxdragon::utils::BrowserLaunchFlags::Default,
		);
	} else {
		live_region.announce("Help file not found");
		dialogs::show_error(frame, &anyhow::anyhow!("The help file is missing. Try reinstalling Fedra."));
	}
}

pub(super) fn show_window(ctx: &mut UiCommandContext<'_>) {
	let frame = ctx.frame;
	if !frame.is_shown() {
		frame.show(true);
		ctx.tray_hidden.set(false);
	}
	if frame.is_iconized() {
		frame.iconize(false);
	}
	frame.raise();
}

pub(super) fn check_for_updates(ctx: &mut UiCommandContext<'_>) {
	let frame = ctx.frame;
	crate::ui::update_check::run_update_check(*frame, false);
}

pub(super) fn app_closing(ctx: &mut UiCommandContext<'_>) {
	let _ = ctx.state.save_config();
	crate::read_position::sync_before_exit(ctx.state);
	ctx.frame.destroy();
}

pub(super) fn exit_app(ctx: &mut UiCommandContext<'_>) {
	ctx.frame.close(true);
}
