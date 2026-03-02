#![allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap, clippy::cast_sign_loss, unsafe_op_in_unsafe_fn)]

use std::{cell::RefCell, rc::Rc};

use wxdragon::prelude::*;

// ── Win32 ListView constants (defined locally for feature-flag safety) ───────

/// `LVS_REPORT`: single-column report view
const LVS_REPORT: u32 = 0x0001;
/// `LVS_NOCOLUMNHEADER`: hide the column header
const LVS_NOCOLUMNHEADER: u32 = 0x4000;
/// `LVS_OWNERDATA`: virtual / owner-data mode — control stores no item text
const LVS_OWNERDATA: u32 = 0x1000;
/// `LVS_SINGLESEL`: single selection only
const LVS_SINGLESEL: u32 = 0x0004;
/// `LVS_SHOWSELALWAYS`: keep selection highlighted even when not focused
const LVS_SHOWSELALWAYS: u32 = 0x0008;

/// `LVS_EX_FULLROWSELECT`: highlight the whole row
const LVS_EX_FULLROWSELECT: u32 = 0x0000_0020;
/// `LVS_EX_DOUBLEBUFFER`: double-buffer painting to eliminate flicker
const LVS_EX_DOUBLEBUFFER: u32 = 0x0001_0000;

// ListView messages (LVM_FIRST = 0x1000)
const LVM_FIRST: u32 = 0x1000;
const LVM_ENSUREVISIBLE: u32 = LVM_FIRST + 19;
const LVM_REDRAWITEMS: u32 = LVM_FIRST + 21;
const LVM_SETCOLUMNWIDTH: u32 = LVM_FIRST + 30;
const LVM_SETITEMSTATE: u32 = LVM_FIRST + 43;
const LVM_SETITEMCOUNT: u32 = LVM_FIRST + 47;
const LVM_SETEXTENDEDLISTVIEWSTYLE: u32 = LVM_FIRST + 54;
const LVM_INSERTCOLUMNW: u32 = LVM_FIRST + 97;

// ListView notification codes (LVN_FIRST = –100)
/// `LVN_ITEMCHANGED` = –101, as `u32` (for comparison with `NMHDR.code: u32`)
const LVN_ITEMCHANGED: u32 = (-101_i32) as u32;
/// `LVN_GETDISPINFOW` = –177
const LVN_GETDISPINFOW: u32 = (-177_i32) as u32;

// Item / column flag bits
const LVIF_TEXT: u32 = 0x0001;
const LVIF_STATE: u32 = 0x0008;
const LVIS_FOCUSED: u32 = 0x0001;
const LVIS_SELECTED: u32 = 0x0002;
/// `LVSICF_NOINVALIDATEALL`: preserve existing item states when count changes
const LVSICF_NOINVALIDATEALL: isize = 0x0001;

// ── Public types ─────────────────────────────────────────────────────────────

/// Keyboard event info passed to `on_key_down` callbacks.
#[derive(Clone, Copy)]
pub struct KeyInfo {
	pub key_code: i32,
	pub ctrl: bool,
	pub shift: bool,
	pub alt: bool,
}

// ── Inner state ──────────────────────────────────────────────────────────────

struct Inner {
	#[cfg(windows)]
	listview_hwnd: windows::Win32::Foundation::HWND,
	items: Vec<(String, Vec<u16>)>,
	/// Mirrors the Win32 selection state so `get_selection()` is immune to
	/// `WM_SETREDRAW` suppression and `LVM_SETITEMCOUNT` state resets.
	selected_index: std::cell::Cell<Option<usize>>,
	on_selection_changed: Option<Box<dyn Fn(usize)>>,
	on_key_down: Option<Box<dyn Fn(KeyInfo) -> bool>>,
	on_context_menu: Option<Box<dyn Fn()>>,
}

// ── TimelinePanel ─────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct TimelinePanel {
	panel: Panel,
	inner: Rc<RefCell<Inner>>,
}

