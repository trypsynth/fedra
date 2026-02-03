use std::{
	cell::{Cell, RefCell},
	rc::Rc,
	sync::mpsc,
	thread::{self, JoinHandle},
};

use wxdragon::prelude::*;

use crate::{ID_TRAY_EXIT, ID_TRAY_TOGGLE, UiCommand};

#[cfg(target_os = "windows")]
struct HotkeyHandle {
	thread_id: u32,
	join_handle: JoinHandle<()>,
}

pub struct AppShell {
	tray_menu: Menu,
	taskbar: TaskBarIcon,
	#[cfg(target_os = "windows")]
	hotkey_handle: Rc<RefCell<Option<HotkeyHandle>>>,
}

impl AppShell {
	pub fn attach_destroy(self, frame: &Frame) {
		let mut tray_menu_cleanup = self.tray_menu;
		let taskbar_cleanup = self.taskbar;
		#[cfg(target_os = "windows")]
		let hotkey_handle_destroy = self.hotkey_handle;
		frame.on_destroy(move |_| {
			tray_menu_cleanup.destroy_menu();
			taskbar_cleanup.destroy();
			#[cfg(target_os = "windows")]
			if let Some(handle) = hotkey_handle_destroy.borrow_mut().take() {
				use windows::Win32::{
					Foundation::{LPARAM, WPARAM},
					UI::WindowsAndMessaging::{PostThreadMessageW, WM_QUIT},
				};
				if handle.thread_id != 0 {
					unsafe {
						let _ = PostThreadMessageW(handle.thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
					}
				}
				let _ = handle.join_handle.join();
			}
		});
	}
}

pub fn install_app_shell(frame: &Frame, ui_tx: mpsc::Sender<UiCommand>) -> AppShell {
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
	let ui_tx_tray = ui_tx.clone();
	let frame_tray = *frame;
	taskbar.on_menu(move |event| match event.get_id() {
		ID_TRAY_TOGGLE => {
			let _ = ui_tx_tray.send(UiCommand::ToggleWindowVisibility);
		}
		ID_TRAY_EXIT => {
			frame_tray.close(true);
		}
		_ => {}
	});
	#[cfg(target_os = "windows")]
	let hotkey_handle = Rc::new(RefCell::new(start_hotkey_listener(ui_tx)));
	AppShell {
		tray_menu,
		taskbar,
		#[cfg(target_os = "windows")]
		hotkey_handle,
	}
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

fn is_window_active(frame: &Frame) -> bool {
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

#[cfg(target_os = "windows")]
fn start_hotkey_listener(ui_tx: mpsc::Sender<UiCommand>) -> Option<HotkeyHandle> {
	use windows::Win32::{
		System::Threading::GetCurrentThreadId,
		UI::{
			Input::KeyboardAndMouse::{MOD_ALT, MOD_CONTROL, RegisterHotKey, UnregisterHotKey},
			WindowsAndMessaging::{GetMessageW, MSG, WM_HOTKEY},
		},
	};
	const HOTKEY_ID: i32 = 1;
	const HOTKEY_VK: u32 = 0x46; // 'F'
	let (thread_id_tx, thread_id_rx) = mpsc::channel();
	let join_handle = thread::spawn(move || {
		let thread_id = unsafe { GetCurrentThreadId() };
		let _ = thread_id_tx.send(thread_id);
		let modifiers = MOD_CONTROL | MOD_ALT;
		let registered = unsafe { RegisterHotKey(None, HOTKEY_ID, modifiers, HOTKEY_VK).is_ok() };
		if !registered {
			return;
		}
		let mut msg = MSG::default();
		loop {
			let result = unsafe { GetMessageW(&mut msg, None, 0, 0) };
			if result.0 <= 0 {
				break;
			}
			if msg.message == WM_HOTKEY {
				let _ = ui_tx.send(UiCommand::ToggleWindowVisibility);
			}
		}
		unsafe {
			let _ = UnregisterHotKey(None, HOTKEY_ID);
		}
	});
	let thread_id = thread_id_rx.recv().ok()?;
	Some(HotkeyHandle { thread_id, join_handle })
}
