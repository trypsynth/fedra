use std::{cell::RefCell, rc::Rc};

use wx_utils::{add_ok_cancel_footer, build_ok_cancel_buttons_on};
use wxdragon::prelude::*;

use super::{TabSizer, checkbox, choice, chosen};
use crate::config::{AutoloadMode, Config, DefaultTimeline, FindLoadingMode};

const AUTOLOAD_MODES: [AutoloadMode; 3] = [AutoloadMode::Never, AutoloadMode::AtEnd, AutoloadMode::AtBoundary];

#[allow(clippy::struct_excessive_bools, reason = "one field per checkbox")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimelineSettings {
	pub restore_open_timelines: bool,
	pub sync_read_position: bool,
	pub load_older_to_restore: bool,
	pub autoload: AutoloadMode,
	pub fetch_limit: u8,
	pub streaming: bool,
	pub refresh_minutes: u8,
	pub find_loading_mode: FindLoadingMode,
	pub default_timelines: Vec<DefaultTimeline>,
}

impl TimelineSettings {
	pub fn load(config: &Config) -> Self {
		Self {
			restore_open_timelines: config.restore_open_timelines,
			sync_read_position: config.sync_read_position,
			load_older_to_restore: config.load_older_to_restore,
			autoload: config.autoload,
			fetch_limit: config.fetch_limit,
			streaming: config.streaming,
			refresh_minutes: config.refresh_minutes,
			find_loading_mode: config.find_loading_mode,
			default_timelines: config.default_timelines.clone(),
		}
	}

	pub fn save(self, config: &mut Config) {
		config.restore_open_timelines = self.restore_open_timelines;
		config.sync_read_position = self.sync_read_position;
		config.load_older_to_restore = self.load_older_to_restore;
		config.autoload = self.autoload;
		config.fetch_limit = self.fetch_limit;
		config.streaming = self.streaming;
		config.refresh_minutes = self.refresh_minutes;
		config.find_loading_mode = self.find_loading_mode;
		config.default_timelines = self.default_timelines;
	}
}

pub struct TimelinesTab {
	pub panel: Panel,
	restore: CheckBox,
	sync_position: CheckBox,
	load_older: CheckBox,
	autoload: Choice,
	fetch_limit: SpinCtrl,
	streaming: CheckBox,
	refresh_minutes: SpinCtrl,
	find_load: CheckBox,
	default_timelines: Rc<RefCell<Vec<DefaultTimeline>>>,
	initial_autoload: AutoloadMode,
}

impl TimelinesTab {
	pub fn new(panel: Panel, dialog: Dialog, initial: TimelineSettings) -> Self {
		let restore = checkbox(panel, "&Restore open timelines on startup", initial.restore_open_timelines);
		let sync_position =
			checkbox(panel, "&Sync home and notifications position with your server", initial.sync_read_position);
		let load_older = checkbox(panel, "Load ol&der posts to find where you left off", initial.load_older_to_restore);
		let autoload_label = StaticText::builder(&panel).with_label("&Autoload posts:").build();
		let autoload_index = AUTOLOAD_MODES.iter().position(|m| *m == initial.autoload).unwrap_or(0);
		let autoload =
			choice(panel, &["Never", "When reaching the end", "When navigating past the end"], autoload_index);
		let fetch_limit_label = StaticText::builder(&panel).with_label("Posts to &load at a time:").build();
		let fetch_limit =
			SpinCtrl::builder(&panel).with_range(1, 40).with_initial_value(i32::from(initial.fetch_limit)).build();
		let streaming = checkbox(panel, "Stream new posts in real &time", initial.streaming);
		let refresh_label =
			StaticText::builder(&panel).with_label("&Minutes between checks for new posts when not streaming:").build();
		let refresh_minutes =
			SpinCtrl::builder(&panel).with_range(1, 60).with_initial_value(i32::from(initial.refresh_minutes)).build();
		let find_load =
			checkbox(panel, "Load more on find &next", initial.find_loading_mode == FindLoadingMode::LoadOnNext);
		let default_timelines = Rc::new(RefCell::new(initial.default_timelines));
		let customize = Button::builder(&panel).with_label("&Customize Default Timelines...").build();
		let defaults = default_timelines.clone();
		customize.on_click(move |_| {
			let current = defaults.borrow().clone();
			if let Some(updated) = prompt_for_default_timelines(&dialog, &current) {
				*defaults.borrow_mut() = updated;
			}
		});
		let sizer = TabSizer::new();
		sizer.add(&restore);
		sizer.add(&sync_position);
		sizer.add(&load_older);
		sizer.add_labeled(autoload_label, &autoload, true);
		sizer.add_labeled(fetch_limit_label, &fetch_limit, false);
		sizer.add(&streaming);
		sizer.add_labeled(refresh_label, &refresh_minutes, false);
		sizer.add(&find_load);
		sizer.add(&customize);
		sizer.finish(panel);
		Self {
			panel,
			restore,
			sync_position,
			load_older,
			autoload,
			fetch_limit,
			streaming,
			refresh_minutes,
			find_load,
			default_timelines,
			initial_autoload: initial.autoload,
		}
	}

	pub fn values(&self) -> TimelineSettings {
		TimelineSettings {
			restore_open_timelines: self.restore.get_value(),
			sync_read_position: self.sync_position.get_value(),
			load_older_to_restore: self.load_older.get_value(),
			autoload: chosen(self.autoload, &AUTOLOAD_MODES, self.initial_autoload),
			fetch_limit: u8::try_from(self.fetch_limit.value()).unwrap_or(1).clamp(1, 40),
			streaming: self.streaming.get_value(),
			refresh_minutes: u8::try_from(self.refresh_minutes.value()).unwrap_or(1).clamp(1, 60),
			find_loading_mode: if self.find_load.get_value() {
				FindLoadingMode::LoadOnNext
			} else {
				FindLoadingMode::None
			},
			default_timelines: self.default_timelines.borrow().clone(),
		}
	}
}

fn prompt_for_default_timelines(parent: &dyn WxWidget, initial: &[DefaultTimeline]) -> Option<Vec<DefaultTimeline>> {
	let dialog = Dialog::builder(parent, "Default Timelines").with_size(350, 300).build();
	let panel = Panel::builder(&dialog).build();
	let main_sizer = BoxSizer::builder(Orientation::Vertical).build();
	let info_label =
		StaticText::builder(&panel).with_label("Select additional timelines to open automatically on startup:").build();
	main_sizer.add(&info_label, 0, SizerFlag::Expand | SizerFlag::All, 10);
	let mut checkboxes = Vec::new();
	for timeline in DefaultTimeline::all() {
		let cb = CheckBox::builder(&panel).with_label(timeline.display_name()).build();
		cb.set_value(initial.contains(timeline));
		main_sizer.add(&cb, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Bottom, 10);
		checkboxes.push((cb, *timeline));
	}
	let (ok_button, cancel_button) = build_ok_cancel_buttons_on(&panel, &dialog, "OK");
	add_ok_cancel_footer(main_sizer, ok_button, cancel_button);
	panel.set_sizer(main_sizer, true);
	let dialog_sizer = BoxSizer::builder(Orientation::Vertical).build();
	dialog_sizer.add(&panel, 1, SizerFlag::Expand, 0);
	dialog.set_sizer(dialog_sizer, true);
	dialog.centre();
	let result = dialog.show_modal();
	let selected = checkboxes.into_iter().filter(|(cb, _)| cb.get_value()).map(|(_, timeline)| timeline).collect();
	dialog.destroy();
	(result == ID_OK).then_some(selected)
}
