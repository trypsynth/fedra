//! Shows modal dialogs without holding up the UI command handler that asked for them.
//!
//! A modal dialog runs a nested event loop. Shown from inside a command or response handler, it
//! would leave that handler (and its borrow of the app state) running for as long as the dialog
//! is open, so streamed posts, notifications and request answers would pile up until it closed.
//! Shown from here, the handler has already returned and they're handled as they arrive.
//!
//! A handler gathers what the dialog needs, calls [`dialog`] (or one of its variants) with a
//! `show` closure that runs the dialog and a `finish` closure that gets the result back with the
//! app state. Dialogs run one at a time, in the order they were asked for.

use std::{
	cell::{Cell, RefCell},
	collections::VecDeque,
};

use wx_utils::{post_to_ui, tasks::spawn_for};
use wxdragon::prelude::*;

use super::{UiCommandContext, UiHandles};
use crate::ui::dialogs;

thread_local! {
	/// Dialogs waiting for the event loop to get to them.
	static QUEUE: RefCell<VecDeque<Box<dyn FnOnce()>>> = const { RefCell::new(VecDeque::new()) };
	static RUNNING: Cell<bool> = const { Cell::new(false) };
}

/// Clears the running flag even if a dialog panics.
struct RunningGuard;

impl Drop for RunningGuard {
	fn drop(&mut self) {
		RUNNING.with(|running| running.set(false));
	}
}

fn pump() {
	if RUNNING.with(Cell::get) {
		// A dialog is open and this ran from its event loop. It schedules the next one when it closes.
		return;
	}
	let Some(job) = QUEUE.with(|queue| queue.borrow_mut().pop_front()) else { return };
	RUNNING.with(|running| running.set(true));
	{
		let _guard = RunningGuard;
		job();
	}
	if QUEUE.with(|queue| !queue.borrow().is_empty()) {
		post_to_ui(pump);
	}
}

fn enqueue(job: Box<dyn FnOnce()>) {
	QUEUE.with(|queue| queue.borrow_mut().push_back(job));
	post_to_ui(pump);
}

/// Runs `finish` with the app state as soon as nothing else is using it.
fn finish_with_state(handles: UiHandles, finish: impl FnOnce(&mut UiCommandContext<'_>) + 'static) {
	let frame = handles.frame;
	spawn_for(&frame, async move { handles.with_ctx(finish).await });
}

/// Runs `show` once the current handler has returned, then `finish` with its result.
pub fn dialog<T: 'static>(
	ctx: &UiCommandContext<'_>,
	show: impl FnOnce(&Frame) -> T + 'static,
	finish: impl FnOnce(&mut UiCommandContext<'_>, T) + 'static,
) {
	let handles = ctx.handles.clone();
	enqueue(Box::new(move || {
		let result = show(&handles.frame);
		finish_with_state(handles, move |ctx| finish(ctx, result));
	}));
}

/// Like [`dialog`] for a dialog that can be cancelled: `finish` only runs when it wasn't.
pub fn dialog_some<T: 'static>(
	ctx: &UiCommandContext<'_>,
	show: impl FnOnce(&Frame) -> Option<T> + 'static,
	finish: impl FnOnce(&mut UiCommandContext<'_>, T) + 'static,
) {
	let handles = ctx.handles.clone();
	enqueue(Box::new(move || {
		if let Some(result) = show(&handles.frame) {
			finish_with_state(handles, move |ctx| finish(ctx, result));
		}
	}));
}

/// Runs a dialog that has nothing to report back.
pub fn show(ctx: &UiCommandContext<'_>, show: impl FnOnce(&Frame) + 'static) {
	let frame = *ctx.frame;
	enqueue(Box::new(move || show(&frame)));
}

/// Shows an error over the main window.
pub fn error(ctx: &UiCommandContext<'_>, err: anyhow::Error) {
	show(ctx, move |frame| dialogs::show_error(frame, &err));
}
