use wxdragon::prelude::*;

use crate::mastodon::Language;

/// Where a post's language is picked: from the instance's list when it has one, or typed as a code.
pub enum LanguagePicker {
	Listed(Choice, Vec<String>),
	Typed(ComboBox),
}

impl LanguagePicker {
	pub fn new(parent: &dyn WxWidget, languages: &[Language], initial: Option<&str>) -> Self {
		if languages.is_empty() {
			let codes = ["", "en", "es", "fr", "de", "it", "pt", "ja", "ko", "zh"];
			let combo = ComboBox::builder(parent).with_choices(codes.iter().map(ToString::to_string).collect()).build();
			combo.set_value(initial.unwrap_or_default());
			return Self::Typed(combo);
		}
		let mut codes = vec![String::new()];
		let mut names = vec!["Not set".to_string()];
		for language in languages {
			codes.push(language.code.clone());
			names.push(language.name.clone());
		}
		// A post being edited, or the user's own default, can be in a language the instance doesn't list.
		if let Some(code) = initial
			&& !codes.iter().any(|c| c.eq_ignore_ascii_case(code))
		{
			codes.push(code.to_string());
			names.push(code.to_string());
		}
		let choice = Choice::builder(parent).build();
		// Filled frozen: added one by one as it's drawn, the instance's 180 or so languages took most of a second, every
		// time compose opened.
		choice.freeze();
		for name in &names {
			choice.append(name);
		}
		choice.thaw();
		let selected = initial.and_then(|code| codes.iter().position(|c| c.eq_ignore_ascii_case(code))).unwrap_or(0);
		choice.set_selection(u32::try_from(selected).unwrap_or(0));
		Self::Listed(choice, codes)
	}

	pub fn add_to(&self, sizer: BoxSizer) {
		match self {
			Self::Listed(choice, _) => sizer.add(choice, 1, SizerFlag::Expand, 0),
			Self::Typed(combo) => sizer.add(combo, 1, SizerFlag::Expand, 0),
		};
	}

	pub fn value(&self) -> Option<String> {
		match self {
			Self::Listed(choice, codes) => {
				choice.get_selection().and_then(|i| codes.get(i as usize)).filter(|code| !code.is_empty()).cloned()
			}
			Self::Typed(combo) => normalize_language_code(&combo.get_value()),
		}
	}
}

pub fn normalize_language_code(input: &str) -> Option<String> {
	let trimmed = input.trim();
	if trimmed.is_empty() {
		return None;
	}
	Some(trimmed.to_ascii_lowercase())
}

/// The language the system is set to, as one of the instance's codes when it has a list. The web app falls back to its
/// interface language in the same way when the user has never set a posting language.
pub fn system_language(languages: &[Language]) -> Option<String> {
	let locale = sys_locale::get_locale()?;
	let family = locale.split(['-', '_']).next().unwrap_or_default().to_ascii_lowercase();
	if languages.is_empty() {
		return (!family.is_empty()).then_some(family);
	}
	let locale = locale.replace('_', "-");
	languages
		.iter()
		.find(|l| l.code.eq_ignore_ascii_case(&locale))
		.or_else(|| languages.iter().find(|l| l.code.eq_ignore_ascii_case(&family)))
		.map(|l| l.code.clone())
}
