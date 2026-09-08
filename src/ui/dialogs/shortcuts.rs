//! Fedra's side of the shared Customize Keyboard Shortcuts dialog.
//!
//! The dialog itself lives in `wx_utils::shortcuts`; all that is needed here is to describe
//! Fedra's keymap to it. Quick keys and normal mode are separate keymaps with their own
//! defaults, so the same key can mean different things in each and a conflict in one is not a
//! conflict in the other. That is what `TabKind::SeparateKeymaps` says.

use wx_utils::shortcuts::{ShortcutModel, TabKind};
use wxdragon::prelude::*;

use crate::config::{ActionId, KeyChord, ShortcutsConfig};

/// Tab order. Quick keys comes first because it is the mode most users customize.
const QUICK_KEYS_TAB: usize = 0;

/// A [`ShortcutsConfig`] presented as one tab per input mode.
///
/// A newtype because both the trait and `ShortcutsConfig` are foreign to this module's owner.
#[derive(Clone)]
struct ModeShortcutsModel(ShortcutsConfig);

impl ModeShortcutsModel {
	const fn is_quick(tab: usize) -> bool {
		tab == QUICK_KEYS_TAB
	}
}

impl ShortcutModel for ModeShortcutsModel {
	type Action = ActionId;

	fn tabs(&self) -> Vec<String> {
		vec!["Quick Keys Mode".to_string(), "Normal Mode".to_string()]
	}

	fn tab_kind(&self) -> TabKind {
		TabKind::SeparateKeymaps
	}

	fn actions(&self, _tab: usize) -> Vec<ActionId> {
		ActionId::all().to_vec()
	}

	fn action_name(&self, action: ActionId) -> String {
		action.display_name().to_string()
	}

	fn chord(&self, tab: usize, action: ActionId) -> Option<KeyChord> {
		self.0.get_chord(Self::is_quick(tab), action)
	}

	fn set_chord(&mut self, tab: usize, action: ActionId, chord: Option<KeyChord>) {
		self.0.active_mode_mut(Self::is_quick(tab)).set_chord(action, chord);
	}

	fn reset_action(&mut self, tab: usize, action: ActionId) {
		self.0.active_mode_mut(Self::is_quick(tab)).reset_action(action);
	}

	fn reset_all(&mut self, tab: usize) {
		self.0.active_mode_mut(Self::is_quick(tab)).reset_all();
	}
}

pub fn prompt_for_shortcuts(parent: &dyn WxWidget, initial: &ShortcutsConfig) -> Option<ShortcutsConfig> {
	let model = ModeShortcutsModel(initial.clone());
	wx_utils::shortcuts::prompt_for_shortcuts(parent, &model).map(|updated| updated.0)
}
