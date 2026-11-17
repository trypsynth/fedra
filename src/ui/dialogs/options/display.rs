use wxdragon::prelude::*;

use super::{TabSizer, checkbox, choice, chosen};
use crate::config::{Config, ContentWarningDisplay, DisplayNameEmojiMode, SortOrder};

const CONTENT_WARNINGS: [ContentWarningDisplay; 3] =
	[ContentWarningDisplay::Inline, ContentWarningDisplay::Hidden, ContentWarningDisplay::WarningOnly];
const EMOJI_MODES: [DisplayNameEmojiMode; 4] = [
	DisplayNameEmojiMode::None,
	DisplayNameEmojiMode::UnicodeOnly,
	DisplayNameEmojiMode::InstanceOnly,
	DisplayNameEmojiMode::All,
];

/// Everything here changes how posts read, so the open timeline is redrawn when any of it does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplaySettings {
	pub content_warning_display: ContentWarningDisplay,
	pub display_name_emoji_mode: DisplayNameEmojiMode,
	pub show_link_previews: bool,
	pub sort_order: SortOrder,
	pub preserve_thread_order: bool,
}

impl DisplaySettings {
	pub const fn load(config: &Config) -> Self {
		Self {
			content_warning_display: config.content_warning_display,
			display_name_emoji_mode: config.display_name_emoji_mode,
			show_link_previews: config.show_link_previews,
			sort_order: config.sort_order,
			preserve_thread_order: config.preserve_thread_order,
		}
	}

	pub const fn save(&self, config: &mut Config) {
		config.content_warning_display = self.content_warning_display;
		config.display_name_emoji_mode = self.display_name_emoji_mode;
		config.show_link_previews = self.show_link_previews;
		config.sort_order = self.sort_order;
		config.preserve_thread_order = self.preserve_thread_order;
	}
}

pub struct DisplayTab {
	pub panel: Panel,
	content_warnings: Choice,
	emoji_mode: Choice,
	link_previews: CheckBox,
	oldest_first: CheckBox,
	thread_order: CheckBox,
	initial: DisplaySettings,
}

impl DisplayTab {
	pub fn new(panel: Panel, initial: &DisplaySettings) -> Self {
		let cw_label = StaticText::builder(&panel).with_label("&Content warnings:").build();
		let cw_index = CONTENT_WARNINGS.iter().position(|c| *c == initial.content_warning_display).unwrap_or(0);
		let content_warnings = choice(panel, &["Show inline", "Don't show", "CW only"], cw_index);
		let emoji_label = StaticText::builder(&panel).with_label("Hide &emoji in names:").build();
		let emoji_index = EMOJI_MODES.iter().position(|m| *m == initial.display_name_emoji_mode).unwrap_or(0);
		let emoji_mode = choice(panel, &["None", "Unicode emojis", "Instance emojis", "All"], emoji_index);
		let link_previews = checkbox(panel, "Read link &previews in timelines", initial.show_link_previews);
		let oldest_first =
			checkbox(panel, "Show oldest timeline entries &first", initial.sort_order == SortOrder::OldestToNewest);
		let thread_order = checkbox(panel, "Always preserve thread &order", initial.preserve_thread_order);
		let sizer = TabSizer::new();
		sizer.add_labeled(cw_label, &content_warnings, true);
		sizer.add_labeled(emoji_label, &emoji_mode, true);
		sizer.add(&link_previews);
		sizer.add(&oldest_first);
		sizer.add(&thread_order);
		sizer.finish(panel);
		Self {
			panel,
			content_warnings,
			emoji_mode,
			link_previews,
			oldest_first,
			thread_order,
			initial: initial.clone(),
		}
	}

	pub fn values(&self) -> DisplaySettings {
		DisplaySettings {
			content_warning_display: chosen(
				self.content_warnings,
				&CONTENT_WARNINGS,
				self.initial.content_warning_display,
			),
			display_name_emoji_mode: chosen(self.emoji_mode, &EMOJI_MODES, self.initial.display_name_emoji_mode),
			show_link_previews: self.link_previews.get_value(),
			sort_order: if self.oldest_first.get_value() {
				SortOrder::OldestToNewest
			} else {
				SortOrder::NewestToOldest
			},
			preserve_thread_order: self.thread_order.get_value(),
		}
	}
}
