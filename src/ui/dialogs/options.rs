use std::{cell::RefCell, collections::HashMap, rc::Rc};

use wxdragon::prelude::*;

use crate::{
	config::{
		AutoloadMode, ContentWarningDisplay, DefaultTimeline, DisplayNameEmojiMode, HotkeyConfig,
		NotificationPreference, PerTimelineTemplates, PostTemplates, SortOrder,
	},
	template::{DEFAULT_BOOST_TEMPLATE, DEFAULT_POST_TEMPLATE},
};

pub fn prompt_for_default_timelines(frame: &Frame, initial: &[DefaultTimeline]) -> Option<Vec<DefaultTimeline>> {
	let dialog = Dialog::builder(frame, "Default Timelines").with_size(350, 300).build();
	let panel = Panel::builder(&dialog).build();
	let main_sizer = BoxSizer::builder(Orientation::Vertical).build();

	let info_label = StaticText::builder(&panel)
		.with_label("Select timelines to open automatically on startup:\n(Home and Notifications are always included)")
		.build();
	main_sizer.add(&info_label, 0, SizerFlag::Expand | SizerFlag::All, 10);

	let mut checkboxes = Vec::new();
	for timeline in DefaultTimeline::all() {
		let cb = CheckBox::builder(&panel).with_label(timeline.display_name()).build();
		cb.set_value(initial.contains(timeline));
		main_sizer.add(&cb, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Bottom, 10);
		checkboxes.push((cb, *timeline));
	}

	let button_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	let ok_button = Button::builder(&panel).with_id(ID_OK).with_label("OK").build();
	ok_button.set_default();
	let cancel_button = Button::builder(&panel).with_id(ID_CANCEL).with_label("Cancel").build();
	button_sizer.add_stretch_spacer(1);
	button_sizer.add(&ok_button, 0, SizerFlag::Right, 8);
	button_sizer.add(&cancel_button, 0, SizerFlag::Right, 8);
	main_sizer.add_sizer(&button_sizer, 0, SizerFlag::Expand | SizerFlag::All, 10);

	panel.set_sizer(main_sizer, true);
	let dialog_sizer = BoxSizer::builder(Orientation::Vertical).build();
	dialog_sizer.add(&panel, 1, SizerFlag::Expand, 0);
	dialog.set_sizer(dialog_sizer, true);
	dialog.set_affirmative_id(ID_OK);
	dialog.set_escape_id(ID_CANCEL);
	dialog.centre();

	if dialog.show_modal() == ID_OK {
		let mut selected = Vec::new();
		for (cb, timeline) in checkboxes {
			if cb.get_value() {
				selected.push(timeline);
			}
		}
		Some(selected)
	} else {
		None
	}
}

fn prompt_for_hotkey(parent: &dyn WxWidget, initial: &HotkeyConfig) -> Option<HotkeyConfig> {
	let dialog = Dialog::builder(parent, "Window Hotkey").with_size(300, 230).build();
	let panel = Panel::builder(&dialog).build();
	let main_sizer = BoxSizer::builder(Orientation::Vertical).build();

	let ctrl_cb = CheckBox::builder(&panel).with_label("&Ctrl").build();
	ctrl_cb.set_value(initial.ctrl);
	let alt_cb = CheckBox::builder(&panel).with_label("&Alt").build();
	alt_cb.set_value(initial.alt);
	let shift_cb = CheckBox::builder(&panel).with_label("&Shift").build();
	shift_cb.set_value(initial.shift);
	let win_cb = CheckBox::builder(&panel).with_label("&Win").build();
	win_cb.set_value(initial.win);

	main_sizer.add(&ctrl_cb, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Top, 10);
	main_sizer.add(&alt_cb, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 10);
	main_sizer.add(&shift_cb, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 10);
	main_sizer.add(&win_cb, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 10);

	let key_label = StaticText::builder(&panel).with_label("&Key:").build();
	let key_text = TextCtrl::builder(&panel).build();
	key_text.set_value(&hotkey_key_display_name(initial.key));
	let key_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	key_sizer.add(&key_label, 0, SizerFlag::AlignCenterVertical | SizerFlag::Right, 8);
	key_sizer.add(&key_text, 1, SizerFlag::Expand, 0);
	main_sizer.add_sizer(&key_sizer, 0, SizerFlag::Expand | SizerFlag::All, 10);

	let button_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	let ok_button = Button::builder(&panel).with_id(ID_OK).with_label("OK").build();
	ok_button.set_default();
	let cancel_button = Button::builder(&panel).with_id(ID_CANCEL).with_label("Cancel").build();
	button_sizer.add_stretch_spacer(1);
	button_sizer.add(&ok_button, 0, SizerFlag::Right, 8);
	button_sizer.add(&cancel_button, 0, SizerFlag::Right, 8);
	main_sizer.add_sizer(&button_sizer, 0, SizerFlag::Expand | SizerFlag::All, 10);

	panel.set_sizer(main_sizer, true);
	let dialog_sizer = BoxSizer::builder(Orientation::Vertical).build();
	dialog_sizer.add(&panel, 1, SizerFlag::Expand, 0);
	dialog.set_sizer(dialog_sizer, true);
	dialog.set_affirmative_id(ID_OK);
	dialog.set_escape_id(ID_CANCEL);
	dialog.centre();

	if dialog.show_modal() != ID_OK {
		return None;
	}

	let key_value = key_text.get_value();
	let key_char = parse_hotkey_key(&key_value).unwrap_or(initial.key);

	Some(HotkeyConfig {
		ctrl: ctrl_cb.get_value(),
		alt: alt_cb.get_value(),
		shift: shift_cb.get_value(),
		win: win_cb.get_value(),
		key: key_char,
	})
}

