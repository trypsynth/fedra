use std::{
	cell::{Cell, RefCell},
	rc::Rc,
};

#[cfg(not(target_os = "macos"))]
use wx_utils::tray::Tray;
use wx_utils::{global_hotkeys::GlobalHotkeys, tray::Visibility};
use wxdragon::prelude::*;

use crate::{
	UiCommand,
	config::{GlobalAction, GlobalShortcuts, KeyChord},
	ui_wake::UiCommandSender,
};

/// The tray icon, the global hotkeys, and hiding the window and bringing it back.
pub struct AppShell {
	frame: Frame,
	/// Hides the window and brings it back along with whichever dialog the user was in.
	visibility: Rc<Visibility>,
	#[cfg(not(target_os = "macos"))]
	tray: Tray,
	hotkeys: RefCell<Option<GlobalHotkeys>>,
}

impl AppShell {
	/// Registers the global shortcuts, replacing any registered before.
	///
	/// Showing and hiding the window is always registered, and the rest only when `global_keys`
	/// is on: the key that brings a hidden window back is not one you can do without.
	///
	/// Returns the chords that couldn't be registered, nearly always because another program
	/// holds them, so the caller can say so rather than leave a key that silently does nothing.
	pub fn register_hotkeys(
		&self,
		ui_tx: UiCommandSender,
		shortcuts: &GlobalShortcuts,
		global_keys: bool,
	) -> Vec<String> {
		// A chord this process still holds can't be registered a second time.
		*self.hotkeys.borrow_mut() = None;
		let bindings: Vec<_> = GlobalAction::all()
			.into_iter()
			.filter(|&action| global_keys || action == GlobalAction::ToggleWindow)
			.filter_map(|action| shortcuts.get_chord(action).map(|chord| (chord, action)))
			.collect();
		if bindings.is_empty() || !cfg!(windows) {
			return Vec::new();
		}
		let (running, failed) = GlobalHotkeys::register(bindings, move |action| {
			let _ = ui_tx.send(UiCommand::Global(action));
		});
		*self.hotkeys.borrow_mut() = running;
		failed.iter().map(KeyChord::to_shortcut_string).collect()
	}

	/// Hides Fedra when it's what the user is in, and otherwise brings it forward. For the hotkey.
	/// `tray_hidden` learns which it did.
	pub fn toggle(&self, tray_hidden: &Cell<bool>) {
		self.visibility.toggle();
		tray_hidden.set(!self.frame.is_shown());
	}

	/// Hides Fedra when it's shown, and otherwise brings it back. For the tray menu, which takes
	/// the foreground itself, so whether Fedra was in front can't be told any more.
	pub fn show_or_hide(&self, tray_hidden: &Cell<bool>) {
		self.visibility.show_or_hide();
		tray_hidden.set(!self.frame.is_shown());
	}

	/// Brings Fedra to the front, onto the dialog the user was last in.
	pub fn come_forward(&self, tray_hidden: &Cell<bool>) {
		self.visibility.come_forward();
		tray_hidden.set(false);
	}

	/// Shows a notification from the tray icon, which only Windows does.
	#[cfg(windows)]
	pub fn show_balloon(&self, title: &str, body: &str) {
		// wxICON_INFORMATION = 0x00000002
		self.tray.show_balloon(title, body, 5000, 0x0000_0002);
	}

	/// Clean up the tray icon and hotkey thread. Safe to call multiple times.
	/// This should be called during `on_close` (before window destruction begins)
	/// so that destroying the tray icon doesn't cause a focus shift back to the
	/// still-alive window, which screen readers would announce.
	pub fn cleanup(&self) {
		#[cfg(not(target_os = "macos"))]
		self.tray.remove();
		// Leaked rather than dropped: dropping waits for the hotkey thread, and blocking here
		// during close can hang the UI thread. The process is exiting, and Windows releases its
		// hotkeys when it does.
		#[cfg(windows)]
		if let Some(hotkeys) = self.hotkeys.borrow_mut().take() {
			std::mem::forget(hotkeys);
		}
	}
}

#[cfg(target_os = "macos")]
pub fn install_app_shell(frame: &Frame, _ui_tx: UiCommandSender) -> AppShell {
	// macOS uses the Dock to reopen the window, without a menu bar status item.
	AppShell { frame: *frame, visibility: Rc::new(Visibility::new(*frame)), hotkeys: RefCell::new(None) }
}

#[cfg(not(target_os = "macos"))]
pub fn install_app_shell(frame: &Frame, ui_tx: UiCommandSender) -> AppShell {
	let (click_tx, menu_tx, exit_tx) = (ui_tx.clone(), ui_tx.clone(), ui_tx);
	// Through the command queue, so the window's hidden state is saved as it changes.
	let tray = Tray::builder("Fedra")
		.on_click(move || {
			let _ = click_tx.send(UiCommand::ShowWindow);
		})
		.on_show_hide(move || {
			let _ = menu_tx.send(UiCommand::ShowOrHideWindow);
		})
		.build(frame);
	tray.on_exit(move || {
		let _ = exit_tx.send(UiCommand::ExitApp);
	});
	AppShell { frame: *frame, visibility: Rc::new(Visibility::new(*frame)), tray, hotkeys: RefCell::new(None) }
}

pub fn is_window_active(frame: &Frame) -> bool {
	#[cfg(target_os = "windows")]
	{
		use windows::Win32::{Foundation::HWND, UI::WindowsAndMessaging::GetForegroundWindow};
		let handle = frame.get_handle();
		if handle.is_null() {
			return frame.has_focus();
		}
		let frame_hwnd = HWND(handle);
		let foreground = unsafe { GetForegroundWindow() };
		foreground == frame_hwnd
	}
	#[cfg(not(target_os = "windows"))]
	{
		frame.has_focus()
	}
}
