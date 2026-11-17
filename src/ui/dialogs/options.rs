//! The Options dialog. Each tab lives in its own module, with the settings it edits and how they load and save.

use wx_utils::{add_ok_cancel_footer, build_ok_cancel_buttons_on};
#[cfg(windows)]
use wxdragon::accessible::AccRole;
use wxdragon::prelude::*;

use crate::config::Config;

mod advanced;
mod composing;
mod display;
mod filters;
mod general;
mod invisible;
mod notifications;
mod templates;
mod timelines;

pub use advanced::AdvancedSettings;
pub use composing::ComposingSettings;
pub use display::DisplaySettings;
pub use filters::FilterSettings;
pub use general::GeneralSettings;
pub use invisible::InvisibleSettings;
pub use notifications::NotificationSettings;
pub use templates::TemplateSettings;
pub use timelines::TimelineSettings;

/// Space around each option, inside its tab.
const PADDING: i32 = 8;

/// Everything the Options dialog edits, by tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptionsDialogResult {
	pub general: GeneralSettings,
	pub composing: ComposingSettings,
	pub timelines: TimelineSettings,
	pub display: DisplaySettings,
	pub notifications: NotificationSettings,
	pub templates: TemplateSettings,
	pub filters: FilterSettings,
	pub invisible: InvisibleSettings,
	pub advanced: AdvancedSettings,
}

impl OptionsDialogResult {
	pub fn load(config: &Config) -> Self {
		Self {
			general: GeneralSettings::load(config),
			composing: ComposingSettings::load(config),
			timelines: TimelineSettings::load(config),
			display: DisplaySettings::load(config),
			notifications: NotificationSettings::load(config),
			templates: TemplateSettings::load(config),
			filters: FilterSettings::load(config),
			invisible: InvisibleSettings::load(config),
			advanced: AdvancedSettings::load(config),
		}
	}

	pub fn save(self, config: &mut Config) {
		self.general.save(config);
		self.composing.save(config);
		self.timelines.save(config);
		self.display.save(config);
		self.notifications.save(config);
		self.templates.save(config);
		self.filters.save(config);
		self.invisible.save(config);
		self.advanced.save(config);
	}
}

pub fn prompt_for_options(frame: &Frame, initial: OptionsDialogResult) -> Option<OptionsDialogResult> {
	let dialog = Dialog::builder(frame, "Options").with_size(500, 600).build();
	let panel = Panel::builder(&dialog).build();
	let notebook = Notebook::builder(&panel).build();
	let page = || Panel::builder(&notebook).with_style(PanelStyle::TabTraversal).build();
	let general = general::GeneralTab::new(page(), &initial.general);
	let composing = composing::ComposingTab::new(page(), &initial.composing);
	let timelines = timelines::TimelinesTab::new(page(), dialog, initial.timelines);
	let display = display::DisplayTab::new(page(), &initial.display);
	let notifications = notifications::NotificationsTab::new(page(), dialog, initial.notifications);
	let templates = templates::TemplatesTab::new(page(), dialog, &initial.templates);
	let filters = filters::FiltersTab::new(page(), initial.filters);
	let invisible = invisible::InvisibleTab::new(page(), &initial.invisible);
	let advanced = advanced::AdvancedTab::new(page(), &initial.advanced);
	let mut tabs = vec![
		(general.panel, "General"),
		(composing.panel, "Composing"),
		(timelines.panel, "Timelines"),
		(display.panel, "Display"),
		(notifications.panel, "Notifications"),
		(templates.panel, "Templates"),
		(filters.panel, "Filters"),
	];
	// Global shortcuts only exist on Windows so far.
	if cfg!(windows) {
		tabs.push((invisible.panel, "Invisible Interface"));
	} else {
		invisible.panel.show(false);
	}
	tabs.push((advanced.panel, "Advanced"));
	for (index, (tab, label)) in tabs.iter().enumerate() {
		tab.set_accessibility_label(label);
		#[cfg(windows)]
		tab.set_accessibility_role(AccRole::PropertyPage);
		notebook.add_page(tab, label, index == 0, None);
	}
	let main_sizer = BoxSizer::builder(Orientation::Vertical).build();
	main_sizer.add(&notebook, 1, SizerFlag::Expand | SizerFlag::All, PADDING);
	let (ok_button, cancel_button) = build_ok_cancel_buttons_on(&panel, &dialog, "OK");
	add_ok_cancel_footer(main_sizer, ok_button, cancel_button);
	panel.set_sizer(main_sizer, true);
	let dialog_sizer = BoxSizer::builder(Orientation::Vertical).build();
	dialog_sizer.add(&panel, 1, SizerFlag::Expand, 0);
	dialog.set_sizer(dialog_sizer, true);
	dialog.centre();
	let result = (dialog.show_modal() == ID_OK).then(|| OptionsDialogResult {
		general: general.values(),
		composing: composing.values(),
		timelines: timelines.values(),
		display: display.values(),
		notifications: notifications.values(),
		templates: templates.values(),
		filters: filters.values(),
		invisible: invisible.values(),
		advanced: advanced.values(),
	});
	dialog.destroy();
	result
}

/// A tab's options, one below another, each with room around it.
struct TabSizer(BoxSizer);

impl TabSizer {
	fn new() -> Self {
		Self(BoxSizer::builder(Orientation::Vertical).build())
	}

	fn add(&self, control: &impl WxWidget) {
		self.0.add(control, 0, SizerFlag::Expand | SizerFlag::All, PADDING);
	}

	/// `label` beside the control it names.
	fn add_labeled(&self, label: StaticText, control: &impl WxWidget, expand: bool) {
		let row = BoxSizer::builder(Orientation::Horizontal).build();
		row.add(&label, 0, SizerFlag::AlignCenterVertical | SizerFlag::Right, PADDING);
		row.add(control, i32::from(expand), if expand { SizerFlag::Expand } else { SizerFlag::empty() }, 0);
		self.0.add_sizer(&row, 0, SizerFlag::Expand | SizerFlag::All, PADDING);
	}

	fn finish(self, panel: Panel) {
		self.0.add_stretch_spacer(1);
		panel.set_sizer(self.0, true);
	}
}

/// A choice of `names`, showing the one at `selected`.
fn choice(panel: Panel, names: &[&str], selected: usize) -> Choice {
	let choice = Choice::builder(&panel).with_choices(names.iter().map(ToString::to_string).collect()).build();
	choice.set_selection(u32::try_from(selected).unwrap_or(0));
	choice
}

/// The item of `items` picked in `choice`, or `fallback` if none is.
fn chosen<T: Copy>(choice: Choice, items: &[T], fallback: T) -> T {
	choice.get_selection().and_then(|index| items.get(index as usize).copied()).unwrap_or(fallback)
}

fn checkbox(panel: Panel, label: &str, value: bool) -> CheckBox {
	let checkbox = CheckBox::builder(&panel).with_label(label).build();
	checkbox.set_value(value);
	checkbox
}
