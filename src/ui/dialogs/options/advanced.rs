use logkeeper::Level;
use wxdragon::prelude::*;

use super::{TabSizer, checkbox, choice, chosen};
use crate::config::{Config, UpdateChannel};

const CHANNELS: [UpdateChannel; 2] = [UpdateChannel::Stable, UpdateChannel::Dev];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdvancedSettings {
	pub check_for_updates: bool,
	pub update_channel: UpdateChannel,
	pub log_level: Level,
}

impl AdvancedSettings {
	pub const fn load(config: &Config) -> Self {
		Self {
			check_for_updates: config.check_for_updates_on_startup,
			update_channel: config.update_channel,
			log_level: config.log_level,
		}
	}

	pub const fn save(&self, config: &mut Config) {
		config.check_for_updates_on_startup = self.check_for_updates;
		config.update_channel = self.update_channel;
		config.log_level = self.log_level;
	}
}

pub struct AdvancedTab {
	pub panel: Panel,
	check_for_updates: CheckBox,
	update_channel: Choice,
	log_level: Choice,
	initial: AdvancedSettings,
}

impl AdvancedTab {
	pub fn new(panel: Panel, initial: &AdvancedSettings) -> Self {
		let check_for_updates = checkbox(panel, "Check for &updates on startup", initial.check_for_updates);
		let channel_label = StaticText::builder(&panel).with_label("Update &channel:").build();
		let channel_index = CHANNELS.iter().position(|c| *c == initial.update_channel).unwrap_or(0);
		let update_channel = choice(panel, &["Stable releases", "Test builds"], channel_index);
		let log_level_label = StaticText::builder(&panel).with_label("Lo&g level:").build();
		let log_level_names = Level::ALL.map(log_level_name);
		let log_level_index = Level::ALL.iter().position(|level| *level == initial.log_level).unwrap_or(0);
		let log_level = choice(panel, &log_level_names, log_level_index);
		let sizer = TabSizer::new();
		sizer.add(&check_for_updates);
		sizer.add_labeled(channel_label, &update_channel, true);
		sizer.add_labeled(log_level_label, &log_level, true);
		sizer.finish(panel);
		Self { panel, check_for_updates, update_channel, log_level, initial: initial.clone() }
	}

	pub fn values(&self) -> AdvancedSettings {
		AdvancedSettings {
			check_for_updates: self.check_for_updates.get_value(),
			update_channel: chosen(self.update_channel, &CHANNELS, self.initial.update_channel),
			log_level: chosen(self.log_level, &Level::ALL, self.initial.log_level),
		}
	}
}

const fn log_level_name(level: Level) -> &'static str {
	match level {
		Level::Off => "Off",
		Level::Error => "Errors",
		Level::Warn => "Warnings",
		Level::Info => "Information",
		Level::Debug => "Debug",
		Level::Trace => "Trace (very large log)",
	}
}
