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
	pending: Arc<AtomicBool>,
	alive: Arc<AtomicBool>,
}

impl UiWaker {
	pub(crate) fn new(frame: Frame, alive: Arc<AtomicBool>) -> Self {
		Self { frame_ptr: frame.handle_ptr() as usize, pending: Arc::new(AtomicBool::new(false)), alive }
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
			unsafe { ffi::wxd_Window_PostMenuCommand(handle, ID_UI_WAKE) };
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
