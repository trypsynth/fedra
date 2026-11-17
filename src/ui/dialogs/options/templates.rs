use std::{
	cell::{Cell, RefCell},
	collections::HashMap,
	rc::Rc,
};

use wxdragon::prelude::*;

use super::{PADDING, choice};
use crate::{
	config::{Config, PerTimelineTemplates, PostTemplates},
	template::{DEFAULT_BOOST_TEMPLATE, DEFAULT_FAVORITE_TEMPLATE, DEFAULT_POST_TEMPLATE, DEFAULT_QUOTE_TEMPLATE},
};

const TIMELINES: [&str; 13] = [
	"Home",
	"Notifications",
	"Mentions",
	"Direct Messages",
	"Local",
	"Federated",
	"Bookmarks",
	"Favorites",
	"User Timelines",
	"Sent",
	"Threads",
	"Search Results",
	"Hashtag Timelines",
];
const LABELS: [&str; 4] = ["&Post template:", "&Boost template:", "&Quote template:", "&Favorite template:"];
const DEFAULTS: [&str; 4] =
	[DEFAULT_POST_TEMPLATE, DEFAULT_BOOST_TEMPLATE, DEFAULT_QUOTE_TEMPLATE, DEFAULT_FAVORITE_TEMPLATE];
/// The favorite template only shapes favorite notifications, which only the Notifications timeline has.
const FAVORITE: usize = 3;
const NOTIFICATIONS: usize = 1;

type Texts = [String; 4];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateSettings {
	pub window_title_template: String,
	pub templates: PostTemplates,
}

impl TemplateSettings {
	pub fn load(config: &Config) -> Self {
		Self { window_title_template: config.window_title_template.clone(), templates: config.templates.clone() }
	}

	pub fn save(self, config: &mut Config) {
		config.window_title_template = self.window_title_template;
		config.templates = self.templates;
	}
}

pub struct TemplatesTab {
	pub panel: Panel,
	window_title: TextCtrl,
	texts: [TextCtrl; 4],
	/// Each timeline's templates as edited so far, defaults included.
	edited: Rc<RefCell<HashMap<&'static str, Texts>>>,
	/// Index into `TIMELINES` of the timeline the fields show.
	current: Rc<Cell<usize>>,
}

impl TemplatesTab {
	pub fn new(panel: Panel, dialog: Dialog, initial: &TemplateSettings) -> Self {
		let window_title_label = StaticText::builder(&panel).with_label("&Window title template:").build();
		let window_title = TextCtrl::builder(&panel).with_value(&initial.window_title_template).build();
		let timeline_label = StaticText::builder(&panel).with_label("&Timeline:").build();
		let timeline = choice(panel, &TIMELINES, 0);
		let edited: HashMap<_, _> = TIMELINES.iter().map(|key| (*key, texts_for(&initial.templates, key))).collect();
		let labels = LABELS.map(|label| StaticText::builder(&panel).with_label(label).build());
		let texts = std::array::from_fn(|i| {
			TextCtrl::builder(&panel).with_style(TextCtrlStyle::MultiLine).with_value(&edited[TIMELINES[0]][i]).build()
		});
		let reset = Button::builder(&panel).with_label("&Reset to default").build();
		let sizer = BoxSizer::builder(Orientation::Vertical).build();
		let label_flags = SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Top;
		let text_flags = SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right;
		sizer.add(&window_title_label, 0, label_flags, PADDING);
		sizer.add(&window_title, 0, text_flags, PADDING);
		let row = BoxSizer::builder(Orientation::Horizontal).build();
		row.add(&timeline_label, 0, SizerFlag::AlignCenterVertical | SizerFlag::Right, PADDING);
		row.add(&timeline, 1, SizerFlag::Expand, 0);
		sizer.add_sizer(&row, 0, SizerFlag::Expand | SizerFlag::All, PADDING);
		for (label, text) in labels.iter().zip(&texts) {
			sizer.add(label, 0, label_flags, PADDING);
			sizer.add(text, 1, text_flags, PADDING);
		}
		sizer.add(&reset, 0, SizerFlag::All, PADDING);
		panel.set_sizer(sizer, true);
		labels[FAVORITE].show(false);
		texts[FAVORITE].show(false);
		let tab =
			Self { panel, window_title, texts, edited: Rc::new(RefCell::new(edited)), current: Rc::new(Cell::new(0)) };
		let switch = tab.clone_state();
		timeline.on_selection_changed(move |_| {
			switch.store_current();
			let Some(index) = timeline.get_selection() else { return };
			let index = index as usize;
			switch.current.set(index);
			for (text, value) in switch.texts.iter().zip(&switch.edited.borrow()[TIMELINES[index]]) {
				text.set_value(value);
			}
			let show_favorite = index == NOTIFICATIONS;
			labels[FAVORITE].show(show_favorite);
			switch.texts[FAVORITE].show(show_favorite);
			switch.panel.layout();
			dialog.layout();
		});
		let texts = tab.texts;
		reset.on_click(move |_| {
			for (text, default) in texts.iter().zip(DEFAULTS) {
				text.set_value(default);
			}
		});
		tab
	}

	pub fn values(&self) -> TemplateSettings {
		self.store_current();
		let edited = self.edited.borrow();
		let mut templates = PostTemplates::default();
		for key in TIMELINES {
			let [post, boost, quote, favorite] =
				std::array::from_fn(|i| Some(edited[key][i].clone()).filter(|text| text != DEFAULTS[i]));
			if post.is_some() || boost.is_some() || quote.is_some() || favorite.is_some() {
				templates.per_timeline.insert(key.to_string(), PerTimelineTemplates { post, boost, quote, favorite });
			}
		}
		TemplateSettings { window_title_template: self.window_title.get_value(), templates }
	}

	fn clone_state(&self) -> Self {
		Self {
			panel: self.panel,
			window_title: self.window_title,
			texts: self.texts,
			edited: self.edited.clone(),
			current: self.current.clone(),
		}
	}

	fn store_current(&self) {
		let values = self.texts.each_ref().map(TextCtrl::get_value);
		self.edited.borrow_mut().insert(TIMELINES[self.current.get()], values);
	}
}

/// The templates `key` uses, its own where it has them and the defaults elsewhere.
fn texts_for(templates: &PostTemplates, key: &str) -> Texts {
	let own = templates
		.per_timeline
		.get(key)
		.map(|pt| [pt.post.as_deref(), pt.boost.as_deref(), pt.quote.as_deref(), pt.favorite.as_deref()])
		.unwrap_or_default();
	std::array::from_fn(|i| own[i].unwrap_or(DEFAULTS[i]).to_string())
}
