use wxdragon::prelude::*;

use super::{TabSizer, checkbox};
use crate::{config::Config, ui::spell_check};

#[allow(clippy::struct_excessive_bools, reason = "one field per checkbox")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComposingSettings {
	pub enter_to_send: bool,
	pub confirm_discard: bool,
	pub spell_check: bool,
	pub spell_check_announce: bool,
	/// A spell checker language tag. Empty means the Windows language.
	pub spell_language: String,
}

impl ComposingSettings {
	pub fn load(config: &Config) -> Self {
		Self {
			enter_to_send: config.enter_to_send,
			confirm_discard: config.confirm_discard,
			spell_check: config.spell_check,
			spell_check_announce: config.spell_check_announce,
			spell_language: config.spell_language.clone(),
		}
	}

	pub fn save(self, config: &mut Config) {
		config.enter_to_send = self.enter_to_send;
		config.confirm_discard = self.confirm_discard;
		config.spell_check = self.spell_check;
		config.spell_check_announce = self.spell_check_announce;
		config.spell_language = self.spell_language;
	}
}

pub struct ComposingTab {
	pub panel: Panel,
	enter_to_send: CheckBox,
	confirm_discard: CheckBox,
	spell_check: CheckBox,
	spell_check_announce: CheckBox,
	spell_language: Choice,
	/// Tags and names, in the order the language choice lists them after the Windows default.
	languages: Vec<(String, String)>,
}

impl ComposingTab {
	pub fn new(panel: Panel, initial: &ComposingSettings) -> Self {
		let enter_to_send = checkbox(panel, "Use &enter to send posts", initial.enter_to_send);
		let confirm_discard = checkbox(panel, "As&k before discarding a post", initial.confirm_discard);
		let spell_check =
			checkbox(panel, "Check s&pelling in the post box (F7 goes to the next mistake)", initial.spell_check);
		let spell_check_announce =
			checkbox(panel, "Say when a word you type is mis&spelled", initial.spell_check_announce);
		let languages = spell_check::languages();
		let language_label = StaticText::builder(&panel).with_label("Spelling &language:").build();
		let mut names = vec!["Windows default".to_string()];
		names.extend(languages.iter().map(|(_, name)| name.clone()));
		let spell_language = Choice::builder(&panel).with_choices(names).build();
		let selected = languages
			.iter()
			.position(|(tag, _)| tag.eq_ignore_ascii_case(&initial.spell_language))
			.map_or(0, |i| u32::try_from(i).unwrap_or(0) + 1);
		spell_language.set_selection(selected);
		let sizer = TabSizer::new();
		sizer.add(&enter_to_send);
		sizer.add(&confirm_discard);
		sizer.add(&spell_check);
		sizer.add(&spell_check_announce);
		sizer.add_labeled(language_label, &spell_language, true);
		if !spell_check::AVAILABLE {
			// Kept so the saved values carry over, but there's nothing to set them on.
			spell_check.show(false);
			spell_check_announce.show(false);
			language_label.show(false);
			spell_language.show(false);
		}
		sizer.finish(panel);
		Self { panel, enter_to_send, confirm_discard, spell_check, spell_check_announce, spell_language, languages }
	}

	pub fn values(&self) -> ComposingSettings {
		ComposingSettings {
			enter_to_send: self.enter_to_send.get_value(),
			confirm_discard: self.confirm_discard.get_value(),
			spell_check: self.spell_check.get_value(),
			spell_check_announce: self.spell_check_announce.get_value(),
			spell_language: self
				.spell_language
				.get_selection()
				.and_then(|i| (i as usize).checked_sub(1))
				.and_then(|i| self.languages.get(i))
				.map(|(tag, _)| tag.clone())
				.unwrap_or_default(),
		}
	}
}
