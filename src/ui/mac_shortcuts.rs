//! Run shortcuts before Cocoa turns their menu key equivalents into menu selections.

use std::{
	cell::{Cell, RefCell},
	ptr,
	rc::Rc,
};

use block::ConcreteBlock;
use objc::{class, msg_send, runtime::Object, sel, sel_impl};
use wxdragon::{ffi, prelude::*};

use crate::{
	UiCommand,
	config::ShortcutsConfig,
	ui::{
		keys,
		shortcuts::{dispatch_action, timeline_index_for_key},
		window::WindowParts,
	},
	ui_wake::UiCommandSender,
};

// NSEventModifierFlags and NSEventMaskKeyDown, from AppKit.
const SHIFT: usize = 1 << 17;
const CONTROL: usize = 1 << 18;
const OPTION: usize = 1 << 19;
const COMMAND: usize = 1 << 20;
const KEY_DOWN: usize = 1 << 10;

fn wx_key_code(character: u16) -> Option<i32> {
	match character {
		0x7f => Some(keys::BACKSPACE),
		0xf700 => Some(keys::UP),
		0xf701 => Some(keys::DOWN),
		0xf702 => Some(keys::LEFT),
		0xf703 => Some(keys::RIGHT),
		0xf704..=0xf71b => Some(i32::from(character - 0xf704) + keys::F1), // F1 through F24
		0xf728 => Some(keys::DELETE),
		0xf729 => Some(keys::HOME),
		0xf72b => Some(keys::END),
		0xf72c => Some(keys::PAGE_UP),
		0xf72d => Some(keys::PAGE_DOWN),
		_ => {
			let character = char::from_u32(u32::from(character))?;
			if character.is_ascii() { Some(character.to_ascii_uppercase() as i32) } else { None }
		}
	}
}

fn event_key(event: *mut Object) -> Option<(usize, i32)> {
	let (modifiers, character) = unsafe {
		let modifiers: usize = msg_send![event, modifierFlags];
		// Keep AppKit's special key codes: charactersByApplyingModifiers translates
		// arrows, function keys and Delete into different control characters.
		let characters: *mut Object = msg_send![event, charactersIgnoringModifiers];
		let length: usize = msg_send![characters, length];
		if length == 1 {
			let character: u16 = msg_send![characters, characterAtIndex: 0_usize];
			if character == 0x7f || (0xf700..=0xf8ff).contains(&character) {
				return Some((modifiers, wx_key_code(character)?));
			}
		}
		// Unlike charactersIgnoringModifiers, this also removes Shift. Match the
		// base key (e.g. '/' rather than '?') and check its modifiers separately.
		let characters: *mut Object = msg_send![event, charactersByApplyingModifiers: 0_usize];
		let length: usize = msg_send![characters, length];
		if length == 0 {
			return None;
		}
		let character: u16 = msg_send![characters, characterAtIndex: 0_usize];
		(modifiers, character)
	};
	Some((modifiers, wx_key_code(character)?))
}

/// Keep a secondary window's native menu shortcuts visible without selecting
/// their `NSMenuItems` when pressed. The wx menu handler still performs the action.
pub(super) fn install_menu_shortcuts(frame: Frame, shortcuts: &'static [(i32, i32)]) {
	let monitor = ConcreteBlock::new(move |event: *mut Object| -> *mut Object {
		if !frame.is_valid() {
			return event;
		}
		let frame_view = frame.get_handle().cast::<Object>();
		let (frame_window, event_window): (*mut Object, *mut Object) =
			unsafe { (msg_send![frame_view, window], msg_send![event, window]) };
		if frame_window.is_null() || frame_window != event_window {
			return event;
		}
		let Some((modifiers, key_code)) = event_key(event) else { return event };
		if modifiers & (SHIFT | CONTROL | OPTION | COMMAND) != 0 {
			return event;
		}
		let Some((_, id)) = shortcuts.iter().find(|(key, _)| *key == key_code) else { return event };
		unsafe { ffi::wxd_Window_PostMenuCommand(frame.handle_ptr(), *id) };
		ptr::null_mut()
	})
	.copy();
	let token: *mut Object = unsafe {
		let token: *mut Object =
			msg_send![class!(NSEvent), addLocalMonitorForEventsMatchingMask: KEY_DOWN handler: &*monitor];
		let _: *mut Object = msg_send![token, retain];
		token
	};
	frame.on_destroy(move |_| unsafe {
		let _: () = msg_send![class!(NSEvent), removeMonitor: token];
		let _: () = msg_send![token, release];
	});
}

pub(super) fn install(
	parts: &WindowParts,
	ui_tx: UiCommandSender,
	is_shutting_down: Rc<Cell<bool>>,
	quick_action_keys_enabled: Rc<Cell<bool>>,
	shortcuts_cell: Rc<RefCell<ShortcutsConfig>>,
) {
	let frame = parts.frame;
	let timelines_selector = parts.timelines_selector;
	let timeline_list = parts.timeline_list.clone();
	let monitor = ConcreteBlock::new(move |event: *mut Object| -> *mut Object {
		if is_shutting_down.get() || !(timelines_selector.has_focus() || timeline_list.has_focus()) {
			return event;
		}
		// A local NSEvent monitor runs before NSMenu.performKeyEquivalent. Returning null
		// stops only a key that Fedra handled, leaving the visible menu shortcuts intact.
		let Some((modifiers, key_code)) = event_key(event) else { return event };
		// The in-window keymap has no physical-Control modifier. Do not consume a
		// Control key chord as though it were an unmodified quick key.
		if modifiers & CONTROL != 0 {
			return event;
		}
		let quick_mode = quick_action_keys_enabled.get();
		let ctrl = modifiers & COMMAND != 0;
		let alt = modifiers & OPTION != 0;
		let shift = modifiers & SHIFT != 0;
		if let Some(index) = timeline_index_for_key(key_code, quick_mode, ctrl, alt, shift) {
			let _ = ui_tx.send(UiCommand::SwitchTimelineByIndex(index));
			return ptr::null_mut();
		}
		let action = shortcuts_cell.borrow().find_action(quick_mode, key_code, ctrl, alt, shift);
		let Some(action) = action else { return event };
		dispatch_action(action, &frame, &ui_tx, &quick_action_keys_enabled);
		ptr::null_mut()
	})
	.copy();
	unsafe {
		let _: *mut Object =
			msg_send![class!(NSEvent), addLocalMonitorForEventsMatchingMask: KEY_DOWN handler: &*monitor];
	}
}