impl wxdragon::WxWidget for TimelinePanel {
	fn handle_ptr(&self) -> *mut wxdragon::ffi::wxd_Window_t {
		self.panel.handle_ptr()
	}
}

impl TimelinePanel {
	/// Create a new `TimelinePanel` as a child of `parent`.
	pub fn new(parent: &impl wxdragon::WxWidget) -> Self {
		let panel = Panel::builder(parent).build();

		let inner = Rc::new(RefCell::new(Inner {
			#[cfg(windows)]
			listview_hwnd: windows::Win32::Foundation::HWND(std::ptr::null_mut()),
			items: Vec::new(),
			selected_index: std::cell::Cell::new(None),
			on_selection_changed: None,
			on_key_down: None,
			on_context_menu: None,
		}));

		#[cfg(windows)]
		{
			use windows::Win32::{
				Foundation::{HWND, LPARAM, WPARAM},
				UI::{
					Controls::{LVCF_WIDTH, LVCOLUMNW},
					Shell::SetWindowSubclass,
					WindowsAndMessaging::{
						CreateWindowExW, SWP_NOACTIVATE, SWP_NOZORDER, SendMessageW, SetWindowPos, WINDOW_EX_STYLE,
						WINDOW_STYLE, WS_CHILD, WS_TABSTOP, WS_VISIBLE,
					},
				},
			};

			let panel_hwnd = HWND(panel.get_handle());

			// Create the SysListView32 child window in virtual/owner-data mode.
			let lv_style = WINDOW_STYLE(
				WS_CHILD.0
					| WS_VISIBLE.0 | WS_TABSTOP.0
					| LVS_REPORT | LVS_NOCOLUMNHEADER
					| LVS_OWNERDATA | LVS_SINGLESEL
					| LVS_SHOWSELALWAYS,
			);
			let listview_hwnd = unsafe {
				CreateWindowExW(
					WINDOW_EX_STYLE(0),
					windows::core::w!("SysListView32"),
					windows::core::w!(""),
					lv_style,
					0,
					0,
					100,
					100,
					Some(panel_hwnd),
					None,
					None,
					None,
				)
				.unwrap_or(HWND(std::ptr::null_mut()))
			};

			// Set extended LV styles: full-row select + double-buffer.
			unsafe {
				SendMessageW(
					listview_hwnd,
					LVM_SETEXTENDEDLISTVIEWSTYLE,
					Some(WPARAM((LVS_EX_FULLROWSELECT | LVS_EX_DOUBLEBUFFER) as usize)),
					Some(LPARAM((LVS_EX_FULLROWSELECT | LVS_EX_DOUBLEBUFFER) as isize)),
				);
			}

			// Insert one column so the listview has something to display.
			let col = LVCOLUMNW { mask: LVCF_WIDTH, cx: 100, ..Default::default() };
			unsafe {
				SendMessageW(
					listview_hwnd,
					LVM_INSERTCOLUMNW,
					Some(WPARAM(0)),
					Some(LPARAM(std::ptr::addr_of!(col) as isize)),
				);
			}

			// Store the HWND in Inner.
			inner.borrow_mut().listview_hwnd = listview_hwnd;

			// Make the listview fill the panel whenever it resizes.
			unsafe {
				SetWindowPos(listview_hwnd, None, 0, 0, 100, 100, SWP_NOZORDER | SWP_NOACTIVATE).unwrap_or(());
			}

			// Subclass the panel to handle WM_NOTIFY and WM_SIZE.
			let ref_data = Rc::as_ptr(&inner) as usize;
			unsafe {
				let _ = SetWindowSubclass(panel_hwnd, Some(panel_subclass_proc), 0, ref_data);
			}

			// Subclass the listview to handle WM_KEYDOWN and WM_CONTEXTMENU.
			unsafe {
				let _ = SetWindowSubclass(listview_hwnd, Some(listview_subclass_proc), 0, ref_data);
			}
		}

		Self { panel, inner }
	}

	// ── Data manipulation ─────────────────────────────────────────────────

