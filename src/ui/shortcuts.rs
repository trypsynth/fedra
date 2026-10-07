//! Dispatch shared by wx key handlers and the macOS event monitor.

use std::cell::Cell;

use wxdragon::prelude::*;

use crate::{
	UiCommand,
	config::ActionId,
	ui::{commands::command_for, dialogs},
	ui_wake::UiCommandSender,
};

pub(super) fn dispatch_action(
	action: ActionId,
	frame: &Frame,
	ui_tx: &UiCommandSender,
	quick_action_keys_enabled: &Cell<bool>,
) {
	match action {
		ActionId::Find => {
			if let Some(query) = dialogs::show_find_dialog(frame) {
				let _ = ui_tx.send(UiCommand::Find(query));
			}
		}
		ActionId::ToggleQuickActionKeys => {
			let enabled = !quick_action_keys_enabled.get();
			quick_action_keys_enabled.set(enabled);
			let _ = ui_tx.send(UiCommand::SetQuickActionKeysEnabled(enabled));
		}
		_ => {
			if let Some(command) = command_for(action) {
				let _ = ui_tx.send(command);
			}
		}
	}
}

#[allow(clippy::fn_params_excessive_bools, reason = "the flags are the current mode and keyboard modifiers")]
pub(super) fn timeline_index_for_key(
	key_code: i32,
	quick_mode: bool,
	ctrl: bool,
	alt: bool,
	shift: bool,
) -> Option<usize> {
	if (i32::from(b'1')..=i32::from(b'9')).contains(&key_code) && (ctrl || (quick_mode && !alt && !shift)) {
		usize::try_from(key_code - i32::from(b'1')).ok()
	} else {
		None
	}
}

#[cfg(test)]
mod tests {
	use super::timeline_index_for_key;

	#[test]
	fn timeline_numbers_respect_quick_mode_and_modifiers() {
		for key in b'1'..=b'9' {
			let key_code = i32::from(key);
			let index = Some(usize::from(key - b'1'));
			assert_eq!(timeline_index_for_key(key_code, true, false, false, false), index);
			assert_eq!(timeline_index_for_key(key_code, false, true, false, false), index);
			assert_eq!(timeline_index_for_key(key_code, false, false, false, false), None);
			// Leave Shift+number and Alt+number available for configured shortcuts.
			assert_eq!(timeline_index_for_key(key_code, true, false, false, true), None);
			assert_eq!(timeline_index_for_key(key_code, true, false, true, false), None);
		}
		for key in ['0', '/', 'F'] {
			assert_eq!(timeline_index_for_key(key as i32, true, true, false, false), None);
		}
	}
}
