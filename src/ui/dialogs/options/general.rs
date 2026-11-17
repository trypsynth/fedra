use wxdragon::prelude::*;

use super::{TabSizer, checkbox};
use crate::config::Config;

#[allow(clippy::struct_excessive_bools, reason = "one field per checkbox")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneralSettings {
	pub always_show_link_dialog: bool,
	pub strip_tracking: bool,
	pub quick_action_keys: bool,
	pub start_maximized: bool,
}

impl GeneralSettings {
	pub const fn load(config: &Config) -> Self {
		Self {
			always_show_link_dialog: config.always_show_link_dialog,
			strip_tracking: config.strip_tracking,
			quick_action_keys: config.quick_action_keys,
			start_maximized: config.start_maximized,
		}
	}

	pub const fn save(&self, config: &mut Config) {
		config.always_show_link_dialog = self.always_show_link_dialog;
		config.strip_tracking = self.strip_tracking;
		config.quick_action_keys = self.quick_action_keys;
		config.start_maximized = self.start_maximized;
	}
}

pub struct GeneralTab {
	pub panel: Panel,
	link_dialog: CheckBox,
	strip_tracking: CheckBox,
	quick_action_keys: CheckBox,
	start_maximized: CheckBox,
}

impl GeneralTab {
	pub fn new(panel: Panel, initial: &GeneralSettings) -> Self {
		let link_dialog = checkbox(panel, "Always prompt to open &links", initial.always_show_link_dialog);
		let strip_tracking = checkbox(panel, "Strip &tracking parameters from URLs", initial.strip_tracking);
		let quick_action_keys = checkbox(panel, "Use &quick action keys in timelines", initial.quick_action_keys);
		let start_maximized = checkbox(panel, "Start &maximized", initial.start_maximized);
		let sizer = TabSizer::new();
		for control in [&link_dialog, &strip_tracking, &quick_action_keys, &start_maximized] {
			sizer.add(control);
		}
		sizer.finish(panel);
		Self { panel, link_dialog, strip_tracking, quick_action_keys, start_maximized }
	}

	pub fn values(&self) -> GeneralSettings {
		GeneralSettings {
			always_show_link_dialog: self.link_dialog.get_value(),
			strip_tracking: self.strip_tracking.get_value(),
			quick_action_keys: self.quick_action_keys.get_value(),
			start_maximized: self.start_maximized.get_value(),
		}
	}
}