	/// Append an item to the end of the list.
	pub fn append(&self, text: &str) {
		let mut inner = self.inner.borrow_mut();
		let mut wstr = text.encode_utf16().collect::<Vec<_>>();
		wstr.push(0);
		inner.items.push((text.to_string(), wstr));
		let count = inner.items.len();
		#[cfg(windows)]
		unsafe {
			use windows::Win32::{
				Foundation::{LPARAM, WPARAM},
				UI::WindowsAndMessaging::SendMessageW,
			};
			SendMessageW(inner.listview_hwnd, LVM_SETITEMCOUNT, Some(WPARAM(count)), Some(LPARAM(0)));
		}
	}

	/// Replace the text of an existing item. Does NOT fire any Win32
	/// accessibility event — that is the whole point of this widget.
	pub fn set_string(&self, index: usize, text: &str) {
		let mut inner = self.inner.borrow_mut();
		if let Some(slot) = inner.items.get_mut(index) {
			if slot.0 == text {
				return; // nothing to do
			}
			let mut wstr = text.encode_utf16().collect::<Vec<_>>();
			wstr.push(0);
			*slot = (text.to_string(), wstr);
		} else {
			return;
		}
		#[cfg(windows)]
		unsafe {
			use windows::Win32::{
				Foundation::{LPARAM, WPARAM},
				UI::WindowsAndMessaging::SendMessageW,
			};
			// LVM_REDRAWITEMS repaints items without firing NAMECHANGE.
			SendMessageW(inner.listview_hwnd, LVM_REDRAWITEMS, Some(WPARAM(index)), Some(LPARAM(index as isize)));
		}
	}

	/// Return the text of item at `index`, or `None` if out of range.
	pub fn get_string(&self, index: usize) -> Option<String> {
		self.inner.borrow().items.get(index).map(|(s, _)| s.clone())
	}

	/// Return the number of items.
	pub fn get_count(&self) -> usize {
		self.inner.borrow().items.len()
	}

	/// Remove all items.
	pub fn clear(&self) {
		let mut inner = self.inner.borrow_mut();
		inner.items.clear();
		inner.selected_index.set(None);
		#[cfg(windows)]
		unsafe {
			use windows::Win32::{
				Foundation::{LPARAM, WPARAM},
				Graphics::Gdi::InvalidateRect,
				UI::WindowsAndMessaging::SendMessageW,
			};
			SendMessageW(inner.listview_hwnd, LVM_SETITEMCOUNT, Some(WPARAM(0)), Some(LPARAM(0)));
			let _ = InvalidateRect(Some(inner.listview_hwnd), None, true);
		}
	}

	/// Replace all items atomically, firing a single `LVM_SETITEMCOUNT` and
	/// preserving the Win32 selection state (so `apply_timeline_selection`
	/// can skip `set_selection` when the selection index has not changed).
	pub fn replace_all(&self, items: impl IntoIterator<Item = String>) {
		let mut inner = self.inner.borrow_mut();
		inner.items.clear();
		inner.items.extend(items.into_iter().map(|s| {
			let mut wstr = s.encode_utf16().collect::<Vec<_>>();
			wstr.push(0);
			(s, wstr)
		}));
		let count = inner.items.len();
		// If the selected item is now out of range, clear the selection mirror.
		if inner.selected_index.get().is_some_and(|s| s >= count) {
			inner.selected_index.set(None);
		}
		#[cfg(windows)]
		unsafe {
			use windows::Win32::{
				Foundation::{LPARAM, WPARAM},
				Graphics::Gdi::InvalidateRect,
				UI::WindowsAndMessaging::SendMessageW,
			};
			// LVSICF_NOINVALIDATEALL preserves selection state for existing items.
			SendMessageW(
				inner.listview_hwnd,
				LVM_SETITEMCOUNT,
				Some(WPARAM(count)),
				Some(LPARAM(LVSICF_NOINVALIDATEALL)),
			);
			let _ = InvalidateRect(Some(inner.listview_hwnd), None, true);
		}
	}

	// ── Selection ─────────────────────────────────────────────────────────

