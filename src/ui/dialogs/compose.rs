use std::{cell::RefCell, fmt::Write, path::Path, rc::Rc};

use chrono::{DateTime, Local, LocalResult, NaiveDate, NaiveTime, SecondsFormat, TimeZone, Utc};
use wxdragon::prelude::*;

use super::common::{KEY_RETURN, show_warning_widget};
use crate::{
	config::ContentWarningDisplay,
	mastodon::{Mention, PollLimits, Status},
};

const DEFAULT_MAX_POST_CHARS: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PostVisibility {
	Public,
	Unlisted,
	Private,
	Direct,
}

impl PostVisibility {
	pub const fn as_api_str(self) -> &'static str {
		match self {
			Self::Public => "public",
			Self::Unlisted => "unlisted",
			Self::Private => "private",
			Self::Direct => "direct",
		}
	}

	const fn display_name(self) -> &'static str {
		match self {
			Self::Public => "Public",
			Self::Unlisted => "Unlisted",
			Self::Private => "Followers only",
			Self::Direct => "Direct",
		}
	}

	const fn all() -> &'static [Self] {
		&[Self::Public, Self::Unlisted, Self::Private, Self::Direct]
	}
}

pub struct PostResult {
	pub content: String,
	pub visibility: PostVisibility,
	pub spoiler_text: Option<String>,
	pub content_type: Option<String>,
	pub language: Option<String>,
	pub media: Vec<PostMedia>,
	pub poll: Option<PostPoll>,
	pub scheduled_at: Option<String>,
	pub continue_thread: bool,
}

#[derive(Debug, Clone)]
pub struct PostMedia {
	pub path: String,
	pub description: Option<String>,
	pub is_existing: bool,
}

#[derive(Debug, Clone)]
pub struct PostPoll {
	pub options: Vec<String>,
	pub expires_in: u32,
	pub multiple: bool,
	pub hide_totals: bool,
}

const DEFAULT_MAX_POST_CHARS: usize = 500;
const KEY_RETURN: i32 = 13;
const ID_ACTION_FOLLOW: i32 = 6001;
const ID_ACTION_UNFOLLOW: i32 = 6002;
const ID_ACTION_BLOCK: i32 = 6003;
const ID_ACTION_UNBLOCK: i32 = 6004;
const ID_ACTION_MUTE: i32 = 6005;
const ID_ACTION_UNMUTE: i32 = 6006;
const ID_ACTION_OPEN_BROWSER: i32 = 6007;
const ID_ACTION_SHOW_BOOSTS: i32 = 6008;
const ID_ACTION_HIDE_BOOSTS: i32 = 6009;
const ID_ACTION_VIEW_FOLLOWERS: i32 = 6010;
const ID_ACTION_VIEW_FOLLOWING: i32 = 6011;
const ID_ACTION_ACCEPT_FOLLOW_REQUEST: i32 = 6012;
const ID_ACTION_REJECT_FOLLOW_REQUEST: i32 = 6013;

#[allow(clippy::struct_excessive_bools)]
struct ComposeDialogConfig {
	title_prefix: String,
	ok_label: String,
	initial_content: String,
	initial_cw: Option<String>,
	initial_language: Option<String>,
	default_visibility: PostVisibility,
	can_change_visibility: bool,
	show_schedule_controls: bool,
	show_thread_checkbox: bool,
	initial_thread_mode: bool,
	quoted_text: Option<String>,
}

const fn visibility_index(visibility: PostVisibility) -> usize {
	match visibility {
		PostVisibility::Public => 0,
		PostVisibility::Unlisted => 1,
		PostVisibility::Private => 2,
		PostVisibility::Direct => 3,
	}
}

fn refresh_media_list(media_list: ListBox, items: &[PostMedia]) {
	media_list.clear();
	for item in items {
		let label = if item.is_existing {
			item.description.as_ref().map_or_else(|| "Existing Media".to_string(), |desc| format!("Existing: {desc}"))
		} else {
			Path::new(&item.path).file_name().and_then(|name| name.to_str()).unwrap_or(&item.path).to_string()
		};
		media_list.append(&label);
	}
}

fn refresh_poll_list(poll_list: ListBox, items: &[String]) {
	poll_list.clear();
	for item in items {
		let label = if item.is_empty() { "(empty option)" } else { item.as_str() };
		poll_list.append(label);
	}
}

enum PollDialogResult {
	Updated(PostPoll),
	Removed,
}