fn hotkey_key_display_name(key: char) -> String {
	match key {
		' ' => "Space".to_string(),
		c if c.is_ascii_alphanumeric() => c.to_ascii_uppercase().to_string(),
		c => c.to_string(),
	}
}

fn parse_hotkey_key(input: &str) -> Option<char> {
	let trimmed = input.trim();
	if trimmed.eq_ignore_ascii_case("space") {
		return Some(' ');
	}
	let ch = if trimmed.len() == 1 { trimmed.chars().next()? } else { return None };
	if ch.is_ascii_alphanumeric() || ch.is_ascii_punctuation() || ch == ' ' {
		Some(ch.to_ascii_uppercase())
	} else {
		None
	}
}

pub struct OptionsDialogInput {
	pub enter_to_send: bool,
	pub always_show_link_dialog: bool,
	pub show_link_previews: bool,
	pub strip_tracking: bool,
	pub quick_action_keys: bool,
	pub check_for_updates: bool,
	pub update_channel: crate::config::UpdateChannel,
	pub autoload: AutoloadMode,
	pub fetch_limit: u8,
	pub content_warning_display: ContentWarningDisplay,
	pub display_name_emoji_mode: DisplayNameEmojiMode,
	pub sort_order: SortOrder,
	pub preserve_thread_order: bool,
	pub default_timelines: Vec<DefaultTimeline>,
	pub restore_open_timelines: bool,
	pub notification_preference: NotificationPreference,
	pub hotkey: HotkeyConfig,
	pub templates: PostTemplates,
	pub filters: crate::config::TimelineFilters,
	pub find_loading_mode: crate::config::FindLoadingMode,
}

#[allow(clippy::struct_excessive_bools)]
pub struct OptionsDialogResult {
	pub enter_to_send: bool,
	pub always_show_link_dialog: bool,
	pub show_link_previews: bool,
	pub strip_tracking: bool,
	pub quick_action_keys: bool,
	pub check_for_updates: bool,
	pub update_channel: crate::config::UpdateChannel,
	pub autoload: AutoloadMode,
	pub fetch_limit: u8,
	pub content_warning_display: ContentWarningDisplay,
	pub display_name_emoji_mode: DisplayNameEmojiMode,
	pub sort_order: SortOrder,
	pub preserve_thread_order: bool,
	pub default_timelines: Vec<DefaultTimeline>,
	pub restore_open_timelines: bool,
	pub notification_preference: NotificationPreference,
	pub hotkey: HotkeyConfig,
	pub templates: PostTemplates,
	pub filters: crate::config::TimelineFilters,
	pub find_loading_mode: crate::config::FindLoadingMode,
}

type TemplateState = HashMap<String, (String, String, String)>;