	/// Return the currently selected item index, or `None` if nothing is selected.
	/// Reads from a cached mirror rather than querying Win32 directly, making it
	/// immune to `WM_SETREDRAW` suppression or `LVM_SETITEMCOUNT` state resets.
	pub fn get_selection(&self) -> Option<usize> {
		self.inner.borrow().selected_index.get()
	}

	/// Select the item at `index`, scrolling it into view.
	pub fn set_selection(&self, index: usize) {
		#[cfg(windows)]
		unsafe {
			use windows::Win32::{
				Foundation::{LPARAM, WPARAM},
				UI::{
					Controls::{LIST_VIEW_ITEM_FLAGS, LIST_VIEW_ITEM_STATE_FLAGS, LVITEMW},
					WindowsAndMessaging::SendMessageW,
				},
			};
			let inner = self.inner.borrow();
			inner.selected_index.set(Some(index));
			let lv = inner.listview_hwnd;

			// Select + focus the requested item. LVS_SINGLESEL handles deselecting automatically.
			let sel = LVITEMW {
				stateMask: LIST_VIEW_ITEM_STATE_FLAGS(LVIS_SELECTED | LVIS_FOCUSED),
				state: LIST_VIEW_ITEM_STATE_FLAGS(LVIS_SELECTED | LVIS_FOCUSED),
				mask: LIST_VIEW_ITEM_FLAGS(LVIF_STATE),
				..Default::default()
			};
			SendMessageW(lv, LVM_SETITEMSTATE, Some(WPARAM(index)), Some(LPARAM(std::ptr::addr_of!(sel) as isize)));

			// Scroll the item into view.
			SendMessageW(lv, LVM_ENSUREVISIBLE, Some(WPARAM(index)), Some(LPARAM(0)));
		}
	}

	// ── Painting control ──────────────────────────────────────────────────

	/// Suspend repainting of the list (use before bulk updates).
	pub fn freeze(&self) {
		#[cfg(windows)]
		unsafe {
			use windows::Win32::{
				Foundation::{LPARAM, WPARAM},
				UI::WindowsAndMessaging::{SendMessageW, WM_SETREDRAW},
			};
			let lv = self.inner.borrow().listview_hwnd;
			SendMessageW(lv, WM_SETREDRAW, Some(WPARAM(0)), Some(LPARAM(0)));
		}
	}

	/// Resume repainting and repaint immediately.
	pub fn thaw(&self) {
		#[cfg(windows)]
		unsafe {
			use windows::Win32::{
				Foundation::{LPARAM, WPARAM},
				Graphics::Gdi::InvalidateRect,
				UI::WindowsAndMessaging::{SendMessageW, WM_SETREDRAW},
			};
			let lv = self.inner.borrow().listview_hwnd;
			SendMessageW(lv, WM_SETREDRAW, Some(WPARAM(1)), Some(LPARAM(0)));
			let _ = InvalidateRect(Some(lv), None, true);
		}
	}

	/// Show a popup menu at the current cursor position.
	pub fn popup_menu(&self, menu: &mut Menu, pos: Option<Point>) {
		self.panel.popup_menu(menu, pos);
	}

	// ── Event callbacks ───────────────────────────────────────────────────

	/// Register a callback invoked when the selected item changes.
	/// The argument is the newly selected list index.
	pub fn on_selection_changed(&self, cb: impl Fn(usize) + 'static) {
		self.inner.borrow_mut().on_selection_changed = Some(Box::new(cb));
	}

	/// Register a callback invoked on key-down in the list.
	/// Return `true` to consume the key (prevent default), `false` to let it pass.
	pub fn on_key_down(&self, cb: impl Fn(KeyInfo) -> bool + 'static) {
		self.inner.borrow_mut().on_key_down = Some(Box::new(cb));
	}

	/// Register a callback invoked when the context menu is requested.
	pub fn on_context_menu(&self, cb: impl Fn() + 'static) {
		self.inner.borrow_mut().on_context_menu = Some(Box::new(cb));
	}
}

// ── Win32 subclass procs ──────────────────────────────────────────────────────

