use std::{
	cell::{Cell, RefCell},
	rc::Rc,
};

use wxdragon::prelude::*;

use super::{PADDING, choice};
use crate::config::{Config, TimelineFilter, TimelineFilters};

const TIMELINES: [&str; 9] = [
	"Home",
	"Notifications",
	"Mentions",
	"Local",
	"Federated",
	"List Timelines",
	"User Timelines",
	"Sent",
	"Hashtag Timelines",
];

type Field = fn(&mut TimelineFilter) -> &mut bool;

/// Each checkbox and the kind of post it filters out. Checked means filtered, the opposite of the setting.
const FIELDS: [(&str, Field); 10] = [
	("Original posts (not replies or boosts)", |f| &mut f.original_posts),
	("Replies to others", |f| &mut f.replies_to_others),
	("Replies to me", |f| &mut f.replies_to_me),
	("Threads (self-replies)", |f| &mut f.threads),
	("Boosts", |f| &mut f.boosts),
	("Quote posts", |f| &mut f.quote_posts),
	("Posts with media", |f| &mut f.media_posts),
	("Posts without media", |f| &mut f.text_only_posts),
	("Your posts", |f| &mut f.your_posts),
	("Your replies", |f| &mut f.your_replies),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilterSettings {
	pub filters: TimelineFilters,
}

impl FilterSettings {
	pub fn load(config: &Config) -> Self {
		Self { filters: config.filters.clone() }
	}

	pub fn save(self, config: &mut Config) {
		config.filters = self.filters;
	}
}

pub struct FiltersTab {
	pub panel: Panel,
	checkboxes: Rc<[CheckBox; 10]>,
	filters: Rc<RefCell<TimelineFilters>>,
	/// Index into `TIMELINES` of the timeline the checkboxes show.
	current: Rc<Cell<usize>>,
}

impl FiltersTab {
	pub fn new(panel: Panel, initial: FilterSettings) -> Self {
		let timeline_label = StaticText::builder(&panel).with_label("&Timeline:").build();
		let timeline = choice(panel, &TIMELINES, 0);
		let group = StaticBoxSizerBuilder::new_with_label(Orientation::Vertical, &panel, "What to filter").build();
		let checkboxes = Rc::new(FIELDS.map(|(label, _)| {
			let checkbox = CheckBox::builder(&panel).with_label(label).build();
			group.add(&checkbox, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Bottom, 5);
			checkbox
		}));
		let tab =
			Self { panel, checkboxes, filters: Rc::new(RefCell::new(initial.filters)), current: Rc::new(Cell::new(0)) };
		tab.show_current();
		let switch = tab.clone_state();
		timeline.on_selection_changed(move |_| {
			switch.store_current();
			if let Some(index) = timeline.get_selection() {
				switch.current.set(index as usize);
			}
			switch.show_current();
		});
		let row = BoxSizer::builder(Orientation::Horizontal).build();
		row.add(&timeline_label, 0, SizerFlag::AlignCenterVertical | SizerFlag::Right, PADDING);
		row.add(&timeline, 1, SizerFlag::Expand, 0);
		let sizer = BoxSizer::builder(Orientation::Vertical).build();
		sizer.add_sizer(&row, 0, SizerFlag::Expand | SizerFlag::All, PADDING);
		sizer.add_sizer(&group, 0, SizerFlag::Expand | SizerFlag::All, PADDING);
		tab.panel.set_sizer(sizer, true);
		tab
	}

	pub fn values(&self) -> FilterSettings {
		self.store_current();
		FilterSettings { filters: self.filters.borrow().clone() }
	}

	fn clone_state(&self) -> Self {
		Self {
			panel: self.panel,
			checkboxes: self.checkboxes.clone(),
			filters: self.filters.clone(),
			current: self.current.clone(),
		}
	}

	fn show_current(&self) {
		let mut filter = self.filters.borrow().resolve(TIMELINES[self.current.get()]);
		for (checkbox, (_, field)) in self.checkboxes.iter().zip(FIELDS) {
			checkbox.set_value(!*field(&mut filter));
		}
	}

	fn store_current(&self) {
		let mut filter = TimelineFilter::default();
		for (checkbox, (_, field)) in self.checkboxes.iter().zip(FIELDS) {
			*field(&mut filter) = !checkbox.get_value();
		}
		self.filters.borrow_mut().per_timeline.insert(TIMELINES[self.current.get()].to_string(), filter);
	}
}
