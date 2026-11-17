use wxdragon::prelude::*;

use super::{TabSizer, checkbox};
use crate::config::Config;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvisibleSettings {
	pub global_keys: bool,
	pub repeat_at_timeline_edges: bool,
}

impl InvisibleSettings {
	pub const fn load(config: &Config) -> Self {
		Self { global_keys: config.global_keys, repeat_at_timeline_edges: config.repeat_at_timeline_edges }
	}

	pub const fn save(&self, config: &mut Config) {
		config.global_keys = self.global_keys;
		config.repeat_at_timeline_edges = self.repeat_at_timeline_edges;
	}
}

pub struct InvisibleTab {
	pub panel: Panel,
	global_keys: CheckBox,
	repeat_at_edges: CheckBox,
}

impl InvisibleTab {
	pub fn new(panel: Panel, initial: &InvisibleSettings) -> Self {
		let global_keys = checkbox(panel, "Use the &invisible interface", initial.global_keys);
		let repeat_at_edges =
			checkbox(panel, "&Repeat the post at the start or end of a timeline", initial.repeat_at_timeline_edges);
		let sizer = TabSizer::new();
		sizer.add(&global_keys);
		sizer.add(&repeat_at_edges);
		sizer.finish(panel);
		Self { panel, global_keys, repeat_at_edges }
	}

	pub fn values(&self) -> InvisibleSettings {
		InvisibleSettings {
			global_keys: self.global_keys.get_value(),
			repeat_at_timeline_edges: self.repeat_at_edges.get_value(),
		}
	}
}