/// Translates a Win32 virtual-key code to the equivalent wxWidgets key code.
#[cfg(windows)]
const fn vk_to_wx_key(vk: usize) -> Option<i32> {
	match vk {
		0x08 => Some(8),                              // VK_BACK → Backspace
		0x0D => Some(13),                             // VK_RETURN → Enter
		0x24 => Some(313),                            // VK_HOME
		0x25 => Some(314),                            // VK_LEFT
		0x26 => Some(315),                            // VK_UP
		0x27 => Some(316),                            // VK_RIGHT
		0x28 => Some(317),                            // VK_DOWN
		0x2E => Some(127),                            // VK_DELETE → KEY_DELETE
		0x72 => Some(342),                            // VK_F3
		0xBE => Some(46),                             // VK_OEM_PERIOD → '.'
		0xBF => Some(191),                            // VK_OEM_2 → '/'
		0xDB => Some(91),                             // VK_OEM_4 → '['
		0xDD => Some(93),                             // VK_OEM_6 → ']'
		0x30..=0x39 | 0x41..=0x5A => Some(vk as i32), // 0–9 and A–Z
		_ => None,
	}
}

/// Subclass proc for the wx **panel** — handles `WM_NOTIFY`, `WM_SIZE`, and `WM_SETFOCUS`.
#[cfg(windows)]
unsafe extern "system" fn panel_subclass_proc(
	hwnd: windows::Win32::Foundation::HWND,
	msg: u32,
	wparam: windows::Win32::Foundation::WPARAM,
	lparam: windows::Win32::Foundation::LPARAM,
	_uid_subclass: usize,
	ref_data: usize,
) -> windows::Win32::Foundation::LRESULT {
	use windows::Win32::{
		Foundation::{LPARAM, LRESULT, WPARAM},
		UI::{
			Controls::{NMLISTVIEW, NMLVDISPINFOW},
			Input::KeyboardAndMouse::SetFocus,
			Shell::DefSubclassProc,
			WindowsAndMessaging::{
				SWP_NOACTIVATE, SWP_NOZORDER, SendMessageW, SetWindowPos, WM_NOTIFY, WM_SETFOCUS, WM_SIZE,
			},
		},
	};

	match msg {
		WM_NOTIFY => {
			let nmhdr = &*(lparam.0 as *const windows::Win32::UI::Controls::NMHDR);
			let code = nmhdr.code;

			if code == LVN_GETDISPINFOW {
				let disp = &mut *(lparam.0 as *mut NMLVDISPINFOW);
				if disp.item.mask.0 & LVIF_TEXT != 0 {
					let item_index = disp.item.iItem as usize;
					let inner = &*(ref_data as *const RefCell<Inner>);
					if let Ok(borrow) = inner.try_borrow()
						&& let Some((_, text_wstr)) = borrow.items.get(item_index)
					{
						disp.item.pszText = windows::core::PWSTR(text_wstr.as_ptr() as *mut u16);
					}
				}
				return LRESULT(0);
			}

			if code == LVN_ITEMCHANGED {
				let nmlv = &*(lparam.0 as *const NMLISTVIEW);
				// Fire on_selection_changed only when an item *gains* selection.
				// uChanged is LIST_VIEW_ITEM_FLAGS, uNewState is plain u32.
				if nmlv.uChanged.0 & LVIF_STATE != 0 && nmlv.uNewState & LVIS_SELECTED != 0 && nmlv.iItem >= 0 {
					let item_index = nmlv.iItem as usize;
					// Get a raw pointer to the callback *before* releasing the borrow,
					// then call through it.  Safe because Inner is alive (Rc exists)
					// and we're on the single UI thread with no concurrent mutation.
					let inner = &*(ref_data as *const RefCell<Inner>);
					if let Ok(borrow) = inner.try_borrow() {
						borrow.selected_index.set(Some(item_index));
					}
					let cb_ptr: Option<*const dyn Fn(usize)> = inner
						.try_borrow()
						.ok()
						.and_then(|b| b.on_selection_changed.as_ref().map(|cb| std::ptr::from_ref(cb.as_ref())));
					if let Some(ptr) = cb_ptr {
						(*ptr)(item_index);
					}
				}
				return LRESULT(0);
			}

			DefSubclassProc(hwnd, msg, wparam, lparam)
		}

		WM_SIZE => {
			// Resize the listview to fill the panel.
			let lp = lparam.0 as u32;
			let width = (lp & 0xFFFF) as i32;
			let height = ((lp >> 16) & 0xFFFF) as i32;
			let inner = &*(ref_data as *const RefCell<Inner>);
			if let Ok(borrow) = inner.try_borrow() {
				let lv = borrow.listview_hwnd;
				SetWindowPos(lv, None, 0, 0, width, height, SWP_NOZORDER | SWP_NOACTIVATE).unwrap_or(());
				// Stretch the single column to the full width.
				SendMessageW(lv, LVM_SETCOLUMNWIDTH, Some(WPARAM(0)), Some(LPARAM(width as isize)));
			}
			DefSubclassProc(hwnd, msg, wparam, lparam)
		}

		WM_SETFOCUS => {
			// Redirect keyboard focus from the container panel to the inner ListView
			// so that Tab navigation lands on the list, not the panel.
			let inner = &*(ref_data as *const RefCell<Inner>);
			if let Ok(borrow) = inner.try_borrow() {
				let _ = SetFocus(Some(borrow.listview_hwnd));
			}
			DefSubclassProc(hwnd, msg, wparam, lparam)
		}

		_ => DefSubclassProc(hwnd, msg, wparam, lparam),
	}
}

