use std::cell::{Cell, RefCell};

use wx_utils::global_hotkeys::GlobalHotkeys;
use wxdragon::prelude::*;

#[cfg(not(target_os = "macos"))]
use crate::{ID_TRAY_EXIT, ID_TRAY_TOGGLE};
use crate::{
	UiCommand,
	config::{GlobalAction, GlobalShortcuts, KeyChord},
	ui_wake::UiCommandSender,
};

pub struct AppShell {
	#[cfg(not(target_os = "macos"))]
	pub(crate) tray_menu: RefCell<Option<Menu>>,
	#[cfg(not(target_os = "macos"))]
	pub(crate) taskbar: TaskBarIcon,
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

	/// Clean up the tray icon and hotkey thread. Safe to call multiple times.
	/// This should be called during `on_close` (before window destruction begins)
	/// so that destroying the tray icon doesn't cause a focus shift back to the
	/// still-alive window, which screen readers would announce.
	pub fn cleanup(&self) {
		#[cfg(not(target_os = "macos"))]
		{
			if let Some(mut menu) = self.tray_menu.borrow_mut().take() {
				menu.destroy_menu();
			}
			self.taskbar.destroy();
		}
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
pub fn install_app_shell(_frame: &Frame, _ui_tx: UiCommandSender) -> AppShell {
	// macOS uses the Dock to reopen the window, without a menu bar status item.
	AppShell { hotkeys: RefCell::new(None) }
}

#[cfg(not(target_os = "macos"))]
pub fn install_app_shell(_frame: &Frame, ui_tx: UiCommandSender) -> AppShell {
	let mut tray_menu = Menu::builder()
		.append_item(ID_TRAY_TOGGLE, "Show/Hide", "Show or hide Fedra")
		.append_separator()
		.append_item(ID_TRAY_EXIT, "Exit", "Exit Fedra")
		.build();
	let taskbar = TaskBarIcon::builder().with_icon_type(TaskBarIconType::CustomStatusItem).build();
	taskbar.set_popup_menu(&mut tray_menu);
	let tray_icon = ArtProvider::get_bitmap(ArtId::Information, ArtClient::Menu, Some(Size::new(16, 16)));
	if let Some(icon) = tray_icon {
		let _ = taskbar.set_icon(&icon, "Fedra");
	} else if let Some(fallback) = Bitmap::new(16, 16) {
		let _ = taskbar.set_icon(&fallback, "Fedra");
	}
	taskbar.on_menu(move |event| match event.get_id() {
		ID_TRAY_TOGGLE => {
			let _ = ui_tx.send(UiCommand::ToggleWindowVisibility);
		}
		ID_TRAY_EXIT => {
			let _ = ui_tx.send(UiCommand::ExitApp);
		}
		_ => {}
	});
	AppShell { tray_menu: RefCell::new(Some(tray_menu)), taskbar, hotkeys: RefCell::new(None) }
}

pub fn toggle_window_visibility(frame: &Frame, tray_hidden: &Cell<bool>) {
	let is_shown = frame.is_shown();
	if is_shown && is_window_active(frame) {
		frame.show(false);
		tray_hidden.set(true);
		return;
	}
	if is_shown && !is_window_active(frame) {
		if frame.is_iconized() {
			frame.iconize(false);
		}
		frame.raise();
		return;
	}
	if !is_shown {
		frame.show(true);
		frame.raise();
		tray_hidden.set(false);
	}
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
