use std::sync::{
	Arc,
	atomic::{AtomicBool, Ordering},
	mpsc::{self, Sender},
};

use wxdragon::{ffi, prelude::*};

use crate::{ID_UI_WAKE, UiCommand};

#[derive(Clone)]
pub struct UiWaker {
	frame_ptr: usize,
	event_id: i32,
	pending: Arc<AtomicBool>,
	alive: Arc<AtomicBool>,
}

impl UiWaker {
	#[cfg(test)]
	pub(crate) fn silent() -> Self {
		Self {
			frame_ptr: 0,
			event_id: 0,
			pending: Arc::new(AtomicBool::new(false)),
			alive: Arc::new(AtomicBool::new(false)),
		}
	}
	pub(crate) fn new(frame: Frame, alive: Arc<AtomicBool>) -> Self {
		Self::with_event(frame, alive, ID_UI_WAKE)
	}

	pub(crate) fn with_event(frame: Frame, alive: Arc<AtomicBool>, event_id: i32) -> Self {
		Self { frame_ptr: frame.handle_ptr() as usize, event_id, pending: Arc::new(AtomicBool::new(false)), alive }
	}

	pub(crate) fn wake(&self) {
		if !self.pending.swap(true, Ordering::SeqCst) {
			if !self.alive.load(Ordering::SeqCst) {
				return;
			}
			let handle = self.frame_ptr as *mut ffi::wxd_Window_t;
			if handle.is_null() {
				return;
			}
			unsafe { ffi::wxd_Window_PostMenuCommand(handle, self.event_id) };
		}
	}

	pub(crate) fn reset(&self) {
		self.pending.store(false, Ordering::SeqCst);
	}
}

#[derive(Clone)]
pub struct UiCommandSender {
	tx: Sender<UiCommand>,
	waker: UiWaker,
}

impl UiCommandSender {
	pub(crate) const fn new(tx: Sender<UiCommand>, waker: UiWaker) -> Self {
		Self { tx, waker }
	}

	pub(crate) fn send(&self, cmd: UiCommand) -> Result<(), Box<mpsc::SendError<UiCommand>>> {
		let result = self.tx.send(cmd).map_err(Box::new);
		self.waker.wake();
		result
	}
}