/// Subclass proc for the **listview** — handles `WM_KEYDOWN` and `WM_CONTEXTMENU`.
#[cfg(windows)]
unsafe extern "system" fn listview_subclass_proc(
	hwnd: windows::Win32::Foundation::HWND,
	msg: u32,
	wparam: windows::Win32::Foundation::WPARAM,
	lparam: windows::Win32::Foundation::LPARAM,
	_uid_subclass: usize,
	ref_data: usize,
) -> windows::Win32::Foundation::LRESULT {
	use windows::Win32::{
		Foundation::LRESULT,
		UI::{
			Input::KeyboardAndMouse::{GetKeyState, VK_CONTROL, VK_MENU, VK_SHIFT},
			Shell::DefSubclassProc,
			WindowsAndMessaging::{WM_CONTEXTMENU, WM_KEYDOWN},
		},
	};

	match msg {
		WM_KEYDOWN => {
			let vk = wparam.0; // virtual key code
			if let Some(key_code) = vk_to_wx_key(vk) {
				let ctrl = GetKeyState(i32::from(VK_CONTROL.0)) < 0;
				let shift = GetKeyState(i32::from(VK_SHIFT.0)) < 0;
				let alt = GetKeyState(i32::from(VK_MENU.0)) < 0;
				let key_info = KeyInfo { key_code, ctrl, shift, alt };
				let inner = &*(ref_data as *const RefCell<Inner>);
				let cb_ptr: Option<*const dyn Fn(KeyInfo) -> bool> = inner
					.try_borrow()
					.ok()
					.and_then(|b| b.on_key_down.as_ref().map(|cb| std::ptr::from_ref(cb.as_ref())));
				if let Some(ptr) = cb_ptr {
					let consumed = (*ptr)(key_info);
					if consumed {
						return LRESULT(0);
					}
				}
			}
			DefSubclassProc(hwnd, msg, wparam, lparam)
		}

		WM_CONTEXTMENU => {
			let inner = &*(ref_data as *const RefCell<Inner>);
			let cb_ptr: Option<*const dyn Fn()> = inner
				.try_borrow()
				.ok()
				.and_then(|b| b.on_context_menu.as_ref().map(|cb| std::ptr::from_ref(cb.as_ref())));
			if let Some(ptr) = cb_ptr {
				(*ptr)();
			}
			LRESULT(0)
		}

		_ => DefSubclassProc(hwnd, msg, wparam, lparam),
	}
}
