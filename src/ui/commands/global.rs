//! System-wide shortcuts: using Fedra without its window, from anywhere.
//!
//! Most global actions are ordinary commands under another key. Moving through posts and
//! timelines is the exception: it drives the same list the window does, and speaks where it
//! lands whenever a screen reader wouldn't already be reading the list.

use super::{UiCommandContext, command_for, deferred, handle_ui_command};
use crate::{
	UiCommand,
	config::{ActionId, GlobalAction},
	ui::{app_shell, dialogs, keys},
};

pub(super) fn run(ctx: &mut UiCommandContext<'_>, action: GlobalAction) {
	match action {
		GlobalAction::ToggleWindow => app_shell::toggle_window_visibility(ctx.frame, ctx.tray_hidden),
		GlobalAction::PreviousPost => step(ctx, keys::UP),
		GlobalAction::NextPost => step(ctx, keys::DOWN),
		GlobalAction::FirstPost => step(ctx, keys::HOME),
		GlobalAction::LastPost => step(ctx, keys::END),
		GlobalAction::ReadPost => {
			let text = ctx.timeline_list.selected_text().unwrap_or_else(|| "No post selected".to_string());
			ctx.live_region.announce(&text);
		}
		GlobalAction::PreviousTimeline => switch_timeline(ctx, false),
		GlobalAction::NextTimeline => switch_timeline(ctx, true),
		GlobalAction::Exit => handle_ui_command(UiCommand::ExitApp, ctx),
		GlobalAction::Action(ActionId::Find) => {
			deferred::dialog_some(
				ctx,
				|frame| dialogs::show_find_dialog(frame),
				|ctx, query| {
					handle_ui_command(UiCommand::Find(query), ctx);
				},
			);
		}
		GlobalAction::Action(action) => {
			if let Some(command) = command_for(action) {
				handle_ui_command(command, ctx);
			}
		}
	}
}

/// Moves through the active timeline as `key` would in the list, and reads the post it lands on.
///
/// At either end the selection stays put and that post is read again, so the key never goes
/// silent.
fn step(ctx: &UiCommandContext<'_>, key: i32) {
	let list = &ctx.timeline_list;
	if list.get_count() == 0 {
		ctx.live_region.announce("No posts");
		return;
	}
	let before = list.get_selection();
	list.navigate(key);
	if !ctx.state.config.repeat_at_timeline_edges && list.get_selection() == before {
		return;
	}
	// With the list focused in a foreground window, the screen reader reads the new selection
	// itself, and saying it again would read it twice.
	if list.has_focus() && app_shell::is_window_active(ctx.frame) {
		return;
	}
	if let Some(text) = list.selected_text() {
		ctx.live_region.announce(&text);
	}
}

/// Switches to the next or previous timeline, and reads its name with the post it lands on in one
/// announcement, since a second one would cut the first off.
fn switch_timeline(ctx: &mut UiCommandContext<'_>, forward: bool) {
	let count = ctx.state.timeline_manager.len();
	if count == 0 {
		return;
	}
	let current = ctx.state.timeline_manager.active_index();
	let index = if forward { (current + 1) % count } else { (current + count - 1) % count };
	if index != current {
		handle_ui_command(UiCommand::TimelineSelectionChanged(index), ctx);
	}
	let name = ctx.state.timeline_manager.display_names().get(index).cloned().unwrap_or_default();
	let text = match ctx.timeline_list.selected_text() {
		Some(post) => format!("{name}. {post}"),
		None => name,
	};
	ctx.live_region.announce(&text);
}