pub fn prompt_for_options(frame: &Frame, input: OptionsDialogInput) -> Option<OptionsDialogResult> {
	let OptionsDialogInput {
		enter_to_send,
		always_show_link_dialog,
		show_link_previews,
		strip_tracking,
		quick_action_keys,
		check_for_updates,
		update_channel,
		autoload,
		fetch_limit,
		content_warning_display,
		display_name_emoji_mode,
		sort_order,
		preserve_thread_order,
		default_timelines: default_timelines_val,
		restore_open_timelines,
		notification_preference,
		hotkey,
		templates,
		filters,
		find_loading_mode,
	} = input;
	let dialog = Dialog::builder(frame, "Options").with_size(500, 520).build();
	let panel = Panel::builder(&dialog).build();
	let main_sizer = BoxSizer::builder(Orientation::Vertical).build();
	let notebook = Notebook::builder(&panel).build();
	let general_panel = Panel::builder(&notebook).with_style(PanelStyle::TabTraversal).build();
	let general_sizer = BoxSizer::builder(Orientation::Vertical).build();
	let enter_checkbox = CheckBox::builder(&general_panel).with_label("Use &enter to send posts").build();
	enter_checkbox.set_value(enter_to_send);
	let link_checkbox = CheckBox::builder(&general_panel).with_label("Always prompt to open &links").build();
	link_checkbox.set_value(always_show_link_dialog);
	let previews_checkbox = CheckBox::builder(&general_panel).with_label("Read &link previews in timelines").build();
	previews_checkbox.set_value(show_link_previews);
	let strip_tracking_checkbox =
		CheckBox::builder(&general_panel).with_label("Strip &tracking parameters from URLs").build();
	strip_tracking_checkbox.set_value(strip_tracking);
	let quick_action_checkbox =
		CheckBox::builder(&general_panel).with_label("Use &quick action keys in timelines").build();
	quick_action_checkbox.set_value(quick_action_keys);
	let update_checkbox = CheckBox::builder(&general_panel).with_label("Check for &updates on startup").build();
	update_checkbox.set_value(check_for_updates);

	let channel_label = StaticText::builder(&general_panel).with_label("Update Channel:").build();
	let channel_choices = vec!["Stable".to_string(), "Dev".to_string()];
	let channel_choice =
		ComboBox::builder(&general_panel).with_choices(channel_choices).with_style(ComboBoxStyle::ReadOnly).build();
	let channel_index = match update_channel {
		crate::config::UpdateChannel::Stable => 0,
		crate::config::UpdateChannel::Dev => 1,
	};
	channel_choice.set_selection(channel_index);
	let channel_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	channel_sizer.add(&channel_label, 0, SizerFlag::AlignCenterVertical | SizerFlag::Right, 8);
	channel_sizer.add(&channel_choice, 1, SizerFlag::Expand, 0);

	let notification_label = StaticText::builder(&general_panel).with_label("Notifications:").build();
	let notification_choices =
		vec!["Classic Windows Notifications".to_string(), "Sound only".to_string(), "Disabled".to_string()];
	let notification_choice = ComboBox::builder(&general_panel)
		.with_choices(notification_choices)
		.with_style(ComboBoxStyle::ReadOnly)
		.build();
	let notification_index = match notification_preference {
		NotificationPreference::Classic => 0,
		NotificationPreference::SoundOnly => 1,
		NotificationPreference::Disabled => 2,
	};
	notification_choice.set_selection(notification_index);
	let notification_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	notification_sizer.add(&notification_label, 0, SizerFlag::AlignCenterVertical | SizerFlag::Right, 8);
	notification_sizer.add(&notification_choice, 1, SizerFlag::Expand, 0);
	general_sizer.add(&enter_checkbox, 0, SizerFlag::Expand | SizerFlag::All, 8);
	general_sizer.add(&link_checkbox, 0, SizerFlag::Expand | SizerFlag::All, 8);
	general_sizer.add(&previews_checkbox, 0, SizerFlag::Expand | SizerFlag::All, 8);
	general_sizer.add(&strip_tracking_checkbox, 0, SizerFlag::Expand | SizerFlag::All, 8);
	general_sizer.add(&quick_action_checkbox, 0, SizerFlag::Expand | SizerFlag::All, 8);
	general_sizer.add(&update_checkbox, 0, SizerFlag::Expand | SizerFlag::All, 8);
	general_sizer.add_sizer(&channel_sizer, 0, SizerFlag::Expand | SizerFlag::All, 8);
	general_sizer.add_sizer(&notification_sizer, 0, SizerFlag::Expand | SizerFlag::All, 8);
	let hotkey_button = Button::builder(&general_panel).with_label("Customize Window Hotkey...").build();
	let current_hotkey = Rc::new(RefCell::new(hotkey));
	let hotkey_clone = current_hotkey.clone();
	let hotkey_frame = *frame;
	hotkey_button.on_click(move |_| {
		let initial = hotkey_clone.borrow().clone();
		if let Some(updated) = prompt_for_hotkey(&hotkey_frame, &initial) {
			*hotkey_clone.borrow_mut() = updated;
		}
	});
	general_sizer.add(&hotkey_button, 0, SizerFlag::Expand | SizerFlag::All, 8);
	general_sizer.add_stretch_spacer(1);
	general_panel.set_sizer(general_sizer, true);
	notebook.add_page(&general_panel, "General", true, None);
	let timeline_panel = Panel::builder(&notebook).with_style(PanelStyle::TabTraversal).build();
	let timeline_sizer = BoxSizer::builder(Orientation::Vertical).build();
	let restore_timelines_checkbox =
		CheckBox::builder(&timeline_panel).with_label("&Restore open timelines on startup").build();
	restore_timelines_checkbox.set_value(restore_open_timelines);
	let autoload_label = StaticText::builder(&timeline_panel).with_label("&Autoload posts:").build();
	let autoload_choices =
		vec!["Never".to_string(), "When reaching the end".to_string(), "When navigating past the end".to_string()];
	let autoload_choice =
		ComboBox::builder(&timeline_panel).with_choices(autoload_choices).with_style(ComboBoxStyle::ReadOnly).build();
	let autoload_index = match autoload {
		AutoloadMode::Never => 0,
		AutoloadMode::AtEnd => 1,
		AutoloadMode::AtBoundary => 2,
	};
	autoload_choice.set_selection(autoload_index);
	let autoload_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	autoload_sizer.add(&autoload_label, 0, SizerFlag::AlignCenterVertical | SizerFlag::Right, 8);
	autoload_sizer.add(&autoload_choice, 1, SizerFlag::Expand, 0);
	let fetch_limit_label =
		StaticText::builder(&timeline_panel).with_label("Posts to &fetch when loading more:").build();
	let fetch_limit_spin =
		SpinCtrl::builder(&timeline_panel).with_range(1, 40).with_initial_value(i32::from(fetch_limit)).build();
	let fetch_limit_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	fetch_limit_sizer.add(&fetch_limit_label, 0, SizerFlag::AlignCenterVertical | SizerFlag::Right, 8);
	fetch_limit_sizer.add(&fetch_limit_spin, 0, SizerFlag::empty(), 0);
	let cw_label = StaticText::builder(&timeline_panel).with_label("Content warning display:").build();
	let cw_choices = vec!["Show inline".to_string(), "Don't show".to_string(), "CW only".to_string()];
	let cw_choice =
		ComboBox::builder(&timeline_panel).with_choices(cw_choices).with_style(ComboBoxStyle::ReadOnly).build();
	let cw_index = match content_warning_display {
		ContentWarningDisplay::Inline => 0,
		ContentWarningDisplay::Hidden => 1,
		ContentWarningDisplay::WarningOnly => 2,
	};
	cw_choice.set_selection(cw_index);
	let cw_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	cw_sizer.add(&cw_label, 0, SizerFlag::AlignCenterVertical | SizerFlag::Right, 8);
	cw_sizer.add(&cw_choice, 1, SizerFlag::Expand, 0);
	let emoji_mode_label = StaticText::builder(&timeline_panel).with_label("Display name &emoji filtering:").build();
	let emoji_mode_choices =
		vec!["None".to_string(), "Unicode emojis".to_string(), "Instance emojis".to_string(), "All".to_string()];
	let emoji_mode_choice =
		ComboBox::builder(&timeline_panel).with_choices(emoji_mode_choices).with_style(ComboBoxStyle::ReadOnly).build();
	let emoji_mode_index = match display_name_emoji_mode {
		DisplayNameEmojiMode::None => 0,
		DisplayNameEmojiMode::UnicodeOnly => 1,
		DisplayNameEmojiMode::InstanceOnly => 2,
		DisplayNameEmojiMode::All => 3,
	};
	emoji_mode_choice.set_selection(emoji_mode_index);
	let emoji_mode_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	emoji_mode_sizer.add(&emoji_mode_label, 0, SizerFlag::AlignCenterVertical | SizerFlag::Right, 8);
	emoji_mode_sizer.add(&emoji_mode_choice, 1, SizerFlag::Expand, 0);
	let sort_checkbox = CheckBox::builder(&timeline_panel).with_label("Show oldest timeline entries &first").build();
	sort_checkbox.set_value(sort_order == SortOrder::OldestToNewest);
	let thread_order_checkbox = CheckBox::builder(&timeline_panel).with_label("Always preserve thread &order").build();
	thread_order_checkbox.set_value(preserve_thread_order);

	let find_load_checkbox = CheckBox::builder(&timeline_panel).with_label("Load more on find &next").build();
	find_load_checkbox.set_value(find_loading_mode == crate::config::FindLoadingMode::LoadOnNext);

	timeline_sizer.add_sizer(&autoload_sizer, 0, SizerFlag::Expand | SizerFlag::All, 8);
	timeline_sizer.add_sizer(&fetch_limit_sizer, 0, SizerFlag::Expand | SizerFlag::All, 8);
	timeline_sizer.add_sizer(&cw_sizer, 0, SizerFlag::Expand | SizerFlag::All, 8);
	timeline_sizer.add_sizer(&emoji_mode_sizer, 0, SizerFlag::Expand | SizerFlag::All, 8);
	timeline_sizer.add(&sort_checkbox, 0, SizerFlag::Expand | SizerFlag::All, 8);
	timeline_sizer.add(&thread_order_checkbox, 0, SizerFlag::Expand | SizerFlag::All, 8);
	timeline_sizer.add(&find_load_checkbox, 0, SizerFlag::Expand | SizerFlag::All, 8);
	timeline_sizer.add(&restore_timelines_checkbox, 0, SizerFlag::Expand | SizerFlag::All, 8);
	let customize_button = Button::builder(&timeline_panel).with_label("Customize Default Timelines...").build();
	let current_defaults = Rc::new(RefCell::new(default_timelines_val));
	let defaults_clone = current_defaults.clone();
	let parent_frame = *frame;
	customize_button.on_click(move |_| {
		let initial = defaults_clone.borrow().clone();
		if let Some(updated) = prompt_for_default_timelines(&parent_frame, &initial) {
			*defaults_clone.borrow_mut() = updated;
		}
	});
	timeline_sizer.add(&customize_button, 0, SizerFlag::Expand | SizerFlag::All, 8);
	timeline_sizer.add_stretch_spacer(1);
	timeline_panel.set_sizer(timeline_sizer, true);
	notebook.add_page(&timeline_panel, "Timeline", false, None);
	let template_panel = Panel::builder(&notebook).with_style(PanelStyle::TabTraversal).build();
	let template_sizer = BoxSizer::builder(Orientation::Vertical).build();
	let timeline_keys: Vec<&str> = vec![
		"Home",
		"Notifications",
		"Direct Messages",
		"Local",
		"Federated",
		"Bookmarks",
		"Favorites",
		"User Timelines",
		"Threads",
		"Search Results",
		"Hashtag Timelines",
	];
	let timeline_key_strings: Vec<String> = timeline_keys.iter().map(|s| (*s).to_string()).collect();
	let template_timeline_label = StaticText::builder(&template_panel).with_label("&Timeline:").build();
	let template_timeline_choice = ComboBox::builder(&template_panel)
		.with_choices(timeline_key_strings)
		.with_style(ComboBoxStyle::ReadOnly)
		.build();
	template_timeline_choice.set_selection(0);
	let template_timeline_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	template_timeline_sizer.add(&template_timeline_label, 0, SizerFlag::AlignCenterVertical | SizerFlag::Right, 8);
	template_timeline_sizer.add(&template_timeline_choice, 1, SizerFlag::Expand, 0);
	let post_template_label = StaticText::builder(&template_panel).with_label("&Post template:").build();
	let post_template_text = TextCtrl::builder(&template_panel)
		.with_style(TextCtrlStyle::MultiLine)
		.with_value(
			templates.per_timeline.get("Home").and_then(|pt| pt.post.as_deref()).unwrap_or(DEFAULT_POST_TEMPLATE),
		)
		.build();
	let boost_template_label = StaticText::builder(&template_panel).with_label("&Boost template:").build();
	let boost_template_text = TextCtrl::builder(&template_panel)
		.with_style(TextCtrlStyle::MultiLine)
		.with_value(
			templates.per_timeline.get("Home").and_then(|pt| pt.boost.as_deref()).unwrap_or(DEFAULT_BOOST_TEMPLATE),
		)
		.build();
	let quote_template_label = StaticText::builder(&template_panel).with_label("&Quote template:").build();
	let quote_template_text = TextCtrl::builder(&template_panel)
		.with_style(TextCtrlStyle::MultiLine)
		.with_value(
			templates
				.per_timeline
				.get("Home")
				.and_then(|pt| pt.quote.as_deref())
				.unwrap_or(crate::template::DEFAULT_QUOTE_TEMPLATE),
		)
		.build();
	let template_button_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	let reset_button = Button::builder(&template_panel).with_label("Reset to default").build();
	template_button_sizer.add(&reset_button, 0, SizerFlag::empty(), 0);
	template_sizer.add_sizer(&template_timeline_sizer, 0, SizerFlag::Expand | SizerFlag::All, 8);
	template_sizer.add(
		&post_template_label,
		0,
		SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Top,
		8,
	);
	template_sizer.add(&post_template_text, 1, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
	template_sizer.add(
		&boost_template_label,
		0,
		SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Top,
		8,
	);
	template_sizer.add(&boost_template_text, 1, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
	template_sizer.add(
		&quote_template_label,
		0,
		SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Top,
		8,
	);
	template_sizer.add(&quote_template_text, 1, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
	template_sizer.add_sizer(&template_button_sizer, 0, SizerFlag::Expand | SizerFlag::All, 8);
	template_panel.set_sizer(template_sizer, true);
	notebook.add_page(&template_panel, "Templates", false, None);

	let filters_panel = Panel::builder(&notebook).with_style(PanelStyle::TabTraversal).build();
	let filters_sizer = BoxSizer::builder(Orientation::Vertical).build();

	let filter_timeline_keys: Vec<&str> =
		vec!["Home", "Notifications", "Local", "Federated", "List Timelines", "User Timelines", "Hashtag Timelines"];
	let filter_timeline_key_strings: Vec<String> = filter_timeline_keys.iter().map(|s| (*s).to_string()).collect();
	let filter_timeline_label = StaticText::builder(&filters_panel).with_label("&Timeline:").build();
	let filter_timeline_choice = ComboBox::builder(&filters_panel)
		.with_choices(filter_timeline_key_strings)
		.with_style(ComboBoxStyle::ReadOnly)
		.build();
	filter_timeline_choice.set_selection(0);
	let filter_timeline_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	filter_timeline_sizer.add(&filter_timeline_label, 0, SizerFlag::AlignCenterVertical | SizerFlag::Right, 8);
	filter_timeline_sizer.add(&filter_timeline_choice, 1, SizerFlag::Expand, 0);
	filters_sizer.add_sizer(&filter_timeline_sizer, 0, SizerFlag::Expand | SizerFlag::All, 8);

	let what_to_filter_sizer =
		StaticBoxSizerBuilder::new_with_label(Orientation::Vertical, &filters_panel, "What to filter").build();

	let cb_original = CheckBox::builder(&filters_panel).with_label("Original posts (not replies or boosts)").build();
	what_to_filter_sizer.add(
		&cb_original,
		0,
		SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Bottom,
		5,
	);

	let cb_replies_others = CheckBox::builder(&filters_panel).with_label("Replies to others").build();
	what_to_filter_sizer.add(
		&cb_replies_others,
		0,
		SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Bottom,
		5,
	);

	let cb_replies_me = CheckBox::builder(&filters_panel).with_label("Replies to me").build();
	what_to_filter_sizer.add(
		&cb_replies_me,
		0,
		SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Bottom,
		5,
	);

	let cb_threads = CheckBox::builder(&filters_panel).with_label("Threads (self-replies)").build();
	what_to_filter_sizer.add(
		&cb_threads,
		0,
		SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Bottom,
		5,
	);

	let cb_boosts = CheckBox::builder(&filters_panel).with_label("Boosts").build();
	what_to_filter_sizer.add(
		&cb_boosts,
		0,
		SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Bottom,
		5,
	);

	let cb_quotes = CheckBox::builder(&filters_panel).with_label("Quote posts").build();
	what_to_filter_sizer.add(
		&cb_quotes,
		0,
		SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Bottom,
		5,
	);

	let cb_media = CheckBox::builder(&filters_panel).with_label("Posts with media").build();
	what_to_filter_sizer.add(
		&cb_media,
		0,
		SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Bottom,
		5,
	);

	let cb_no_media = CheckBox::builder(&filters_panel).with_label("Posts without media").build();
	what_to_filter_sizer.add(
		&cb_no_media,
		0,
		SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Bottom,
		5,
	);

	let cb_your_posts = CheckBox::builder(&filters_panel).with_label("Your posts").build();
	what_to_filter_sizer.add(
		&cb_your_posts,
		0,
		SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Bottom,
		5,
	);

	let cb_your_replies = CheckBox::builder(&filters_panel).with_label("Your replies").build();
	what_to_filter_sizer.add(
		&cb_your_replies,
		0,
		SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Bottom,
		5,
	);

	filters_sizer.add_sizer(&what_to_filter_sizer, 0, SizerFlag::Expand | SizerFlag::All, 10);

	filters_panel.set_sizer(filters_sizer, true);
	notebook.add_page(&filters_panel, "Filters", false, None);

	let filters_state = Rc::new(RefCell::new(filters));
	let current_filter_key = Rc::new(RefCell::new("Home".to_string()));
	{
		let initial_filter = filters_state.borrow().resolve("Home");
		cb_original.set_value(!initial_filter.original_posts);
		cb_replies_others.set_value(!initial_filter.replies_to_others);
		cb_replies_me.set_value(!initial_filter.replies_to_me);
		cb_threads.set_value(!initial_filter.threads);
		cb_boosts.set_value(!initial_filter.boosts);
		cb_quotes.set_value(!initial_filter.quote_posts);
		cb_media.set_value(!initial_filter.media_posts);
		cb_no_media.set_value(!initial_filter.text_only_posts);
		cb_your_posts.set_value(!initial_filter.your_posts);
		cb_your_replies.set_value(!initial_filter.your_replies);
	}
	let update_filters_state = {
		let filters_state = filters_state.clone();
		let current_filter_key = current_filter_key.clone();
		Rc::new(move || {
			let key = current_filter_key.borrow().clone();
			filters_state.borrow_mut().per_timeline.insert(
				key,
				crate::config::TimelineFilter {
					original_posts: !cb_original.get_value(),
					replies_to_others: !cb_replies_others.get_value(),
					replies_to_me: !cb_replies_me.get_value(),
					threads: !cb_threads.get_value(),
					boosts: !cb_boosts.get_value(),
					quote_posts: !cb_quotes.get_value(),
					media_posts: !cb_media.get_value(),
					text_only_posts: !cb_no_media.get_value(),
					your_posts: !cb_your_posts.get_value(),
					your_replies: !cb_your_replies.get_value(),
				},
			);
		})
	};
	let filter_timeline_keys_clone = filter_timeline_keys.clone();
	let fs_change_cb = filters_state.clone();
	let cur_key_change = current_filter_key;
	let ufs_change = update_filters_state.clone();
	filter_timeline_choice.on_selection_changed(move |_| {
		ufs_change();
		let Some(new_index) = filter_timeline_choice.get_selection() else { return };
		let new_index = new_index as usize;
		let Some(new_key) = filter_timeline_keys_clone.get(new_index) else { return };
		*cur_key_change.borrow_mut() = (*new_key).to_string();
		let new_filter = fs_change_cb.borrow().resolve(new_key);
		cb_original.set_value(!new_filter.original_posts);
		cb_replies_others.set_value(!new_filter.replies_to_others);
		cb_replies_me.set_value(!new_filter.replies_to_me);
		cb_threads.set_value(!new_filter.threads);
		cb_boosts.set_value(!new_filter.boosts);
		cb_quotes.set_value(!new_filter.quote_posts);
		cb_media.set_value(!new_filter.media_posts);
		cb_no_media.set_value(!new_filter.text_only_posts);
		cb_your_posts.set_value(!new_filter.your_posts);
		cb_your_replies.set_value(!new_filter.your_replies);
	});

	let setup_cb_handler = |cb: &CheckBox, ufs: Rc<dyn Fn()>| {
		cb.on_toggled(move |_| {
			ufs();
		});
	};
	setup_cb_handler(&cb_original, update_filters_state.clone());
	setup_cb_handler(&cb_replies_others, update_filters_state.clone());
	setup_cb_handler(&cb_replies_me, update_filters_state.clone());
	setup_cb_handler(&cb_threads, update_filters_state.clone());
	setup_cb_handler(&cb_boosts, update_filters_state.clone());
	setup_cb_handler(&cb_quotes, update_filters_state.clone());
	setup_cb_handler(&cb_media, update_filters_state.clone());
	setup_cb_handler(&cb_no_media, update_filters_state.clone());
	setup_cb_handler(&cb_your_posts, update_filters_state.clone());
	setup_cb_handler(&cb_your_replies, update_filters_state);
	// State for template editing: maps timeline key -> (post_template, boost_template, quote_template)
	let template_state: Rc<RefCell<TemplateState>> = Rc::new(RefCell::new(HashMap::new()));
	{
		let mut state = template_state.borrow_mut();
		for key in &timeline_keys {
			let pt = templates.per_timeline.get(*key);
			let post = pt.and_then(|p| p.post.as_deref()).unwrap_or(DEFAULT_POST_TEMPLATE).to_string();
			let boost = pt.and_then(|p| p.boost.as_deref()).unwrap_or(DEFAULT_BOOST_TEMPLATE).to_string();
			let quote =
				pt.and_then(|p| p.quote.as_deref()).unwrap_or(crate::template::DEFAULT_QUOTE_TEMPLATE).to_string();
			state.insert((*key).to_string(), (post, boost, quote));
		}
	}
	let ts_change = template_state.clone();
	let post_text_change = post_template_text;
	let boost_text_change = boost_template_text;
	let quote_text_change = quote_template_text;
	let prev_selection: Rc<RefCell<String>> = Rc::new(RefCell::new("Home".to_string()));
	let prev_sel_change = prev_selection.clone();
	let timeline_keys_clone = timeline_keys.clone();
	template_timeline_choice.on_selection_changed(move |_| {
		let Some(new_index) = template_timeline_choice.get_selection() else { return };
		let new_index = new_index as usize;
		let Some(new_key) = timeline_keys_clone.get(new_index) else { return };
		{
			let mut state = ts_change.borrow_mut();
			let prev = prev_sel_change.borrow().clone();
			state.insert(
				prev,
				(post_text_change.get_value(), boost_text_change.get_value(), quote_text_change.get_value()),
			);
		}
		let state = ts_change.borrow();
		if let Some((post, boost, quote)) = state.get(*new_key) {
			post_text_change.set_value(post);
			boost_text_change.set_value(boost);
			quote_text_change.set_value(quote);
		}
		*prev_sel_change.borrow_mut() = (*new_key).to_string();
	});
	let ts_reset = template_state.clone();
	let post_text_reset = post_template_text;
	let boost_text_reset = boost_template_text;
	let quote_text_reset = quote_template_text;
	let prev_sel_reset = prev_selection.clone();
	reset_button.on_click(move |_| {
		let current_key = prev_sel_reset.borrow().clone();
		post_text_reset.set_value(DEFAULT_POST_TEMPLATE);
		boost_text_reset.set_value(DEFAULT_BOOST_TEMPLATE);
		quote_text_reset.set_value(crate::template::DEFAULT_QUOTE_TEMPLATE);
		ts_reset.borrow_mut().insert(
			current_key,
			(
				DEFAULT_POST_TEMPLATE.to_string(),
				DEFAULT_BOOST_TEMPLATE.to_string(),
				crate::template::DEFAULT_QUOTE_TEMPLATE.to_string(),
			),
		);
	});
	let button_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	let ok_button = Button::builder(&panel).with_id(ID_OK).with_label("OK").build();
	ok_button.set_default();
	let cancel_button = Button::builder(&panel).with_id(ID_CANCEL).with_label("Cancel").build();
	button_sizer.add_stretch_spacer(1);
	button_sizer.add(&ok_button, 0, SizerFlag::Right, 8);
	button_sizer.add(&cancel_button, 0, SizerFlag::Right, 8);
	main_sizer.add(&notebook, 1, SizerFlag::Expand | SizerFlag::All, 8);
	main_sizer.add_sizer(&button_sizer, 0, SizerFlag::Expand | SizerFlag::All, 8);
	panel.set_sizer(main_sizer, true);
	let dialog_sizer = BoxSizer::builder(Orientation::Vertical).build();
	dialog_sizer.add(&panel, 1, SizerFlag::Expand, 0);
	dialog.set_sizer(dialog_sizer, true);
	dialog.set_affirmative_id(ID_OK);
	dialog.set_escape_id(ID_CANCEL);
	dialog.centre();
	let result = dialog.show_modal();
	if result != ID_OK {
		return None;
	}
	let new_sort = if sort_checkbox.get_value() { SortOrder::OldestToNewest } else { SortOrder::NewestToOldest };
	let new_cw_display = match cw_choice.get_selection() {
		Some(0) => ContentWarningDisplay::Inline,
		Some(1) => ContentWarningDisplay::Hidden,
		Some(2) => ContentWarningDisplay::WarningOnly,
		_ => content_warning_display,
	};
	let new_display_name_emoji_mode = match emoji_mode_choice.get_selection() {
		Some(0) => DisplayNameEmojiMode::None,
		Some(1) => DisplayNameEmojiMode::UnicodeOnly,
		Some(2) => DisplayNameEmojiMode::InstanceOnly,
		Some(3) => DisplayNameEmojiMode::All,
		_ => display_name_emoji_mode,
	};
	let new_autoload = match autoload_choice.get_selection() {
		Some(0) => AutoloadMode::Never,
		Some(1) => AutoloadMode::AtEnd,
		Some(2) => AutoloadMode::AtBoundary,
		_ => autoload,
	};
	let new_fetch_limit = u8::try_from(fetch_limit_spin.value()).unwrap_or(1).clamp(1, 40);
	let new_notification_preference = match notification_choice.get_selection() {
		Some(0) => crate::config::NotificationPreference::Classic,
		Some(1) => crate::config::NotificationPreference::SoundOnly,
		Some(2) => crate::config::NotificationPreference::Disabled,
		_ => notification_preference,
	};
	let new_update_channel = match channel_choice.get_selection() {
		Some(0) => crate::config::UpdateChannel::Stable,
		Some(1) => crate::config::UpdateChannel::Dev,
		_ => update_channel,
	};
	let new_find_loading_mode = if find_load_checkbox.get_value() {
		crate::config::FindLoadingMode::LoadOnNext
	} else {
		crate::config::FindLoadingMode::None
	};
	{
		let mut ts = template_state.borrow_mut();
		let current_key = prev_selection.borrow().clone();
		ts.insert(
			current_key,
			(post_template_text.get_value(), boost_template_text.get_value(), quote_template_text.get_value()),
		);
	}
	let new_templates = {
		let ts = template_state.borrow();
		let mut per_timeline = HashMap::new();
		for key in &timeline_keys {
			if let Some((post, boost, quote)) = ts.get(*key) {
				let post_override = if post == DEFAULT_POST_TEMPLATE { None } else { Some(post.clone()) };
				let boost_override = if boost == DEFAULT_BOOST_TEMPLATE { None } else { Some(boost.clone()) };
				let quote_override =
					if quote == crate::template::DEFAULT_QUOTE_TEMPLATE { None } else { Some(quote.clone()) };
				if post_override.is_some() || boost_override.is_some() || quote_override.is_some() {
					per_timeline.insert(
						(*key).to_string(),
						PerTimelineTemplates { post: post_override, boost: boost_override, quote: quote_override },
					);
				}
			}
		}
		PostTemplates { per_timeline }
	};
	Some(OptionsDialogResult {
		enter_to_send: enter_checkbox.get_value(),
		always_show_link_dialog: link_checkbox.get_value(),
		show_link_previews: previews_checkbox.get_value(),
		strip_tracking: strip_tracking_checkbox.get_value(),
		quick_action_keys: quick_action_checkbox.get_value(),
		check_for_updates: update_checkbox.get_value(),
		restore_open_timelines: restore_timelines_checkbox.get_value(),
		update_channel: new_update_channel,
		autoload: new_autoload,
		fetch_limit: new_fetch_limit,
		content_warning_display: new_cw_display,
		display_name_emoji_mode: new_display_name_emoji_mode,
		sort_order: new_sort,
		preserve_thread_order: thread_order_checkbox.get_value(),
		default_timelines: current_defaults.borrow().clone(),
		notification_preference: new_notification_preference,
		hotkey: current_hotkey.borrow().clone(),
		templates: new_templates,
		filters: filters_state.borrow().clone(),
		find_loading_mode: new_find_loading_mode,
	})
}