fn prompt_for_poll(
	parent: &dyn WxWidget,
	existing: Option<&PostPoll>,
	limits: &PollLimits,
) -> Option<PollDialogResult> {
	const ID_REMOVE_POLL: i32 = 20_001;
	const DURATION_PRESETS: &[(u32, &str)] = &[
		(300, "5 minutes"),
		(1_800, "30 minutes"),
		(3_600, "1 hour"),
		(21_600, "6 hours"),
		(43_200, "12 hours"),
		(86_400, "1 day"),
		(259_200, "3 days"),
		(604_800, "7 days"),
	];

	let dialog = Dialog::builder(parent, "Manage Poll").with_size(520, 420).build();
	let panel = Panel::builder(&dialog).build();
	let main_sizer = BoxSizer::builder(Orientation::Vertical).build();
	let list_label = StaticText::builder(&panel).with_label("Options:").build();
	let poll_list = ListBox::builder(&panel).build();
	let add_button = Button::builder(&panel).with_label("Add Option").build();
	let remove_button = Button::builder(&panel).with_label("Remove Option").build();
	let option_label = StaticText::builder(&panel).with_label("Selected option text:").build();
	let option_text = TextCtrl::builder(&panel).build();
	let limits = limits.clone();
	let presets_secs: Vec<u32> = DURATION_PRESETS
		.iter()
		.filter(|(s, _)| *s >= limits.min_expiration && *s <= limits.max_expiration)
		.map(|(s, _)| *s)
		.collect();
	let presets_secs = if presets_secs.is_empty() { vec![limits.min_expiration] } else { presets_secs };
	let preset_labels: Vec<String> = presets_secs
		.iter()
		.map(|s| {
			DURATION_PRESETS
				.iter()
				.find(|(ps, _)| ps == s)
				.map_or_else(|| format!("{} minutes", s / 60), |(_, label)| (*label).to_string())
		})
		.collect();
	let duration_label = StaticText::builder(&panel).with_label("Duration:").build();
	let duration_choice = ComboBox::builder(&panel).with_choices(preset_labels).build();
	let multiple_checkbox = CheckBox::builder(&panel).with_label("Allow multiple selections").build();
	let hide_totals_checkbox = CheckBox::builder(&panel).with_label("Hide vote counts until poll closes").build();
	let remove_poll_button = Button::builder(&panel).with_label("Remove Poll").build();
	let buttons_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	let ok_button = Button::builder(&panel).with_id(ID_OK).with_label("Done").build();
	ok_button.set_default();
	let cancel_button = Button::builder(&panel).with_id(ID_CANCEL).with_label("Cancel").build();
	let list_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	let list_buttons = BoxSizer::builder(Orientation::Vertical).build();
	list_buttons.add(&add_button, 0, SizerFlag::Bottom, 8);
	list_buttons.add(&remove_button, 0, SizerFlag::Bottom, 8);
	list_sizer.add(&poll_list, 1, SizerFlag::Expand | SizerFlag::Right, 8);
	list_sizer.add_sizer(&list_buttons, 0, SizerFlag::AlignLeft, 0);
	buttons_sizer.add(&remove_poll_button, 0, SizerFlag::Right, 8);
	buttons_sizer.add_stretch_spacer(1);
	buttons_sizer.add(&ok_button, 0, SizerFlag::Right, 8);
	buttons_sizer.add(&cancel_button, 0, SizerFlag::Right, 8);
	main_sizer.add(&list_label, 0, SizerFlag::Expand | SizerFlag::All, 8);
	main_sizer.add_sizer(&list_sizer, 1, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
	main_sizer.add(&option_label, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Top, 8);
	main_sizer.add(&option_text, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
	main_sizer.add(&duration_label, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Top, 8);
	main_sizer.add(&duration_choice, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
	main_sizer.add(&multiple_checkbox, 0, SizerFlag::Expand | SizerFlag::All, 8);
	main_sizer.add(
		&hide_totals_checkbox,
		0,
		SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Bottom,
		8,
	);
	main_sizer.add_sizer(&buttons_sizer, 0, SizerFlag::Expand | SizerFlag::All, 8);
	panel.set_sizer(main_sizer, true);
	let dialog_sizer = BoxSizer::builder(Orientation::Vertical).build();
	dialog_sizer.add(&panel, 1, SizerFlag::Expand, 0);
	dialog.set_sizer(dialog_sizer, true);
	dialog.set_affirmative_id(ID_OK);
	dialog.set_escape_id(ID_CANCEL);
	let options: Rc<RefCell<Vec<String>>> =
		Rc::new(RefCell::new(existing.as_ref().map(|poll| poll.options.clone()).unwrap_or_default()));
	refresh_poll_list(poll_list, &options.borrow());
	if options.borrow().is_empty() {
		remove_button.enable(false);
		option_label.enable(false);
		option_text.enable(false);
	} else {
		poll_list.set_selection(0, true);
		remove_button.enable(true);
		option_label.enable(true);
		option_text.enable(true);
		let first_option = options.borrow().first().cloned();
		if let Some(first) = first_option {
			option_text.set_value(&first);
		}
	}
	let default_expires = existing.as_ref().map_or(86_400, |poll| poll.expires_in);
	let default_idx =
		presets_secs.iter().enumerate().min_by_key(|&(_, s)| s.abs_diff(default_expires)).map_or(0, |(i, _)| i);
	if let Ok(idx) = u32::try_from(default_idx) {
		duration_choice.set_selection(idx);
	}
	if let Some(existing) = existing.as_ref() {
		multiple_checkbox.set_value(existing.multiple);
		hide_totals_checkbox.set_value(existing.hide_totals);
	}
	remove_poll_button.enable(existing.is_some());
	let options_add = options.clone();
	let poll_list_add = poll_list;
	let add_button_add = add_button;
	let remove_button_add = remove_button;
	let option_label_add = option_label;
	let option_text_add = option_text;
	add_button.on_click(move |_| {
		let (new_len, can_add_more) = {
			let mut items = options_add.borrow_mut();
			if items.len() >= limits.max_options {
				return;
			}
			items.push(String::new());
			(items.len(), items.len() < limits.max_options)
		};
		let items_snapshot = options_add.borrow().clone();
		refresh_poll_list(poll_list_add, &items_snapshot);
		if let Ok(selection) = u32::try_from(new_len - 1) {
			poll_list_add.set_selection(selection, true);
		}
		remove_button_add.enable(true);
		option_label_add.enable(true);
		option_text_add.set_value("");
		option_text_add.enable(true);
		if !can_add_more {
			add_button_add.enable(false);
		}
	});
	if options.borrow().len() >= limits.max_options {
		add_button.enable(false);
	}
	let options_remove = options.clone();
	let poll_list_remove = poll_list_add;
	let option_text_remove = option_text;
	let remove_button_remove = remove_button;
	let add_button_remove = add_button;
	let option_label_remove = option_label;
	remove_button.on_click(move |_| {
		if let Some(selection) = poll_list_remove.get_selection() {
			let index = selection as usize;
			let items_snapshot = {
				let mut items = options_remove.borrow_mut();
				if index < items.len() {
					items.remove(index);
				}
				items.clone()
			};
			refresh_poll_list(poll_list_remove, &items_snapshot);
			if items_snapshot.is_empty() {
				remove_button_remove.enable(false);
				option_text_remove.set_value("");
				option_text_remove.enable(false);
				option_label_remove.enable(false);
			} else {
				let next = index.min(items_snapshot.len() - 1);
				if let Ok(selection) = u32::try_from(next) {
					poll_list_remove.set_selection(selection, true);
				}
				remove_button_remove.enable(true);
				option_label_remove.enable(true);
				option_text_remove.enable(true);
			}
		}
		if options_remove.borrow().len() < limits.max_options {
			add_button_remove.enable(true);
		}
	});
	let options_select = options.clone();
	let poll_list_select = poll_list_remove;
	let option_text_select = option_text_remove;
	poll_list_select.on_selection_changed(move |_| {
		let selection = poll_list_select.get_selection().map(|sel| sel as usize);
		let item_value = {
			let Ok(items) = options_select.try_borrow() else { return };
			if let Some(index) = selection
				&& index < items.len()
			{
				Some(items[index].clone())
			} else {
				None
			}
		};
		if let Some(value) = item_value {
			option_text_select.set_value(&value);
		}
	});
	let options_edit = options.clone();
	let poll_list_edit = poll_list_select;
	option_text_select.on_text_changed(move |_| {
		let selection = poll_list_edit.get_selection().map(|sel| sel as usize);
		let updated = if let Some(index) = selection {
			let value = option_text_select.get_value();
			let trimmed = value.trim().to_string();
			if trimmed.chars().count() > limits.max_option_chars {
				return;
			}
			let Ok(mut items) = options_edit.try_borrow_mut() else { return };
			if index < items.len() {
				items[index] = trimmed;
				Some((items.clone(), index))
			} else {
				None
			}
		} else {
			None
		};
		if let Some((items_snapshot, index)) = updated {
			refresh_poll_list(poll_list_edit, &items_snapshot);
			if let Ok(selection) = u32::try_from(index) {
				poll_list_edit.set_selection(selection, true);
			}
		}
	});
	remove_poll_button.on_click(move |_| {
		dialog.end_modal(ID_REMOVE_POLL);
	});
	dialog.centre();
	let result = dialog.show_modal();
	if result == ID_CANCEL {
		return None;
	}
	if result == ID_REMOVE_POLL {
		return Some(PollDialogResult::Removed);
	}
	let mut options = options.borrow().clone();
	options.retain(|option| !option.trim().is_empty());
	if options.len() < 2 {
		show_warning_widget(parent, "Polls need at least two options.", "Poll");
		return None;
	}
	if options.len() > limits.max_options {
		show_warning_widget(parent, "Too many poll options for this instance.", "Poll");
		return None;
	}
	let selected_preset = duration_choice.get_selection().and_then(|i| presets_secs.get(i as usize).copied());
	let Some(expires_in) = selected_preset else {
		show_warning_widget(parent, "Please select a poll duration.", "Poll");
		return None;
	};
	Some(PollDialogResult::Updated(PostPoll {
		options,
		expires_in,
		multiple: multiple_checkbox.get_value(),
		hide_totals: hide_totals_checkbox.get_value(),
	}))
}

fn prompt_for_media(parent: &dyn WxWidget, initial: Vec<PostMedia>) -> Option<Vec<PostMedia>> {
	let dialog = Dialog::builder(parent, "Manage Media").with_size(520, 360).build();
	let panel = Panel::builder(&dialog).build();
	let main_sizer = BoxSizer::builder(Orientation::Vertical).build();
	let list_label = StaticText::builder(&panel).with_label("Attachments:").build();
	let media_list = ListBox::builder(&panel).build();
	let add_button = Button::builder(&panel).with_label("Add...").build();
	let remove_button = Button::builder(&panel).with_label("Remove").build();
	let desc_label = StaticText::builder(&panel).with_label("Description for selected media:").build();
	let desc_text = TextCtrl::builder(&panel).build();
	let buttons_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	let ok_button = Button::builder(&panel).with_id(ID_OK).with_label("Done").build();
	ok_button.set_default();
	let cancel_button = Button::builder(&panel).with_id(ID_CANCEL).with_label("Cancel").build();
	let list_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	let list_buttons = BoxSizer::builder(Orientation::Vertical).build();
	list_buttons.add(&add_button, 0, SizerFlag::Bottom, 8);
	list_buttons.add(&remove_button, 0, SizerFlag::Bottom, 8);
	list_sizer.add(&media_list, 1, SizerFlag::Expand | SizerFlag::Right, 8);
	list_sizer.add_sizer(&list_buttons, 0, SizerFlag::AlignLeft, 0);
	buttons_sizer.add_stretch_spacer(1);
	buttons_sizer.add(&ok_button, 0, SizerFlag::Right, 8);
	buttons_sizer.add(&cancel_button, 0, SizerFlag::Right, 8);
	main_sizer.add(&list_label, 0, SizerFlag::Expand | SizerFlag::All, 8);
	main_sizer.add_sizer(&list_sizer, 1, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
	main_sizer.add(&desc_label, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Top, 8);
	main_sizer.add(&desc_text, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
	main_sizer.add_sizer(&buttons_sizer, 0, SizerFlag::Expand | SizerFlag::All, 8);
	panel.set_sizer(main_sizer, true);
	let dialog_sizer = BoxSizer::builder(Orientation::Vertical).build();
	dialog_sizer.add(&panel, 1, SizerFlag::Expand, 0);
	dialog.set_sizer(dialog_sizer, true);
	dialog.set_affirmative_id(ID_OK);
	dialog.set_escape_id(ID_CANCEL);
	let items: Rc<RefCell<Vec<PostMedia>>> = Rc::new(RefCell::new(initial));
	refresh_media_list(media_list, &items.borrow());
	if !items.borrow().is_empty() {
		media_list.set_selection(0, true);
		remove_button.enable(true);
		desc_label.enable(true);
		desc_text.enable(true);
		let first_desc = items.borrow().first().and_then(|media| media.description.clone()).unwrap_or_default();
		desc_text.set_value(&first_desc);
	}
	if items.borrow().is_empty() {
		remove_button.enable(false);
		desc_label.enable(false);
		desc_text.enable(false);
	}
	let items_add = items.clone();
	let media_list_add = media_list;
	let remove_button_add = remove_button;
	let desc_label_add = desc_label;
	let desc_text_add = desc_text;
	add_button.on_click(move |_| {
		let file_dialog = FileDialog::builder(&panel)
			.with_message("Select media to attach")
			.with_wildcard("Media files|*.png;*.jpg;*.jpeg;*.gif;*.webp;*.heic;*.heif;*.avif;*.mp4;*.m4v;*.webm;*.mov;*.mp3;*.ogg;*.wav;*.flac;*.opus;*.aac;*.m4a;*.3gp|All files|*.*")
			.with_style(FileDialogStyle::Open | FileDialogStyle::FileMustExist | FileDialogStyle::Multiple)
			.build();
		if file_dialog.show_modal() == ID_OK {
			let mut paths = file_dialog.get_paths();
			if paths.is_empty()
				&& let Some(path) = file_dialog.get_path()
			{
				paths.push(path);
			}
			if !paths.is_empty() {
				let new_len = {
					let mut items = items_add.borrow_mut();
					for path in paths {
						items.push(PostMedia { path, description: None, is_existing: false });
					}
					refresh_media_list(media_list_add, &items);
					items.len()
				};
				if new_len > 0 {
					if let Ok(selection) = u32::try_from(new_len - 1) {
						media_list_add.set_selection(selection, true);
					}
					remove_button_add.enable(true);
					desc_label_add.enable(true);
					desc_text_add.enable(true);
					desc_text_add.set_value("");
				}
			}
		}
	});

	let items_remove = items.clone();
	let media_list_remove = media_list_add;
	let remove_button_remove = remove_button_add;
	let desc_label_remove = desc_label;
	let desc_text_remove = desc_text;
	remove_button.on_click(move |_| {
		if let Some(selection) = media_list_remove.get_selection() {
			let index = selection as usize;
			let (items_len, next_index) = {
				let mut items = items_remove.borrow_mut();
				if index < items.len() {
					items.remove(index);
				}
				refresh_media_list(media_list_remove, &items);
				(items.len(), index.min(items.len().saturating_sub(1)))
			};
			if items_len > 0 {
				if let Ok(selection) = u32::try_from(next_index) {
					media_list_remove.set_selection(selection, true);
				}
				remove_button_remove.enable(true);
			} else {
				remove_button_remove.enable(false);
			}
		}
		desc_text_remove.set_value("");
		desc_label_remove.enable(false);
		desc_text_remove.enable(false);
	});

	let items_select = items.clone();
	let desc_label_select = desc_label_remove;
	let desc_text_select = desc_text_remove;
	let remove_button_select = remove_button_remove;
	let media_list_select = media_list_remove;
	media_list_select.on_selection_changed(move |_| {
		let selection = media_list_select.get_selection().map(|sel| sel as usize);
		let selected_desc = {
			let items = items_select.borrow();
			if let Some(index) = selection
				&& index < items.len()
			{
				Some(items[index].description.clone())
			} else {
				None
			}
		};
		if let Some(desc) = selected_desc {
			desc_label_select.enable(true);
			desc_text_select.enable(true);
			desc_text_select.set_value(desc.as_deref().unwrap_or(""));
			remove_button_select.enable(true);
		} else {
			desc_text_select.set_value("");
			desc_label_select.enable(false);
			desc_text_select.enable(false);
			remove_button_select.enable(false);
		}
	});

	let items_desc = items.clone();
	let media_list_desc = media_list_select;
	desc_text_select.on_text_changed(move |_| {
		let selection = media_list_desc.get_selection().map(|sel| sel as usize);
		let mut items = items_desc.borrow_mut();
		if let Some(index) = selection
			&& index < items.len()
		{
			let value = desc_text_select.get_value();
			let trimmed = value.trim();
			items[index].description = if trimmed.is_empty() { None } else { Some(trimmed.to_string()) };
		}
	});

	dialog.centre();
	let result = dialog.show_modal();
	if result != ID_OK {
		return None;
	}
	Some(items.borrow().clone())
}

pub fn prompt_for_vote(frame: &Frame, poll: &crate::mastodon::Poll, post_text: &str) -> Option<Vec<usize>> {
	let dialog = Dialog::builder(frame, "Vote").with_size(400, 500).build();
	let panel = Panel::builder(&dialog).build();
	let main_sizer = BoxSizer::builder(Orientation::Vertical).build();

	let post_display = TextCtrl::builder(&panel)
		.with_value(post_text)
		.with_style(TextCtrlStyle::MultiLine | TextCtrlStyle::ReadOnly)
		.build();
	main_sizer.add(&post_display, 1, SizerFlag::Expand | SizerFlag::All, 8);

	let info_text = if poll.expired {
		"This poll has expired."
	} else if poll.voted.unwrap_or(false) {
		"You have already voted on this poll."
	} else if poll.multiple {
		"Select options (multiple allowed):"
	} else {
		"Select an option:"
	};
	let info_label = StaticText::builder(&panel).with_label(info_text).build();
	main_sizer.add(&info_label, 0, SizerFlag::Expand | SizerFlag::All, 8);

	let options_sizer = BoxSizer::builder(Orientation::Vertical).build();
	let mut checkboxes = Vec::new();
	let mut radio_buttons = Vec::new();
	if poll.multiple {
		for option in &poll.options {
			let cb = CheckBox::builder(&panel).with_label(&option.title).build();
			if poll.expired || poll.voted.unwrap_or(false) {
				cb.enable(false);
			}
			options_sizer.add(&cb, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Bottom, 4);
			checkboxes.push(cb);
		}
	} else {
		for (i, option) in poll.options.iter().enumerate() {
			let style = if i == 0 { RadioButtonStyle::GroupStart } else { RadioButtonStyle::Default };
			let rb = RadioButton::builder(&panel).with_label(&option.title).with_style(style).build();
			if poll.expired || poll.voted.unwrap_or(false) {
				rb.enable(false);
			}
			options_sizer.add(&rb, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Bottom, 4);
			radio_buttons.push(rb);
		}
	}
	main_sizer.add_sizer(&options_sizer, 1, SizerFlag::Expand | SizerFlag::All, 8);

	if poll.expired || poll.voted.unwrap_or(false) {
		let total_votes = poll.votes_count.max(1);
		let results_sizer = BoxSizer::builder(Orientation::Vertical).build();
		for option in &poll.options {
			let votes = option.votes_count.unwrap_or(0);
			let percent = votes.saturating_mul(100).saturating_div(total_votes).min(i32::MAX as u64);
			let percent = i32::try_from(percent).unwrap_or(i32::MAX);
			let label = format!("{}: {} votes ({}%)", option.title, votes, percent);
			let text = StaticText::builder(&panel).with_label(&label).build();
			results_sizer.add(&text, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 4);
		}
		main_sizer.add_sizer(&results_sizer, 0, SizerFlag::Expand | SizerFlag::All, 8);
	}

	let button_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	let vote_button = Button::builder(&panel).with_id(ID_OK).with_label("Vote").build();
	vote_button.set_default();
	let close_button = Button::builder(&panel).with_id(ID_CANCEL).with_label("Close").build();

	if poll.expired || poll.voted.unwrap_or(false) {
		vote_button.enable(false);
	}

	button_sizer.add(&vote_button, 0, SizerFlag::Right, 8);
	button_sizer.add(&close_button, 0, SizerFlag::Right, 8);
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

	let mut selected_indices = Vec::new();
	if poll.multiple {
		for (i, cb) in checkboxes.iter().enumerate() {
			if cb.get_value() {
				selected_indices.push(i);
			}
		}
	} else {
		for (i, rb) in radio_buttons.iter().enumerate() {
			if rb.get_value() {
				selected_indices.push(i);
			}
		}
	}

	if selected_indices.is_empty() {
		return None;
	}

	Some(selected_indices)
}

fn normalize_language_code(input: &str) -> Option<String> {
	let trimmed = input.trim();
	if trimmed.is_empty() {
		return None;
	}
	Some(trimmed.to_ascii_lowercase())
}

fn schedule_button_label(scheduled_at: Option<&str>) -> String {
	if let Some(iso) = scheduled_at
		&& let Ok(dt) = DateTime::parse_from_rfc3339(iso)
	{
		let local = dt.with_timezone(&Local);
		return format!("Edit schedule, currently {}", local.format("%Y-%m-%d %H:%M"));
	}
	"Schedule...".to_string()
}

fn parse_schedule_inputs(date_value: &str, time_value: &str) -> Option<DateTime<Utc>> {
	let date = NaiveDate::parse_from_str(date_value.trim(), "%Y-%m-%d").ok()?;
	let time_trimmed = time_value.trim();
	let time = NaiveTime::parse_from_str(time_trimmed, "%H:%M")
		.or_else(|_| NaiveTime::parse_from_str(time_trimmed, "%H:%M:%S"))
		.or_else(|_| NaiveTime::parse_from_str(time_trimmed, "%I:%M %p"))
		.ok()?;
	let local_dt = date.and_time(time);
	let resolved = match Local.from_local_datetime(&local_dt) {
		LocalResult::Single(dt) => dt,
		LocalResult::Ambiguous(early, _) => early,
		LocalResult::None => return None,
	};
	Some(resolved.with_timezone(&Utc))
}

#[allow(clippy::option_option)]
fn prompt_for_schedule(parent: &dyn WxWidget, current: Option<&str>) -> Option<Option<String>> {
	const ID_CLEAR_SCHEDULE: i32 = 24_001;
	let dialog = Dialog::builder(parent, "Schedule Post").with_size(360, 210).build();
	let panel = Panel::builder(&dialog).build();
	let main_sizer = BoxSizer::builder(Orientation::Vertical).build();
	let date_label = StaticText::builder(&panel).with_label("Date (YYYY-MM-DD):").build();
	let date_input = TextCtrl::builder(&panel).with_style(TextCtrlStyle::ProcessEnter).build();
	let time_label = StaticText::builder(&panel).with_label("Time (HH:MM, 24-hour, local):").build();
	let time_input = TextCtrl::builder(&panel).with_style(TextCtrlStyle::ProcessEnter).build();
	let now_local = Local::now() + chrono::Duration::minutes(10);
	date_input.set_value(&now_local.format("%Y-%m-%d").to_string());
	time_input.set_value(&now_local.format("%H:%M").to_string());
	if let Some(current) = current
		&& let Ok(dt) = DateTime::parse_from_rfc3339(current)
	{
		let local = dt.with_timezone(&Local);
		date_input.set_value(&local.format("%Y-%m-%d").to_string());
		time_input.set_value(&local.format("%H:%M").to_string());
	}
	let button_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	let set_button = Button::builder(&panel).with_id(ID_OK).with_label("Set Schedule").build();
	let clear_button = Button::builder(&panel).with_id(ID_CLEAR_SCHEDULE).with_label("Clear").build();
	let cancel_button = Button::builder(&panel).with_id(ID_CANCEL).with_label("Cancel").build();
	set_button.set_default();
	button_sizer.add(&clear_button, 0, SizerFlag::Right, 8);
	button_sizer.add_stretch_spacer(1);
	button_sizer.add(&set_button, 0, SizerFlag::Right, 8);
	button_sizer.add(&cancel_button, 0, SizerFlag::Right, 8);
	main_sizer.add(&date_label, 0, SizerFlag::Expand | SizerFlag::All, 8);
	main_sizer.add(&date_input, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
	main_sizer.add(&time_label, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Top, 8);
	main_sizer.add(&time_input, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
	main_sizer.add_sizer(&button_sizer, 0, SizerFlag::Expand | SizerFlag::All, 8);
	panel.set_sizer(main_sizer, true);
	let dialog_sizer = BoxSizer::builder(Orientation::Vertical).build();
	dialog_sizer.add(&panel, 1, SizerFlag::Expand, 0);
	dialog.set_sizer(dialog_sizer, true);
	dialog.set_affirmative_id(ID_OK);
	dialog.set_escape_id(ID_CANCEL);
	let dialog_enter = dialog;
	time_input.on_key_down(move |event| {
		if let WindowEventData::Keyboard(ref key_event) = event {
			if key_event.get_key_code() == Some(KEY_RETURN) && !key_event.shift_down() && !key_event.control_down() {
				dialog_enter.end_modal(ID_OK);
				event.skip(false);
			} else {
				event.skip(true);
			}
		} else {
			event.skip(true);
		}
	});
	dialog.centre();
	date_input.set_focus();
	let result = dialog.show_modal();
	if result == ID_CANCEL {
		return None;
	}
	if result == ID_CLEAR_SCHEDULE {
		return Some(None);
	}
	let Some(scheduled_utc) = parse_schedule_inputs(&date_input.get_value(), &time_input.get_value()) else {
		show_warning_widget(parent, "Enter date as YYYY-MM-DD and time as HH:MM.", "Invalid Schedule");
		return None;
	};
	let minimum = Utc::now() + chrono::Duration::minutes(5);
	if scheduled_utc < minimum {
		show_warning_widget(parent, "Scheduled time must be at least 5 minutes in the future.", "Invalid Schedule");
		return None;
	}
	Some(Some(scheduled_utc.to_rfc3339_opts(SecondsFormat::Secs, true)))
}

fn prompt_for_compose(
	frame: &Frame,
	max_chars: Option<usize>,
	poll_limits: &PollLimits,
	enter_to_send: bool,
	config: ComposeDialogConfig,
	initial_media: Vec<PostMedia>,
	initial_poll: Option<PostPoll>,
) -> Option<PostResult> {
	let max_chars = max_chars.unwrap_or(DEFAULT_MAX_POST_CHARS);
	let title_prefix = config.title_prefix;
	let ok_label = config.ok_label;
	let initial_content = config.initial_content;
	let initial_cw = config.initial_cw;
	let default_visibility = config.default_visibility;
	let dialog =
		Dialog::builder(frame, &format!("{title_prefix} - 0 of {max_chars} characters")).with_size(700, 560).build();
	let panel = Panel::builder(&dialog).build();
	let main_sizer = BoxSizer::builder(Orientation::Vertical).build();

	if let Some(quoted_text) = config.quoted_text {
		let label = if title_prefix.starts_with("Quote ") {
			format!("Quoting from {}:", title_prefix.trim_start_matches("Quote "))
		} else {
			"Quoting:".to_string()
		};
		let quote_label = StaticText::builder(&panel).with_label(&label).build();
		let quote_text = TextCtrl::builder(&panel)
			.with_value(&quoted_text)
			.with_style(TextCtrlStyle::MultiLine | TextCtrlStyle::ReadOnly)
			.build();
		main_sizer.add(&quote_label, 0, SizerFlag::Expand | SizerFlag::All, 8);
		main_sizer.add(&quote_text, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
	}

	let content_label = StaticText::builder(&panel).with_label("What's on your mind?").build();
	let content_text = TextCtrl::builder(&panel).with_style(TextCtrlStyle::MultiLine).build();
	let cw_checkbox = CheckBox::builder(&panel).with_label("Content warning").build();
	let cw_label = StaticText::builder(&panel).with_label("Warning text:").build();
	let cw_text = TextCtrl::builder(&panel).build();
	cw_label.show(false);
	cw_text.show(false);
	let content_type_label = StaticText::builder(&panel).with_label("Content type (if supported):").build();
	let content_type_options = [
		("Default".to_string(), None),
		("Plain text (text/plain)".to_string(), Some("text/plain".to_string())),
		("Markdown (text/markdown)".to_string(), Some("text/markdown".to_string())),
		("HTML (text/html)".to_string(), Some("text/html".to_string())),
	];
	let content_type_labels: Vec<String> = content_type_options.iter().map(|(label, _)| label.clone()).collect();
	let content_type_choice = Choice::builder(&panel).with_choices(content_type_labels).build();
	content_type_choice.set_selection(0);
	let visibility_label = StaticText::builder(&panel).with_label("Visibility:").build();
	let visibility_choices: Vec<String> = PostVisibility::all().iter().map(|v| v.display_name().to_string()).collect();
	let visibility_choice = Choice::builder(&panel).with_choices(visibility_choices).build();
	if let Ok(selection) = u32::try_from(visibility_index(default_visibility)) {
		visibility_choice.set_selection(selection);
	}
	if !config.can_change_visibility {
		visibility_label.enable(false);
		visibility_choice.enable(false);
	}
	let visibility_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	visibility_sizer.add(&visibility_label, 0, SizerFlag::AlignCenterVertical | SizerFlag::Right, 8);
	visibility_sizer.add(&visibility_choice, 1, SizerFlag::Expand, 0);
	let content_type_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	content_type_sizer.add(&content_type_label, 0, SizerFlag::AlignCenterVertical | SizerFlag::Right, 8);
	content_type_sizer.add(&content_type_choice, 1, SizerFlag::Expand, 0);
	let language_choices = vec![
		String::new(),
		"en".to_string(),
		"es".to_string(),
		"fr".to_string(),
		"de".to_string(),
		"it".to_string(),
		"pt".to_string(),
		"ja".to_string(),
		"ko".to_string(),
		"zh".to_string(),
	];
	let language_label = StaticText::builder(&panel).with_label("Post language (ISO code):").build();
	let language_combo = ComboBox::builder(&panel).with_choices(language_choices).build();
	let initial_language_value =
		config.initial_language.as_deref().and_then(normalize_language_code).unwrap_or_default();
	language_combo.set_value(&initial_language_value);
	let language_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	language_sizer.add(&language_label, 0, SizerFlag::AlignCenterVertical | SizerFlag::Right, 8);
	language_sizer.add(&language_combo, 1, SizerFlag::Expand, 0);
	let media_button = Button::builder(&panel).with_label("Manage Media...").build();
	let poll_button = Button::builder(&panel).with_label("Add Poll...").build();
	let schedule_button = Button::builder(&panel).with_label("Schedule...").build();
	let clear_schedule_button = Button::builder(&panel).with_label("Clear Schedule").build();
	clear_schedule_button.enable(false);
	let thread_checkbox = CheckBox::builder(&panel).with_label("Thread mode").build();
	if !config.show_thread_checkbox {
		thread_checkbox.show(false);
	}
	thread_checkbox.set_value(config.initial_thread_mode);
	let button_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	let ok_button = Button::builder(&panel).with_id(ID_OK).with_label(&ok_label).build();
	if enter_to_send {
		ok_button.set_default();
	}
	let cancel_button = Button::builder(&panel).with_id(ID_CANCEL).with_label("Cancel").build();
	button_sizer.add_stretch_spacer(1);
	button_sizer.add(&ok_button, 0, SizerFlag::Right, 8);
	button_sizer.add(&cancel_button, 0, SizerFlag::Right, 8);
	main_sizer.add(&content_label, 0, SizerFlag::Expand | SizerFlag::All, 8);
	main_sizer.add(&content_text, 1, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
	main_sizer.add(&cw_checkbox, 0, SizerFlag::Expand | SizerFlag::All, 8);
	main_sizer.add(&cw_label, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
	main_sizer.add(&cw_text, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
	main_sizer.add_sizer(&visibility_sizer, 0, SizerFlag::Expand | SizerFlag::All, 8);
	main_sizer.add_sizer(&content_type_sizer, 0, SizerFlag::Expand | SizerFlag::All, 8);
	main_sizer.add_sizer(&language_sizer, 0, SizerFlag::Expand | SizerFlag::All, 8);
	main_sizer.add(&media_button, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Top, 8);
	main_sizer.add(&poll_button, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
	if config.show_schedule_controls {
		let schedule_sizer = BoxSizer::builder(Orientation::Horizontal).build();
		schedule_sizer.add(&schedule_button, 0, SizerFlag::Right, 8);
		schedule_sizer.add(&clear_schedule_button, 0, SizerFlag::Right, 8);
		schedule_sizer.add_stretch_spacer(1);
		main_sizer.add_sizer(
			&schedule_sizer,
			0,
			SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Top,
			8,
		);
	}
	if config.show_thread_checkbox {
		main_sizer.add(&thread_checkbox, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
	}
	main_sizer.add_sizer(&button_sizer, 0, SizerFlag::Expand | SizerFlag::All, 8);
	panel.set_sizer(main_sizer, true);
	let dialog_sizer = BoxSizer::builder(Orientation::Vertical).build();
	dialog_sizer.add(&panel, 1, SizerFlag::Expand, 0);
	dialog.set_sizer(dialog_sizer, true);
	dialog.set_affirmative_id(ID_OK);
	dialog.set_escape_id(ID_CANCEL);
	let media_items: Rc<RefCell<Vec<PostMedia>>> = Rc::new(RefCell::new(initial_media));
	let media_items_manage = media_items.clone();
	let media_parent = dialog;
	media_button.on_click(move |_| {
		let current = media_items_manage.borrow().clone();
		if let Some(updated) = prompt_for_media(&media_parent, current) {
			*media_items_manage.borrow_mut() = updated;
		}
	});
	let poll_state: Rc<RefCell<Option<PostPoll>>> = Rc::new(RefCell::new(initial_poll));
	{
		if poll_state.borrow().as_ref().is_some() {
			poll_button.set_label("Edit Poll...");
		}
	}
	let poll_state_manage = poll_state.clone();
	let poll_button_update = poll_button;
	let poll_parent = dialog;
	let poll_limits = poll_limits.clone();
	poll_button_update.on_click(move |_| {
		let result = {
			let current = poll_state_manage.borrow();
			prompt_for_poll(&poll_parent, current.as_ref(), &poll_limits)
		};
		match result {
			Some(PollDialogResult::Updated(poll)) => {
				*poll_state_manage.borrow_mut() = Some(poll);
				poll_button_update.set_label("Edit Poll...");
			}
			Some(PollDialogResult::Removed) => {
				*poll_state_manage.borrow_mut() = None;
				poll_button_update.set_label("Add Poll...");
			}
			None => {}
		}
	});
	let scheduled_state: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
	if config.show_schedule_controls {
		let scheduled_state_set = scheduled_state.clone();
		let schedule_button_set = schedule_button;
		let clear_schedule_button_set = clear_schedule_button;
		let schedule_parent = dialog;
		schedule_button.on_click(move |_| {
			let current = scheduled_state_set.borrow().clone();
			if let Some(updated) = prompt_for_schedule(&schedule_parent, current.as_deref()) {
				*scheduled_state_set.borrow_mut() = updated;
				let label = schedule_button_label(scheduled_state_set.borrow().as_deref());
				schedule_button_set.set_label(&label);
				clear_schedule_button_set.enable(scheduled_state_set.borrow().is_some());
			}
		});
		let scheduled_state_clear = scheduled_state.clone();
		let schedule_button_clear = schedule_button;
		let clear_schedule_button_clear = clear_schedule_button;
		clear_schedule_button.on_click(move |_| {
			*scheduled_state_clear.borrow_mut() = None;
			schedule_button_clear.set_label("Schedule...");
			clear_schedule_button_clear.enable(false);
		});
	}
	let cw_label_toggle = cw_label;
	let cw_text_toggle = cw_text;
	let panel_toggle = panel;
	let dialog_toggle = dialog;
	cw_checkbox.on_toggled(move |event| {
		let checked = event.is_checked();
		cw_label_toggle.show(checked);
		cw_text_toggle.show(checked);
		if !checked {
			cw_text_toggle.set_value("");
		}
		panel_toggle.layout();
		dialog_toggle.layout();
	});
	if !initial_content.is_empty() {
		content_text.set_value(&initial_content);
	}
	let content_text_title = content_text;
	let dialog_title = dialog;
	let title_prefix_update = title_prefix.clone();
	let update_title = move || {
		let text = content_text_title.get_value();
		let char_count = text.chars().count();
		dialog_title.set_label(&format!("{title_prefix_update} - {char_count} of {max_chars} characters"));
	};
	update_title();
	let update_title_on_change = update_title;
	content_text.on_text_changed(move |_| {
		let current = content_text.get_value();
		if current.chars().count() > max_chars {
			bell();
		}
		update_title_on_change();
	});
	if let Some(cw) = initial_cw.as_deref().map(str::trim)
		&& !cw.is_empty()
	{
		cw_checkbox.set_value(true);
		cw_label.show(true);
		cw_text.show(true);
		cw_text.set_value(cw);
	}
	if config.show_schedule_controls {
		schedule_button.set_label(&schedule_button_label(scheduled_state.borrow().as_deref()));
	}
	content_text.on_key_down(move |event| {
		if let WindowEventData::Keyboard(ref key_event) = event {
			let key = key_event.get_key_code();
			let shift = key_event.shift_down();
			let ctrl = key_event.control_down();
			let should_submit = if enter_to_send {
				key == Some(KEY_RETURN) && !shift && !ctrl
			} else {
				key == Some(KEY_RETURN) && ctrl
			};

			if should_submit {
				dialog.end_modal(ID_OK);
				event.skip(false);
			} else {
				event.skip(true);
			}
		} else {
			event.skip(true);
		}
	});
	dialog.centre();
	content_text.set_focus();
	if !initial_content.is_empty() {
		content_text.set_insertion_point_end();
	}
	let result = dialog.show_modal();
	if result != ID_OK {
		return None;
	}
	let content = content_text.get_value();
	let trimmed = content.trim();
	let char_count = trimmed.chars().count();
	if char_count > max_chars {
		show_warning_widget(
			frame,
			&format!("Post is {char_count} characters, which exceeds the {max_chars} character limit."),
			&title_prefix,
		);
		return None;
	}
	let visibility_idx = visibility_choice.get_selection().unwrap_or(0) as usize;
	let visibility = PostVisibility::all().get(visibility_idx).copied().unwrap_or(PostVisibility::Public);
	let spoiler_text = if cw_checkbox.get_value() {
		let text = cw_text.get_value();
		let trimmed = text.trim();
		if trimmed.is_empty() { None } else { Some(trimmed.to_string()) }
	} else {
		None
	};
	let content_type_idx = content_type_choice.get_selection().unwrap_or(0) as usize;
	let content_type = content_type_options.get(content_type_idx).and_then(|(_, value)| value.clone());
	let language = normalize_language_code(&language_combo.get_value());
	let media = media_items.borrow().clone();
	let poll = poll_state.borrow().clone();
	if trimmed.is_empty() && media.is_empty() && poll.is_none() {
		return None;
	}
	Some(PostResult {
		content: trimmed.to_string(),
		visibility,
		spoiler_text,
		content_type,
		language,
		media,
		poll,
		scheduled_at: scheduled_state.borrow().clone(),
		continue_thread: thread_checkbox.get_value(),
	})
}

pub fn prompt_for_post(
	frame: &Frame,
	max_chars: Option<usize>,
	poll_limits: &PollLimits,
	enter_to_send: bool,
	default_visibility: Option<PostVisibility>,
) -> Option<PostResult> {
	prompt_for_compose(
		frame,
		max_chars,
		poll_limits,
		enter_to_send,
		ComposeDialogConfig {
			title_prefix: "Post".to_string(),
			ok_label: "Post".to_string(),
			initial_content: String::new(),
			initial_cw: None,
			initial_language: None,
			default_visibility: default_visibility.unwrap_or(PostVisibility::Public),
			can_change_visibility: true,
			show_schedule_controls: true,
			show_thread_checkbox: true,
			initial_thread_mode: false,
			quoted_text: None,
		},
		Vec::new(),
		None,
	)
}

pub fn prompt_for_reply(
	frame: &Frame,
	replying_to: &Status,
	max_chars: Option<usize>,
	poll_limits: &PollLimits,
	reply_all: bool,
	self_acct: Option<&str>,
	enter_to_send: bool,
	initial_thread_mode: bool,
) -> Option<PostResult> {
	let author = replying_to.account.display_name_or_username();
	let mention = if reply_all {
		let mut accts = Vec::new();
		let self_acct = self_acct.map(|acct| acct.trim().trim_start_matches('@')).filter(|acct| !acct.is_empty());
		if let Some(self_acct) = self_acct {
			if !self_acct.eq_ignore_ascii_case(replying_to.account.acct.trim().trim_start_matches('@')) {
				accts.push(replying_to.account.acct.clone());
			}
		} else {
			accts.push(replying_to.account.acct.clone());
		}
		for m in &replying_to.mentions {
			if let Some(self_acct) = self_acct
				&& is_self_mention(self_acct, m)
			{
				continue;
			}
			if !accts.iter().any(|a| a == &m.acct) {
				accts.push(m.acct.clone());
			}
		}
		accts.iter().map(|a| format!("@{a}")).collect::<Vec<_>>().join(" ") + " "
	} else {
		format!("@{} ", replying_to.account.acct)
	};
	let default_visibility = match replying_to.visibility.as_str() {
		"unlisted" => PostVisibility::Unlisted,
		"private" => PostVisibility::Private,
		"direct" => PostVisibility::Direct,
		_ => PostVisibility::Public,
	};
	let initial_cw =
		if replying_to.spoiler_text.trim().is_empty() { None } else { Some(replying_to.spoiler_text.clone()) };
	prompt_for_compose(
		frame,
		max_chars,
		poll_limits,
		enter_to_send,
		ComposeDialogConfig {
			title_prefix: format!("Reply to {author}"),
			ok_label: "Post".to_string(),
			initial_content: mention,
			initial_cw,
			initial_language: None,
			default_visibility,
			can_change_visibility: true,
			show_schedule_controls: true,
			show_thread_checkbox: true,
			initial_thread_mode,
			quoted_text: None,
		},
		Vec::new(),
		None,
	)
}

pub fn prompt_for_edit(
	frame: &Frame,
	status: &Status,
	max_chars: Option<usize>,
	poll_limits: &PollLimits,
	enter_to_send: bool,
) -> Option<PostResult> {
	let default_visibility = match status.visibility.as_str() {
		"unlisted" => PostVisibility::Unlisted,
		"private" => PostVisibility::Private,
		"direct" => PostVisibility::Direct,
		_ => PostVisibility::Public,
	};
	let initial_cw = if status.spoiler_text.trim().is_empty() { None } else { Some(status.spoiler_text.clone()) };
	let initial_media = status
		.media_attachments
		.iter()
		.map(|m| PostMedia { path: m.id.clone(), description: m.description.clone(), is_existing: true })
		.collect();
	let initial_poll = status.poll.as_ref().map(|p| PostPoll {
		options: p.options.iter().map(|o| o.title.clone()).collect(),
		expires_in: 3600, // API doesn't return original expires_in, defaulting to 1 hour
		multiple: p.multiple,
		hide_totals: false, // API doesn't return hide_totals, defaulting to false
	});

	prompt_for_compose(
		frame,
		max_chars,
		poll_limits,
		enter_to_send,
		ComposeDialogConfig {
			title_prefix: "Edit Post".to_string(),
			ok_label: "Save".to_string(),
			initial_content: status.display_text(),
			initial_cw,
			initial_language: status.language.clone(),
			default_visibility,
			can_change_visibility: false,
			show_schedule_controls: false,
			show_thread_checkbox: false,
			initial_thread_mode: false,
			quoted_text: None,
		},
		initial_media,
		initial_poll,
	)
}

pub fn prompt_for_quote(
	frame: &Frame,
	quoting: &Status,
	max_chars: Option<usize>,
	poll_limits: &PollLimits,
	enter_to_send: bool,
) -> Option<PostResult> {
	let author = quoting.account.display_name_or_username();
	let default_visibility = match quoting.visibility.as_str() {
		"unlisted" => PostVisibility::Unlisted,
		"private" => PostVisibility::Private,
		"direct" => PostVisibility::Direct,
		_ => PostVisibility::Public,
	};
	let quoted_text = quoting.content_with_cw(ContentWarningDisplay::Inline, true);

	prompt_for_compose(
		frame,
		max_chars,
		poll_limits,
		enter_to_send,
		ComposeDialogConfig {
			title_prefix: format!("Quote {author}"),
			ok_label: "Post".to_string(),
			initial_content: String::new(),
			initial_cw: None,
			initial_language: None,
			default_visibility,
			can_change_visibility: true,
			show_schedule_controls: true,
			show_thread_checkbox: false,
			initial_thread_mode: false,
			quoted_text: Some(quoted_text),
		},
		Vec::new(),
		None,
	)
}

fn is_self_mention(self_acct: &str, mention: &crate::mastodon::Mention) -> bool {
	let mention_acct = mention.acct.trim().trim_start_matches('@');
	if self_acct.eq_ignore_ascii_case(mention_acct) {
		return true;
	}
	if self_acct.contains('@') {
		return false;
	}
	self_acct.eq_ignore_ascii_case(mention.username.trim().trim_start_matches('@'))
}
