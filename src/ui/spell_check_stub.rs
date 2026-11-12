//! Spell checking is only built on Windows, where it uses the system spell checker.
//! Other platforms get these no-ops with the same names, so callers need no `cfg`.

use wxdragon::prelude::*;

/// Whether this platform has a spell checker to offer the options dialog.
pub const AVAILABLE: bool = false;

pub fn languages() -> Vec<(String, String)> {
	Vec::new()
}

pub fn configure(_enabled: bool, _announce: bool, _language: &str) {}

pub fn announce_enabled() -> bool {
	false
}

/// A hidden label, kept so callers match the Windows build. Nothing is ever said through it here.
pub fn announcer(parent: &impl WxWidget) -> StaticText {
	let label = StaticText::builder(parent).with_label("").with_size(Size::new(0, 0)).build();
	label.show(false);
	label
}

pub fn text_changed(_field: &TextCtrl) {}

pub fn refresh(_field: &TextCtrl) {}

pub fn install(_field: &TextCtrl, _announcer: StaticText) {}

pub fn detach(_field: &TextCtrl) {}

pub fn next_error(_field: &TextCtrl) {}
