use std::{cell::RefCell, rc::Rc};

use wx_utils::{add_ok_cancel_footer, build_ok_cancel_buttons_on};
use wxdragon::prelude::*;

use super::{TabSizer, choice, chosen};
use crate::config::{Config, NotificationKind, NotificationPreference};

const PREFERENCES: [NotificationPreference; 3] =
	[NotificationPreference::Classic, NotificationPreference::SoundOnly, NotificationPreference::Disabled];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationSettings {
	pub preference: NotificationPreference,
	/// Types left out of timelines.
	pub hidden: Vec<NotificationKind>,
	/// Types that show but never make a sound or notification.
	pub silent: Vec<NotificationKind>,
}

impl NotificationSettings {
	pub fn load(config: &Config) -> Self {
		Self {
			preference: config.notification_preference,
			hidden: config.disabled_notification_types.clone(),
			silent: config.silent_notification_types.clone(),
		}
	}

	pub fn save(self, config: &mut Config) {
		config.notification_preference = self.preference;
		config.disabled_notification_types = self.hidden;
		config.silent_notification_types = self.silent;
	}
}

pub struct NotificationsTab {
	pub panel: Panel,
	preference: Choice,
	types: Rc<RefCell<(Vec<NotificationKind>, Vec<NotificationKind>)>>,
	initial_preference: NotificationPreference,
}

impl NotificationsTab {
	pub fn new(panel: Panel, dialog: Dialog, initial: NotificationSettings) -> Self {
		let preference_label = StaticText::builder(&panel).with_label("&Notifications:").build();
		let preference_index = PREFERENCES.iter().position(|p| *p == initial.preference).unwrap_or(0);
		let preference = choice(panel, &["Operating system notifications", "Sound only", "Disabled"], preference_index);
		let types = Rc::new(RefCell::new((initial.hidden, initial.silent)));
		let types_button = Button::builder(&panel).with_label("Notification &Types...").build();
		let current = types.clone();
		types_button.on_click(move |_| {
			let (hidden, silent) = current.borrow().clone();
			if let Some(updated) = prompt_for_notification_types(&dialog, &hidden, &silent) {
				*current.borrow_mut() = updated;
			}
		});
		let sizer = TabSizer::new();
		sizer.add_labeled(preference_label, &preference, true);
		sizer.add(&types_button);
		sizer.finish(panel);
		Self { panel, preference, types, initial_preference: initial.preference }
	}

	pub fn values(&self) -> NotificationSettings {
		let (hidden, silent) = self.types.borrow().clone();
		NotificationSettings {
			preference: chosen(self.preference, &PREFERENCES, self.initial_preference),
			hidden,
			silent,
		}
	}
}

/// Asks which notification types show in timelines and which of them alert, returning the hidden and the silent ones.
fn prompt_for_notification_types(
	parent: &dyn WxWidget,
	initial_hidden: &[NotificationKind],
	initial_silent: &[NotificationKind],
) -> Option<(Vec<NotificationKind>, Vec<NotificationKind>)> {
	let dialog = Dialog::builder(parent, "Notification Types").with_size(560, 400).build();
	let panel = Panel::builder(&dialog).build();
	let main_sizer = BoxSizer::builder(Orientation::Vertical).build();
	let groups_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	let group = |label: &str, initially_off: &[NotificationKind]| {
		let sizer = StaticBoxSizerBuilder::new_with_label(Orientation::Vertical, &panel, label).build();
		let mut checkboxes = Vec::new();
		for kind in NotificationKind::all() {
			let cb = CheckBox::builder(&panel).with_label(kind.display_name()).build();
			cb.set_value(!initially_off.contains(kind));
			sizer.add(&cb, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Bottom, 5);
			checkboxes.push((cb, *kind));
		}
		groups_sizer.add_sizer(&sizer, 1, SizerFlag::Expand | SizerFlag::All, 5);
		checkboxes
	};
	let shown = group("Show in timelines", initial_hidden);
	let alerting = group("Alert me with a sound or notification", initial_silent);
	main_sizer.add_sizer(&groups_sizer, 1, SizerFlag::Expand | SizerFlag::All, 5);
	let (ok_button, cancel_button) = build_ok_cancel_buttons_on(&panel, &dialog, "OK");
	add_ok_cancel_footer(main_sizer, ok_button, cancel_button);
	panel.set_sizer(main_sizer, true);
	let dialog_sizer = BoxSizer::builder(Orientation::Vertical).build();
	dialog_sizer.add(&panel, 1, SizerFlag::Expand, 0);
	dialog.set_sizer(dialog_sizer, true);
	dialog.centre();
	let result = dialog.show_modal();
	let unchecked = |checkboxes: Vec<(CheckBox, NotificationKind)>| {
		checkboxes.into_iter().filter(|(cb, _)| !cb.get_value()).map(|(_, kind)| kind).collect()
	};
	let types = (unchecked(shown), unchecked(alerting));
	dialog.destroy();
	(result == ID_OK).then_some(types)
}
