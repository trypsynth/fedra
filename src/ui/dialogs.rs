use std::{cell::RefCell, collections::HashMap, fmt::Write, path::Path, rc::Rc, sync::mpsc::Sender};

use url::Url;
use wxdragon::{
	event::{WebViewEventData, WebViewEvents},
	prelude::*,
	widgets::WebView,
};

use crate::{
	commands::UiCommand,
	config::{
		Account, AutoloadMode, ContentWarningDisplay, DefaultTimeline, DisplayNameEmojiMode, HotkeyConfig,
		NotificationPreference, PerTimelineTemplates, PostTemplates, SortOrder,
	},
	html::{self, Link},
	mastodon::{
		Account as MastodonAccount, Filter, FilterAction, FilterContext, Mention, PollLimits, SearchType, Status, Tag,
	},
	network::{NetworkCommand, ProfileUpdate},
	template::{DEFAULT_BOOST_TEMPLATE, DEFAULT_POST_TEMPLATE},
	ui::ids::{ID_BOOST, ID_FAVORITE, ID_REPLY},
};

pub fn parse_instance_url(value: &str) -> Option<Url> {
	let trimmed = value.trim();
	if trimmed.is_empty() {
		return None;
	}
	let candidate = if trimmed.contains("://") { trimmed.to_string() } else { format!("https://{trimmed}") };
	let mut url = Url::parse(&candidate).ok()?;
	if url.host_str().is_none() || !matches!(url.scheme(), "https" | "http") {
		return None;
	}
	url.set_fragment(None);
	url.set_query(None);
	url.set_path("/");
	Some(url)
}

pub fn prompt_for_instance(frame: &Frame) -> Option<Url> {
	loop {
		let dialog = TextEntryDialog::builder(frame, "Enter your Mastodon instance", "Add Account")
			.with_style(TextEntryDialogStyle::Default | TextEntryDialogStyle::ProcessEnter)
			.build();
		if dialog.show_modal() != ID_OK {
			dialog.destroy();
			return None;
		}
		let value = dialog.get_value().unwrap_or_default();
		dialog.destroy();
		if let Some(instance) = parse_instance_url(&value) {
			return Some(instance);
		}
		show_warning(frame, "Please enter a valid instance URL.", "Invalid Instance");
	}
}

pub fn prompt_for_oauth_code(frame: &Frame, instance: &Url) -> Option<String> {
	let message =
		format!("After authorizing Fedra on {}, paste the code here.", instance.host_str().unwrap_or("your instance"));
	prompt_text(frame, &message, "Authorize Fedra")
}

pub fn prompt_for_access_token(frame: &Frame, instance: &Url) -> Option<String> {
	let message = format!(
		"OAuth failed. Create an access token on {} and paste it here.",
		instance.host_str().unwrap_or("your instance")
	);
	prompt_text(frame, &message, "Manual Access Token")
}

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

struct ComposeDialogConfig {
	title_prefix: String,
	ok_label: String,
	initial_content: String,
	initial_cw: Option<String>,
	initial_language: Option<String>,
	default_visibility: PostVisibility,
	can_change_visibility: bool,
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

fn normalize_language_code(input: &str) -> Option<String> {
	let trimmed = input.trim();
	if trimmed.is_empty() {
		return None;
	}
	Some(trimmed.to_ascii_lowercase())
}

#[allow(clippy::struct_excessive_bools)]
pub struct OptionsDialogInput {
	pub enter_to_send: bool,
	pub always_show_link_dialog: bool,
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
		strip_tracking: strip_tracking_checkbox.get_value(),
		quick_action_keys: quick_action_checkbox.get_value(),
		check_for_updates: update_checkbox.get_value(),
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
	let thread_checkbox = CheckBox::builder(&panel).with_label("Thread mode (Send and Reply)").build();
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

pub fn prompt_text(frame: &Frame, message: &str, title: &str) -> Option<String> {
	let dialog = TextEntryDialog::builder(frame, message, title)
		.with_style(TextEntryDialogStyle::Default | TextEntryDialogStyle::ProcessEnter)
		.build();
	if dialog.show_modal() != ID_OK {
		dialog.destroy();
		return None;
	}
	let value = dialog.get_value().unwrap_or_default();
	dialog.destroy();
	let trimmed = value.trim();
	if trimmed.is_empty() { None } else { Some(trimmed.to_string()) }
}

#[derive(Clone, Copy)]
pub enum UserLookupAction {
	Profile,
	Timeline,
}

pub fn prompt_for_user_lookup(
	frame: &Frame,
	suggestions: &[String],
	default_value: Option<&str>,
) -> Option<(String, UserLookupAction)> {
	const ID_VIEW_TIMELINE: i32 = 10040;
	let dialog = Dialog::builder(frame, "Open User").with_size(420, 180).build();
	let panel = Panel::builder(&dialog).build();
	let main_sizer = BoxSizer::builder(Orientation::Vertical).build();
	let prompt_label = StaticText::builder(&panel).with_label("Username:").build();
	let combo = ComboBox::builder(&panel).build();
	combo.freeze();
	for suggestion in suggestions {
		combo.append(suggestion);
	}
	combo.thaw();
	let button_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	let profile_button = Button::builder(&panel).with_id(ID_OK).with_label("View &Profile").build();
	let timeline_button = Button::builder(&panel).with_id(ID_VIEW_TIMELINE).with_label("View &Timeline").build();
	let cancel_button = Button::builder(&panel).with_id(ID_CANCEL).with_label("Cancel").build();
	profile_button.set_default();
	button_sizer.add(&profile_button, 0, SizerFlag::Right, 8);
	button_sizer.add(&timeline_button, 0, SizerFlag::Right, 8);
	button_sizer.add_stretch_spacer(1);
	button_sizer.add(&cancel_button, 0, SizerFlag::Right, 8);
	main_sizer.add(&prompt_label, 0, SizerFlag::Expand | SizerFlag::All, 8);
	main_sizer.add(&combo, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
	main_sizer.add_sizer(&button_sizer, 0, SizerFlag::Expand | SizerFlag::All, 8);
	panel.set_sizer(main_sizer, true);
	let dialog_sizer = BoxSizer::builder(Orientation::Vertical).build();
	dialog_sizer.add(&panel, 1, SizerFlag::Expand, 0);
	dialog.set_sizer(dialog_sizer, true);
	dialog.set_affirmative_id(ID_OK);
	dialog.set_escape_id(ID_CANCEL);
	if let Some(default_value) = default_value {
		combo.set_value(default_value);
		#[allow(clippy::cast_possible_wrap)]
		combo.set_text_selection(0, default_value.len() as i64);
	} else if !suggestions.is_empty() {
		combo.set_selection(0);
	}

	let dialog_timeline = dialog;
	timeline_button.on_click(move |_| {
		dialog_timeline.end_modal(ID_VIEW_TIMELINE);
	});

	dialog.centre();
	let result = dialog.show_modal();
	if result == ID_CANCEL {
		return None;
	}
	let value = combo.get_value();
	let trimmed = value.trim();
	if trimmed.is_empty() {
		return None;
	}
	let action = if result == ID_VIEW_TIMELINE { UserLookupAction::Timeline } else { UserLookupAction::Profile };
	Some((trimmed.to_string(), action))
}

pub fn show_error(frame: &Frame, err: &anyhow::Error) {
	let dialog = MessageDialog::builder(frame, &err.to_string(), "Fedra")
		.with_style(MessageDialogStyle::OK | MessageDialogStyle::IconError)
		.build();
	dialog.show_modal();
}

pub fn show_warning(frame: &Frame, message: &str, title: &str) {
	let dialog = MessageDialog::builder(frame, message, title)
		.with_style(MessageDialogStyle::OK | MessageDialogStyle::IconWarning)
		.build();
	dialog.show_modal();
}

fn show_warning_widget(parent: &dyn WxWidget, message: &str, title: &str) {
	let dialog = MessageDialog::builder(parent, message, title)
		.with_style(MessageDialogStyle::OK | MessageDialogStyle::IconWarning)
		.build();
	dialog.show_modal();
}

pub fn prompt_for_search(frame: &Frame) -> Option<(String, SearchType)> {
	let dialog = Dialog::builder(frame, "Search").with_size(420, 200).build();
	let panel = Panel::builder(&dialog).build();
	let main_sizer = BoxSizer::builder(Orientation::Vertical).build();
	let query_label = StaticText::builder(&panel).with_label("Search query:").build();
	let query_input = TextCtrl::builder(&panel).with_style(TextCtrlStyle::ProcessEnter).build();
	let type_label = StaticText::builder(&panel).with_label("Search &for").build();
	let type_choices = vec!["All".to_string(), "Accounts".to_string(), "Hashtags".to_string(), "Posts".to_string()];
	let type_choice = Choice::builder(&panel).with_choices(type_choices).build();
	type_choice.set_selection(0);
	let type_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	type_sizer.add(&type_label, 0, SizerFlag::AlignCenterVertical | SizerFlag::Right, 8);
	type_sizer.add(&type_choice, 1, SizerFlag::Expand, 0);
	let button_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	let search_button = Button::builder(&panel).with_id(ID_OK).with_label("Search").build();
	search_button.set_default();
	let cancel_button = Button::builder(&panel).with_id(ID_CANCEL).with_label("Cancel").build();
	button_sizer.add_stretch_spacer(1);
	button_sizer.add(&search_button, 0, SizerFlag::Right, 8);
	button_sizer.add(&cancel_button, 0, SizerFlag::Right, 8);
	main_sizer.add(&query_label, 0, SizerFlag::Expand | SizerFlag::All, 8);
	main_sizer.add(&query_input, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
	main_sizer.add_sizer(&type_sizer, 0, SizerFlag::Expand | SizerFlag::All, 8);
	main_sizer.add_sizer(&button_sizer, 0, SizerFlag::Expand | SizerFlag::All, 8);
	panel.set_sizer(main_sizer, true);
	let dialog_sizer = BoxSizer::builder(Orientation::Vertical).build();
	dialog_sizer.add(&panel, 1, SizerFlag::Expand, 0);
	dialog.set_sizer(dialog_sizer, true);
	dialog.set_affirmative_id(ID_OK);
	dialog.set_escape_id(ID_CANCEL);
	query_input.on_key_down(move |event| {
		if let WindowEventData::Keyboard(ref key_event) = event {
			if key_event.get_key_code() == Some(KEY_RETURN) && !key_event.shift_down() && !key_event.control_down() {
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
	query_input.set_focus();
	let result = dialog.show_modal();
	if result != ID_OK {
		return None;
	}
	let query = query_input.get_value();
	let trimmed = query.trim();
	if trimmed.is_empty() {
		return None;
	}
	let search_type = match type_choice.get_selection() {
		Some(1) => SearchType::Accounts,
		Some(2) => SearchType::Hashtags,
		Some(3) => SearchType::Statuses,
		_ => SearchType::All,
	};
	Some((trimmed.to_string(), search_type))
}

pub fn prompt_for_account_search(parent: &dyn WxWidget) -> Option<String> {
	let dialog = Dialog::builder(parent, "Search Accounts").with_size(420, 150).build();
	let panel = Panel::builder(&dialog).build();
	let main_sizer = BoxSizer::builder(Orientation::Vertical).build();
	let query_label = StaticText::builder(&panel).with_label("Search accounts:").build();
	let query_input = TextCtrl::builder(&panel).with_style(TextCtrlStyle::ProcessEnter).build();

	let button_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	let search_button = Button::builder(&panel).with_id(ID_OK).with_label("Search").build();
	search_button.set_default();
	let cancel_button = Button::builder(&panel).with_id(ID_CANCEL).with_label("Cancel").build();
	button_sizer.add_stretch_spacer(1);
	button_sizer.add(&search_button, 0, SizerFlag::Right, 8);
	button_sizer.add(&cancel_button, 0, SizerFlag::Right, 8);

	main_sizer.add(&query_label, 0, SizerFlag::Expand | SizerFlag::All, 8);
	main_sizer.add(&query_input, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
	main_sizer.add_sizer(&button_sizer, 0, SizerFlag::Expand | SizerFlag::All, 8);

	panel.set_sizer(main_sizer, true);
	let dialog_sizer = BoxSizer::builder(Orientation::Vertical).build();
	dialog_sizer.add(&panel, 1, SizerFlag::Expand, 0);
	dialog.set_sizer(dialog_sizer, true);
	dialog.set_affirmative_id(ID_OK);
	dialog.set_escape_id(ID_CANCEL);

	query_input.on_key_down(move |event| {
		if let WindowEventData::Keyboard(ref key_event) = event {
			if key_event.get_key_code() == Some(KEY_RETURN) && !key_event.shift_down() && !key_event.control_down() {
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
	query_input.set_focus();
	let result = dialog.show_modal();
	if result != ID_OK {
		return None;
	}
	let query = query_input.get_value();
	let trimmed = query.trim();
	if trimmed.is_empty() {
		return None;
	}
	Some(trimmed.to_string())
}

pub fn prompt_for_list_selection(frame: &Frame, lists: &[crate::mastodon::List]) -> Option<crate::mastodon::List> {
	let dialog = Dialog::builder(frame, "Open List").with_size(300, 400).build();
	let panel = Panel::builder(&dialog).build();
	let main_sizer = BoxSizer::builder(Orientation::Vertical).build();
	let list_label = StaticText::builder(&panel).with_label("Select a list:").build();
	let list_box = ListBox::builder(&panel).build();
	for list in lists {
		list_box.append(&list.title);
	}
	if !lists.is_empty() {
		list_box.set_selection(0, true);
	}
	let button_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	let ok_button = Button::builder(&panel).with_id(ID_OK).with_label("Open").build();
	ok_button.set_default();
	let cancel_button = Button::builder(&panel).with_id(ID_CANCEL).with_label("Cancel").build();
	button_sizer.add_stretch_spacer(1);
	button_sizer.add(&ok_button, 0, SizerFlag::Right, 8);
	button_sizer.add(&cancel_button, 0, SizerFlag::Right, 8);
	main_sizer.add(&list_label, 0, SizerFlag::Expand | SizerFlag::All, 8);
	main_sizer.add(&list_box, 1, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
	main_sizer.add_sizer(&button_sizer, 0, SizerFlag::Expand | SizerFlag::All, 8);
	panel.set_sizer(main_sizer, true);
	let dialog_sizer = BoxSizer::builder(Orientation::Vertical).build();
	dialog_sizer.add(&panel, 1, SizerFlag::Expand, 0);
	dialog.set_sizer(dialog_sizer, true);
	dialog.set_affirmative_id(ID_OK);
	dialog.set_escape_id(ID_CANCEL);
	dialog.centre();

	let result = dialog.show_modal();
	if result == ID_OK {
		let selection = list_box.get_selection().map(|s| s as usize);
		if let Some(index) = selection {
			return lists.get(index).cloned();
		}
	}
	None
}

#[derive(Clone)]
pub struct ManageListsDialog {
	dialog: Dialog,
	lists_ctrl: ListBox,
	edit_button: Button,
	members_button: Button,
	remove_button: Button,
	lists: Rc<RefCell<Vec<crate::mastodon::List>>>,
}

impl ManageListsDialog {
	pub fn new<F>(frame: &Frame, lists: Vec<crate::mastodon::List>, net_tx: Sender<NetworkCommand>, on_close: F) -> Self
	where
		F: Fn() + 'static + Clone,
	{
		let dialog = Dialog::builder(frame, "List Manager").with_size(450, 350).build();
		let panel = Panel::builder(&dialog).build();
		let main_sizer = BoxSizer::builder(Orientation::Vertical).build();
		let lists_label = StaticText::builder(&panel).with_label("Lists:").build();
		let lists_ctrl = ListBox::builder(&panel).build();

		let buttons_sizer = BoxSizer::builder(Orientation::Horizontal).build();
		let add_button = Button::builder(&panel).with_label("Add...").build();
		let edit_button = Button::builder(&panel).with_label("Edit...").build();
		let members_button = Button::builder(&panel).with_label("Members...").build();
		let remove_button = Button::builder(&panel).with_label("Delete").build();
		let close_button = Button::builder(&panel).with_id(ID_CANCEL).with_label("Close").build();
		close_button.set_default();

		buttons_sizer.add(&add_button, 0, SizerFlag::Right, 8);
		buttons_sizer.add(&edit_button, 0, SizerFlag::Right, 8);
		buttons_sizer.add(&members_button, 0, SizerFlag::Right, 8);
		buttons_sizer.add(&remove_button, 0, SizerFlag::Right, 8);
		buttons_sizer.add_stretch_spacer(1);
		buttons_sizer.add(&close_button, 0, SizerFlag::Right, 8);

		main_sizer.add(&lists_label, 0, SizerFlag::Expand | SizerFlag::All, 8);
		main_sizer.add(&lists_ctrl, 1, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
		main_sizer.add_sizer(&buttons_sizer, 0, SizerFlag::Expand | SizerFlag::All, 8);

		panel.set_sizer(main_sizer, true);
		let dialog_sizer = BoxSizer::builder(Orientation::Vertical).build();
		dialog_sizer.add(&panel, 1, SizerFlag::Expand, 0);
		dialog.set_sizer(dialog_sizer, true);

		edit_button.enable(false);
		members_button.enable(false);
		remove_button.enable(false);

		let lists_rc = Rc::new(RefCell::new(lists));
		let handle = Self { dialog, lists_ctrl, edit_button, members_button, remove_button, lists: lists_rc };

		handle.update_list_display();

		let lists_select = lists_ctrl;
		let edit_btn_select = edit_button;
		let members_btn_select = members_button;
		let remove_btn_select = remove_button;

		lists_select.on_selection_changed(move |_| {
			let has_selection = lists_select.get_selection().is_some();
			edit_btn_select.enable(has_selection);
			members_btn_select.enable(has_selection);
			remove_btn_select.enable(has_selection);
		});

		let net_tx_add = net_tx.clone();
		let dialog_add = handle.dialog;
		add_button.on_click(move |_| {
			if let Some((title, policy, exclusive)) = prompt_list_edit(&dialog_add, None, None, false) {
				let _ = net_tx_add.send(NetworkCommand::CreateList { title, policy, exclusive });
			}
		});

		let lists_edit = handle.lists.clone();
		let list_ctrl_edit = handle.lists_ctrl;
		let net_tx_edit = net_tx.clone();
		let dialog_edit = handle.dialog;
		edit_button.on_click(move |_| {
			if let Some(sel) = list_ctrl_edit.get_selection() {
				let idx = sel as usize;
				let lists = lists_edit.borrow();
				if let Some(list) = lists.get(idx)
					&& let Some((title, policy, exclusive)) = prompt_list_edit(
						&dialog_edit,
						Some(&list.title),
						list.replies_policy.as_deref(),
						list.exclusive,
					) {
					let _ =
						net_tx_edit.send(NetworkCommand::UpdateList { id: list.id.clone(), title, policy, exclusive });
				}
			}
		});

		let lists_members = handle.lists.clone();
		let list_ctrl_members = handle.lists_ctrl;
		let net_tx_members = net_tx.clone();
		members_button.on_click(move |_| {
			if let Some(sel) = list_ctrl_members.get_selection() {
				let idx = sel as usize;
				let lists = lists_members.borrow();
				if let Some(list) = lists.get(idx) {
					let _ = net_tx_members.send(NetworkCommand::FetchListAccounts { list_id: list.id.clone() });
				}
			}
		});

		let lists_remove = handle.lists.clone();
		let list_ctrl_remove = handle.lists_ctrl;
		let net_tx_remove = net_tx;
		let parent_remove = handle.dialog;
		remove_button.on_click(move |_| {
			if let Some(sel) = list_ctrl_remove.get_selection() {
				let idx = sel as usize;
				let lists = lists_remove.borrow();
				if let Some(list) = lists.get(idx) {
					let warning = MessageDialog::builder(
						&parent_remove,
						"Are you sure you want to delete this list?",
						"Delete List",
					)
					.with_style(MessageDialogStyle::YesNo | MessageDialogStyle::IconWarning)
					.build();
					if warning.show_modal() == ID_YES {
						let _ = net_tx_remove.send(NetworkCommand::DeleteList { id: list.id.clone() });
					}
				}
			}
		});

		let dlg_close = handle.dialog;
		close_button.on_click(move |_| {
			dlg_close.close(true);
		});

		let on_close_win = on_close;
		handle.dialog.on_close(move |_| {
			on_close_win();
		});

		handle
	}

	pub fn show(&self) {
		self.dialog.show(true);
	}

	pub fn update_lists(&self, new_lists: Vec<crate::mastodon::List>) {
		*self.lists.borrow_mut() = new_lists;
		self.update_list_display();
	}

	fn update_list_display(&self) {
		let prev_sel = self.lists_ctrl.get_selection();
		self.lists_ctrl.clear();
		for list in self.lists.borrow().iter() {
			self.lists_ctrl.append(&list.title);
		}
		if let Some(sel) = prev_sel {
			if (sel as usize) < self.lists_ctrl.get_count() as usize {
				self.lists_ctrl.set_selection(sel, true);
			} else {
				self.edit_button.enable(false);
				self.members_button.enable(false);
				self.remove_button.enable(false);
			}
		} else {
			self.edit_button.enable(false);
			self.members_button.enable(false);
			self.remove_button.enable(false);
		}
	}

	pub fn get_list_title(&self, list_id: &str) -> Option<String> {
		self.lists.borrow().iter().find(|l| l.id == list_id).map(|l| l.title.clone())
	}
}

pub fn prompt_list_edit(
	parent: &dyn WxWidget,
	initial_title: Option<&str>,
	initial_policy: Option<&str>,
	initial_exclusive: bool,
) -> Option<(String, String, bool)> {
	let title_str = if initial_title.is_some() { "Edit List" } else { "Create List" };
	let dialog = Dialog::builder(parent, title_str).with_size(400, 250).build();
	let panel = Panel::builder(&dialog).build();
	let main_sizer = BoxSizer::builder(Orientation::Vertical).build();

	let title_label = StaticText::builder(&panel).with_label("List Title:").build();
	let title_input = TextCtrl::builder(&panel).with_style(TextCtrlStyle::ProcessEnter).build();
	if let Some(t) = initial_title {
		title_input.set_value(t);
	}

	let policy_label = StaticText::builder(&panel).with_label("Replies Policy:").build();
	let policy_choices = vec![
		"Show replies to followed users".to_string(),
		"Show replies to list members".to_string(),
		"No replies".to_string(),
	];
	let policy_values = ["followed", "list", "none"];
	let policy_choice = Choice::builder(&panel).with_choices(policy_choices).build();
	let policy_idx = initial_policy.and_then(|p| policy_values.iter().position(|&v| v == p)).unwrap_or(0);
	policy_choice.set_selection(u32::try_from(policy_idx).unwrap_or(0));

	let exclusive_check = CheckBox::builder(&panel).with_label("Hide these posts from Home timeline").build();
	exclusive_check.set_value(initial_exclusive);

	let button_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	let ok_button = Button::builder(&panel).with_id(ID_OK).with_label("Save").build();
	ok_button.set_default();
	let cancel_button = Button::builder(&panel).with_id(ID_CANCEL).with_label("Cancel").build();
	button_sizer.add_stretch_spacer(1);
	button_sizer.add(&ok_button, 0, SizerFlag::Right, 8);
	button_sizer.add(&cancel_button, 0, SizerFlag::Right, 8);

	main_sizer.add(&title_label, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Top, 8);
	main_sizer.add(&title_input, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
	main_sizer.add(&policy_label, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Top, 8);
	main_sizer.add(&policy_choice, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
	main_sizer.add(&exclusive_check, 0, SizerFlag::Expand | SizerFlag::All, 8);
	main_sizer.add_sizer(&button_sizer, 0, SizerFlag::Expand | SizerFlag::All, 8);

	panel.set_sizer(main_sizer, true);
	let dialog_sizer = BoxSizer::builder(Orientation::Vertical).build();
	dialog_sizer.add(&panel, 1, SizerFlag::Expand, 0);
	dialog.set_sizer(dialog_sizer, true);
	dialog.set_affirmative_id(ID_OK);
	dialog.set_escape_id(ID_CANCEL);

	let title_enter = title_input;
	let dialog_enter = dialog;
	title_enter.on_key_down(move |event| {
		if let WindowEventData::Keyboard(ref key_event) = event {
			if key_event.get_key_code() == Some(13) && !key_event.shift_down() && !key_event.control_down() {
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
	title_input.set_focus();
	if dialog.show_modal() != ID_OK {
		return None;
	}

	let title = title_input.get_value().trim().to_string();
	if title.is_empty() {
		return None;
	}
	let policy_sel = policy_choice.get_selection().unwrap_or(0) as usize;
	let policy = policy_values.get(policy_sel).unwrap_or(&"followed").to_string();

	Some((title, policy, exclusive_check.get_value()))
}
#[derive(Clone)]
pub struct ManageListMembersDialog {
	dialog: Dialog,
	members_list: ListBox,
	remove_button: Button,
	members: Rc<RefCell<Vec<crate::mastodon::Account>>>,
	list_id: String,
}

impl ManageListMembersDialog {
	pub fn new<F>(
		frame: &Frame,
		list_id: String,
		list_title: &str,
		members: Vec<crate::mastodon::Account>,
		net_tx: Sender<NetworkCommand>,
		on_close: F,
	) -> Self
	where
		F: Fn() + 'static + Clone,
	{
		let dialog = Dialog::builder(frame, &format!("Manage Members: {list_title}")).with_size(450, 400).build();
		let panel = Panel::builder(&dialog).build();
		let main_sizer = BoxSizer::builder(Orientation::Vertical).build();

		let members_label = StaticText::builder(&panel).with_label("Members:").build();
		let members_list = ListBox::builder(&panel).build();

		let buttons_sizer = BoxSizer::builder(Orientation::Horizontal).build();
		let add_button = Button::builder(&panel).with_label("Add Member...").build();
		let remove_button = Button::builder(&panel).with_label("Remove Member").build();
		let close_button = Button::builder(&panel).with_id(ID_CANCEL).with_label("Close").build();
		close_button.set_default();

		buttons_sizer.add(&add_button, 0, SizerFlag::Right, 8);
		buttons_sizer.add(&remove_button, 0, SizerFlag::Right, 8);
		buttons_sizer.add_stretch_spacer(1);
		buttons_sizer.add(&close_button, 0, SizerFlag::Right, 8);

		main_sizer.add(&members_label, 0, SizerFlag::Expand | SizerFlag::All, 8);
		main_sizer.add(&members_list, 1, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
		main_sizer.add_sizer(&buttons_sizer, 0, SizerFlag::Expand | SizerFlag::All, 8);

		panel.set_sizer(main_sizer, true);
		let dialog_sizer = BoxSizer::builder(Orientation::Vertical).build();
		dialog_sizer.add(&panel, 1, SizerFlag::Expand, 0);
		dialog.set_sizer(dialog_sizer, true);

		remove_button.enable(false);

		let members_rc = Rc::new(RefCell::new(members));
		let handle = Self { dialog, members_list, remove_button, members: members_rc, list_id: list_id.clone() };

		handle.update_members_display();

		let list_select = members_list;
		let remove_btn_select = remove_button;

		list_select.on_selection_changed(move |_| {
			remove_btn_select.enable(list_select.get_selection().is_some());
		});

		let dialog_add = handle.dialog;
		let net_tx_search = net_tx.clone();

		add_button.on_click(move |_| {
			if let Some(query) = prompt_for_account_search(&dialog_add) {
				let _ = net_tx_search.send(NetworkCommand::Search {
					query,
					search_type: crate::mastodon::SearchType::Accounts,
					limit: Some(20),
					offset: None,
				});
			}
		});

		let members_remove = handle.members.clone();
		let list_remove = handle.members_list;
		let net_tx_remove = net_tx;
		let list_id_remove = list_id;

		remove_button.on_click(move |_| {
			if let Some(sel) = list_remove.get_selection() {
				let idx = sel as usize;
				let members = members_remove.borrow();
				if let Some(member) = members.get(idx) {
					let _ = net_tx_remove.send(NetworkCommand::RemoveListAccount {
						list_id: list_id_remove.clone(),
						account_id: member.id.clone(),
					});
				}
			}
		});

		let dlg_close = handle.dialog;
		close_button.on_click(move |_| {
			dlg_close.close(true);
		});

		let on_close_win = on_close;
		handle.dialog.on_close(move |_| {
			on_close_win();
		});

		handle
	}

	pub fn show(&self) {
		self.dialog.show(true);
	}

	#[allow(dead_code)]
	pub fn update_members(&self, new_members: Vec<crate::mastodon::Account>) {
		*self.members.borrow_mut() = new_members;
		self.update_members_display();
	}

	#[allow(dead_code)]
	pub fn add_member(&self, member: crate::mastodon::Account) {
		self.members.borrow_mut().push(member);
		self.update_members_display();
	}

	#[allow(dead_code)]
	pub fn remove_member(&self, account_id: &str) {
		self.members.borrow_mut().retain(|a| a.id != account_id);
		self.update_members_display();
	}

	fn update_members_display(&self) {
		let prev_sel = self.members_list.get_selection();
		self.members_list.clear();
		for member in self.members.borrow().iter() {
			self.members_list.append(member.display_name_or_username());
		}
		if let Some(sel) = prev_sel {
			if (sel as usize) < self.members_list.get_count() as usize {
				self.members_list.set_selection(sel, true);
			} else {
				self.remove_button.enable(false);
			}
		} else {
			self.remove_button.enable(false);
		}
	}

	pub fn get_list_id(&self) -> &str {
		&self.list_id
	}
}

#[derive(Clone)]
pub enum ManageAccountsResult {
	Add,
	Remove(String),
	Switch(String),
	None,
}

pub fn prompt_manage_accounts(frame: &Frame, accounts: &[Account], active_id: Option<&str>) -> ManageAccountsResult {
	let dialog = Dialog::builder(frame, "Account Manager").with_size(400, 350).build();
	let panel = Panel::builder(&dialog).build();
	let main_sizer = BoxSizer::builder(Orientation::Vertical).build();
	let accounts_label = StaticText::builder(&panel).with_label("Accounts:").build();
	let accounts_list = ListBox::builder(&panel).build();
	let format_account = |account: &Account| -> String {
		let host = Url::parse(&account.instance)
			.ok()
			.and_then(|u| u.host_str().map(std::string::ToString::to_string))
			.unwrap_or_default();
		let username = account.acct.as_deref().unwrap_or("?");
		if username.contains('@') { format!("@{username}") } else { format!("@{username}@{host}") }
	};
	let active_index = active_id.and_then(|id| accounts.iter().position(|a| a.id == id));
	for (i, account) in accounts.iter().enumerate() {
		let handle = format_account(account);
		let name = account.display_name.as_deref().unwrap_or("Unknown");
		let status = if Some(i) == active_index { "active" } else { "inactive" };
		accounts_list.append(&format!("{name}, {handle}, {status}"));
	}
	if let Some(index) = active_index
		&& let Ok(selection) = u32::try_from(index)
	{
		accounts_list.set_selection(selection, true);
	}
	let buttons_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	let add_button = Button::builder(&panel).with_label("Add...").build();
	let remove_button = Button::builder(&panel).with_label("Remove").build();
	let switch_button = Button::builder(&panel).with_label("Switch To").build();
	switch_button.set_default();
	let close_button = Button::builder(&panel).with_id(ID_CANCEL).with_label("Close").build();
	buttons_sizer.add(&add_button, 0, SizerFlag::Right, 8);
	buttons_sizer.add(&remove_button, 0, SizerFlag::Right, 8);
	buttons_sizer.add(&switch_button, 0, SizerFlag::Right, 8);
	buttons_sizer.add_stretch_spacer(1);
	buttons_sizer.add(&close_button, 0, SizerFlag::Right, 8);
	main_sizer.add(&accounts_label, 0, SizerFlag::Expand | SizerFlag::All, 8);
	main_sizer.add(&accounts_list, 1, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
	main_sizer.add_sizer(&buttons_sizer, 0, SizerFlag::Expand | SizerFlag::All, 8);
	panel.set_sizer(main_sizer, true);
	let dialog_sizer = BoxSizer::builder(Orientation::Vertical).build();
	dialog_sizer.add(&panel, 1, SizerFlag::Expand, 0);
	dialog.set_sizer(dialog_sizer, true);
	dialog.set_affirmative_id(ID_CANCEL);
	dialog.set_escape_id(ID_CANCEL);
	remove_button.enable(false);
	switch_button.enable(false);
	let result = Rc::new(RefCell::new(ManageAccountsResult::None));
	let accounts_list_select = accounts_list;
	let remove_button_select = remove_button;
	let switch_button_select = switch_button;
	let accounts_ref: Vec<Account> = accounts.to_vec();
	let active_idx = active_index;
	accounts_list.on_selection_changed(move |_| {
		if let Some(sel) = accounts_list_select.get_selection() {
			let idx = sel as usize;
			remove_button_select.enable(true);
			let is_active = active_idx == Some(idx);
			switch_button_select.enable(!is_active);
			if !is_active && idx < accounts_ref.len() {
				let handle = format_account(&accounts_ref[idx]);
				switch_button_select.set_label(&format!("Switch to {handle}"));
			} else {
				switch_button_select.set_label("Switch To");
			}
		} else {
			remove_button_select.enable(false);
			switch_button_select.enable(false);
			switch_button_select.set_label("Switch To");
		}
	});
	if let Some(idx) = active_index {
		if let Ok(selection) = u32::try_from(idx) {
			accounts_list.set_selection(selection, true);
		}
		remove_button.enable(true);
		switch_button.enable(false);
		switch_button.set_label("Switch To");
	}
	let result_add = result.clone();
	add_button.on_click(move |_| {
		*result_add.borrow_mut() = ManageAccountsResult::Add;
		dialog.end_modal(ID_OK);
	});
	let result_remove = result.clone();
	let accounts_list_remove = accounts_list;
	let account_ids: Vec<String> = accounts.iter().map(|a| a.id.clone()).collect();
	let account_ids_remove = account_ids.clone();
	let parent = dialog;
	remove_button.on_click(move |_| {
		if let Some(sel) = accounts_list_remove.get_selection() {
			let idx = sel as usize;
			if idx < account_ids_remove.len() {
				let warning = MessageDialog::builder(
					&parent,
					"Are you sure you want to remove this account? This cannot be undone.",
					"Remove Account",
				)
				.with_style(MessageDialogStyle::YesNo | MessageDialogStyle::IconWarning)
				.build();
				if warning.show_modal() == ID_YES {
					*result_remove.borrow_mut() = ManageAccountsResult::Remove(account_ids_remove[idx].clone());
					dialog.end_modal(ID_OK);
				}
			}
		}
	});
	let result_switch = result.clone();
	let accounts_list_switch = accounts_list;
	let account_ids_switch = account_ids;
	switch_button.on_click(move |_| {
		if let Some(sel) = accounts_list_switch.get_selection() {
			let idx = sel as usize;
			if idx < account_ids_switch.len() {
				*result_switch.borrow_mut() = ManageAccountsResult::Switch(account_ids_switch[idx].clone());
				dialog.end_modal(ID_OK);
			}
		}
	});
	dialog.centre();
	dialog.show_modal();
	result.borrow().clone()
}

#[derive(Clone)]
pub enum ManageFiltersResult {
	Add,
	Edit(String),
	Delete(String),
	None,
}

pub fn prompt_manage_filters(frame: &Frame, filters: &[Filter]) -> ManageFiltersResult {
	let dialog = Dialog::builder(frame, "Filter Manager").with_size(400, 350).build();
	let panel = Panel::builder(&dialog).build();
	let main_sizer = BoxSizer::builder(Orientation::Vertical).build();
	let filters_label = StaticText::builder(&panel).with_label("Filters:").build();
	let filters_list = ListBox::builder(&panel).build();
	for filter in filters {
		let action_label = match &filter.action {
			FilterAction::Warn => "Hide with warning",
			FilterAction::Hide => "Hide completely",
			FilterAction::Blur => "Hide media with warning",
			FilterAction::Other(s) => s,
		};
		let label = format!("{} ({})", filter.title, action_label);
		filters_list.append(&label);
	}
	let buttons_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	let add_button = Button::builder(&panel).with_label("Add...").build();
	let edit_button = Button::builder(&panel).with_label("Edit...").build();
	let remove_button = Button::builder(&panel).with_label("Delete").build();
	let close_button = Button::builder(&panel).with_id(ID_CANCEL).with_label("Close").build();
	close_button.set_default();
	buttons_sizer.add(&add_button, 0, SizerFlag::Right, 8);
	buttons_sizer.add(&edit_button, 0, SizerFlag::Right, 8);
	buttons_sizer.add(&remove_button, 0, SizerFlag::Right, 8);
	buttons_sizer.add_stretch_spacer(1);
	buttons_sizer.add(&close_button, 0, SizerFlag::Right, 8);
	main_sizer.add(&filters_label, 0, SizerFlag::Expand | SizerFlag::All, 8);
	main_sizer.add(&filters_list, 1, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
	main_sizer.add_sizer(&buttons_sizer, 0, SizerFlag::Expand | SizerFlag::All, 8);
	panel.set_sizer(main_sizer, true);
	let dialog_sizer = BoxSizer::builder(Orientation::Vertical).build();
	dialog_sizer.add(&panel, 1, SizerFlag::Expand, 0);
	dialog.set_sizer(dialog_sizer, true);
	dialog.set_affirmative_id(ID_CANCEL);
	dialog.set_escape_id(ID_CANCEL);
	edit_button.enable(false);
	remove_button.enable(false);
	let result = Rc::new(RefCell::new(ManageFiltersResult::None));
	let filters_list_select = filters_list;
	let edit_button_select = edit_button;
	let remove_button_select = remove_button;
	filters_list.on_selection_changed(move |_| {
		let has_selection = filters_list_select.get_selection().is_some();
		edit_button_select.enable(has_selection);
		remove_button_select.enable(has_selection);
	});
	let result_add = result.clone();
	add_button.on_click(move |_| {
		*result_add.borrow_mut() = ManageFiltersResult::Add;
		dialog.end_modal(ID_OK);
	});
	let result_edit = result.clone();
	let filters_list_edit = filters_list;
	let filter_ids: Vec<String> = filters.iter().map(|f| f.id.clone()).collect();
	let filter_ids_edit = filter_ids.clone();
	edit_button.on_click(move |_| {
		if let Some(sel) = filters_list_edit.get_selection() {
			let idx = sel as usize;
			if idx < filter_ids_edit.len() {
				*result_edit.borrow_mut() = ManageFiltersResult::Edit(filter_ids_edit[idx].clone());
				dialog.end_modal(ID_OK);
			}
		}
	});
	let result_remove = result.clone();
	let filters_list_remove = filters_list;
	let filter_ids_remove = filter_ids;
	let parent = dialog;
	remove_button.on_click(move |_| {
		if let Some(sel) = filters_list_remove.get_selection() {
			let idx = sel as usize;
			if idx < filter_ids_remove.len() {
				let warning =
					MessageDialog::builder(&parent, "Are you sure you want to delete this filter?", "Delete Filter")
						.with_style(MessageDialogStyle::YesNo | MessageDialogStyle::IconWarning)
						.build();
				if warning.show_modal() == ID_YES {
					*result_remove.borrow_mut() = ManageFiltersResult::Delete(filter_ids_remove[idx].clone());
					dialog.end_modal(ID_OK);
				}
			}
		}
	});
	dialog.centre();
	dialog.show_modal();
	result.borrow().clone()
}

pub struct FilterDialogResult {
	pub title: String,
	pub contexts: Vec<FilterContext>,
	pub action: FilterAction,
	pub keywords: Vec<(String, String, bool, bool)>,
	pub expires_in: Option<u32>,
}

#[derive(Clone)]
struct KeywordEntry {
	id: String,
	keyword: String,
	whole_word: bool,
	destroyed: bool,
}

fn prompt_keyword_edit(
	parent: &dyn WxWidget,
	initial_keyword: Option<&str>,
	initial_whole_word: bool,
) -> Option<(String, bool)> {
	let title = if initial_keyword.is_some() { "Edit Keyword" } else { "Add Keyword" };
	let dialog = Dialog::builder(parent, title).with_size(400, 200).build();
	let panel = Panel::builder(&dialog).build();
	let main_sizer = BoxSizer::builder(Orientation::Vertical).build();

	let keyword_label = StaticText::builder(&panel).with_label("Keyword:").build();
	let keyword_input = TextCtrl::builder(&panel).with_style(TextCtrlStyle::ProcessEnter).build();
	if let Some(k) = initial_keyword {
		keyword_input.set_value(k);
	}
	let whole_word_check = CheckBox::builder(&panel).with_label("Whole word").build();
	whole_word_check.set_value(initial_whole_word);

	main_sizer.add(&keyword_label, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Top, 8);
	main_sizer.add(&keyword_input, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
	main_sizer.add(&whole_word_check, 0, SizerFlag::Expand | SizerFlag::All, 8);

	let buttons_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	let ok_button = Button::builder(&panel).with_id(ID_OK).with_label("OK").build();
	ok_button.set_default();
	let cancel_button = Button::builder(&panel).with_id(ID_CANCEL).with_label("Cancel").build();
	buttons_sizer.add_stretch_spacer(1);
	buttons_sizer.add(&ok_button, 0, SizerFlag::Right, 8);
	buttons_sizer.add(&cancel_button, 0, SizerFlag::Right, 8);
	main_sizer.add_sizer(&buttons_sizer, 0, SizerFlag::Expand | SizerFlag::All, 8);

	panel.set_sizer(main_sizer, true);
	let dialog_sizer = BoxSizer::builder(Orientation::Vertical).build();
	dialog_sizer.add(&panel, 1, SizerFlag::Expand, 0);
	dialog.set_sizer(dialog_sizer, true);
	dialog.set_affirmative_id(ID_OK);
	dialog.set_escape_id(ID_CANCEL);

	let input_enter = keyword_input;
	let dialog_enter = dialog;
	input_enter.on_key_down(move |event| {
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
	keyword_input.set_focus();
	if dialog.show_modal() != ID_OK {
		return None;
	}

	let text = keyword_input.get_value();
	let trimmed = text.trim();
	if trimmed.is_empty() {
		return None;
	}
	Some((trimmed.to_string(), whole_word_check.get_value()))
}

pub fn prompt_filter_edit(frame: &Frame, existing: Option<&Filter>) -> Option<FilterDialogResult> {
	let title = if existing.is_some() { "Edit Filter" } else { "Add Filter" };
	let dialog = Dialog::builder(frame, title).with_size(500, 600).build();
	let panel = Panel::builder(&dialog).build();
	let main_sizer = BoxSizer::builder(Orientation::Vertical).build();
	let title_label = StaticText::builder(&panel).with_label("Filter Title:").build();
	let title_text = TextCtrl::builder(&panel).build();
	if let Some(f) = existing {
		title_text.set_value(&f.title);
	}
	main_sizer.add(&title_label, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Top, 8);
	main_sizer.add(&title_text, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
	let context_label = StaticText::builder(&panel).with_label("Contexts:").build();
	main_sizer.add(&context_label, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Top, 8);
	let context_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	let mut context_checks = Vec::new();
	let all_contexts = [
		FilterContext::Home,
		FilterContext::Notifications,
		FilterContext::Public,
		FilterContext::Thread,
		FilterContext::Account,
	];
	for context in &all_contexts {
		let cb = CheckBox::builder(&panel).with_label(&format!("{context}")).build();
		if let Some(f) = existing
			&& f.context.contains(context)
		{
			cb.set_value(true);
		}
		context_sizer.add(&cb, 1, SizerFlag::Expand, 4);
		context_checks.push((cb, context.clone()));
	}
	main_sizer.add_sizer(&context_sizer, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
	let action_label = StaticText::builder(&panel).with_label("Action:").build();
	let mut action_choices =
		vec!["Hide with warning".to_string(), "Hide completely".to_string(), "Hide media with warning".to_string()];
	let mut custom_action = None;
	if let Some(f) = existing {
		match &f.action {
			FilterAction::Warn | FilterAction::Hide | FilterAction::Blur => {}
			FilterAction::Other(s) => {
				action_choices.push(format!("Custom: {s}"));
				custom_action = Some(s.clone());
			}
		}
	}
	let action_choice = Choice::builder(&panel).with_choices(action_choices).build();
	if let Some(f) = existing {
		match f.action {
			FilterAction::Warn => action_choice.set_selection(0),
			FilterAction::Hide => action_choice.set_selection(1),
			FilterAction::Blur => action_choice.set_selection(2),
			FilterAction::Other(_) => action_choice.set_selection(3),
		}
	} else {
		action_choice.set_selection(0);
	}
	let action_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	action_sizer.add(&action_label, 0, SizerFlag::AlignCenterVertical | SizerFlag::Right, 8);
	action_sizer.add(&action_choice, 1, SizerFlag::Expand, 0);
	main_sizer.add_sizer(&action_sizer, 0, SizerFlag::Expand | SizerFlag::All, 8);
	let expiry_label = StaticText::builder(&panel).with_label("Expires in:").build();
	let expiry_choices = vec![
		"Never".to_string(),
		"30 minutes".to_string(),
		"1 hour".to_string(),
		"6 hours".to_string(),
		"12 hours".to_string(),
		"1 day".to_string(),
		"1 week".to_string(),
	];
	let expiry_choice = Choice::builder(&panel).with_choices(expiry_choices).build();
	expiry_choice.set_selection(0);
	let expiry_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	expiry_sizer.add(&expiry_label, 0, SizerFlag::AlignCenterVertical | SizerFlag::Right, 8);
	expiry_sizer.add(&expiry_choice, 1, SizerFlag::Expand, 0);
	main_sizer.add_sizer(&expiry_sizer, 0, SizerFlag::Expand | SizerFlag::All, 8);
	let keywords_label = StaticText::builder(&panel).with_label("Keywords:").build();
	let keywords_list = ListBox::builder(&panel).build();
	let add_keyword_button = Button::builder(&panel).with_label("Add Keyword...").build();
	let edit_keyword_button = Button::builder(&panel).with_label("Edit Keyword...").build();
	let remove_keyword_button = Button::builder(&panel).with_label("Remove Selected").build();

	main_sizer.add(&keywords_label, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Top, 8);
	main_sizer.add(&keywords_list, 1, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);

	let keyword_actions_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	keyword_actions_sizer.add(&add_keyword_button, 0, SizerFlag::Right, 8);
	keyword_actions_sizer.add(&edit_keyword_button, 0, SizerFlag::Right, 8);
	keyword_actions_sizer.add_stretch_spacer(1);
	keyword_actions_sizer.add(&remove_keyword_button, 0, SizerFlag::Right, 8);
	main_sizer.add_sizer(&keyword_actions_sizer, 0, SizerFlag::Expand | SizerFlag::All, 8);
	let buttons_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	let save_button = Button::builder(&panel).with_id(ID_OK).with_label("Save").build();
	save_button.set_default();
	let cancel_button = Button::builder(&panel).with_id(ID_CANCEL).with_label("Cancel").build();
	buttons_sizer.add_stretch_spacer(1);
	buttons_sizer.add(&save_button, 0, SizerFlag::Right, 8);
	buttons_sizer.add(&cancel_button, 0, SizerFlag::Right, 8);
	main_sizer.add_sizer(&buttons_sizer, 0, SizerFlag::Expand | SizerFlag::All, 8);

	panel.set_sizer(main_sizer, true);
	let dialog_sizer = BoxSizer::builder(Orientation::Vertical).build();
	dialog_sizer.add(&panel, 1, SizerFlag::Expand, 0);
	dialog.set_sizer(dialog_sizer, true);
	dialog.set_affirmative_id(ID_OK);
	dialog.set_escape_id(ID_CANCEL);
	let initial_keywords: Vec<KeywordEntry> = existing.map_or_else(Vec::new, |f| {
		f.keywords
			.iter()
			.map(|k| KeywordEntry {
				id: k.id.clone(),
				keyword: k.keyword.clone(),
				whole_word: k.whole_word,
				destroyed: false,
			})
			.collect()
	});

	let keywords = Rc::new(RefCell::new(initial_keywords));
	let refresh_keywords = {
		let keywords = keywords.clone();
		let list = keywords_list;
		move || {
			let previous_sel = list.get_selection();
			list.clear();
			for k in keywords.borrow().iter() {
				if !k.destroyed {
					let label = if k.whole_word { format!("{} (Whole word)", k.keyword) } else { k.keyword.clone() };
					list.append(&label);
				}
			}
			if let Some(sel) = previous_sel
				&& (sel as usize) < list.get_count() as usize
			{
				list.set_selection(sel, true);
			}
		}
	};
	refresh_keywords();
	let list_select = keywords_list;
	let edit_btn_select = edit_keyword_button;
	let remove_btn_select = remove_keyword_button;
	edit_btn_select.enable(false);
	remove_btn_select.enable(false);

	list_select.on_selection_changed(move |_| {
		let has_sel = list_select.get_selection().is_some();
		edit_btn_select.enable(has_sel);
		remove_btn_select.enable(has_sel);
	});
	let keywords_add = keywords.clone();
	let refresh_add = refresh_keywords.clone();
	let dialog_add = dialog;
	add_keyword_button.on_click(move |_| {
		if let Some((keyword, whole_word)) = prompt_keyword_edit(&dialog_add, None, false) {
			keywords_add.borrow_mut().push(KeywordEntry { id: String::new(), keyword, whole_word, destroyed: false });
			refresh_add();
		}
	});
	let keywords_edit = keywords.clone();
	let list_edit = keywords_list;
	let refresh_edit = refresh_keywords.clone();
	let dialog_edit = dialog;
	edit_keyword_button.on_click(move |_| {
		if let Some(sel) = list_edit.get_selection() {
			let idx = sel as usize;
			let mut visual_count = 0;
			let (current_kw, current_whole_word) = {
				let k = keywords_edit.borrow();
				let mut result = None;
				for entry in k.iter() {
					if !entry.destroyed {
						if visual_count == idx {
							result = Some((entry.keyword.clone(), entry.whole_word));
							break;
						}
						visual_count += 1;
					}
				}
				match result {
					Some(r) => r,
					None => return,
				}
			};

			if let Some((new_kw, new_ww)) = prompt_keyword_edit(&dialog_edit, Some(&current_kw), current_whole_word) {
				let mut k_mut = keywords_edit.borrow_mut();
				visual_count = 0;
				for entry in k_mut.iter_mut() {
					if !entry.destroyed {
						if visual_count == idx {
							entry.keyword = new_kw;
							entry.whole_word = new_ww;
							break;
						}
						visual_count += 1;
					}
				}
				drop(k_mut);
				refresh_edit();
			}
		}
	});
	let keywords_remove = keywords.clone();
	let list_remove = keywords_list;
	let refresh_remove = refresh_keywords;
	let edit_btn_remove = edit_keyword_button;
	let remove_btn_remove = remove_keyword_button;

	remove_keyword_button.on_click(move |_| {
		if let Some(sel) = list_remove.get_selection() {
			let idx = sel as usize;
			let mut visual_count = 0;
			let mut found = false;
			{
				let mut k = keywords_remove.borrow_mut();
				for entry in k.iter_mut() {
					if !entry.destroyed {
						if visual_count == idx {
							entry.destroyed = true;
							found = true;
							break;
						}
						visual_count += 1;
					}
				}
			}
			if found {
				refresh_remove();
				edit_btn_remove.enable(false);
				remove_btn_remove.enable(false);
			}
		}
	});

	dialog.centre();
	title_text.set_focus();
	if dialog.show_modal() != ID_OK {
		return None;
	}

	let title = title_text.get_value().trim().to_string();
	if title.is_empty() {
		return None;
	}

	let mut contexts = Vec::new();
	for (cb, ctx) in context_checks {
		if cb.get_value() {
			contexts.push(ctx);
		}
	}
	if contexts.is_empty() {
		contexts.push(FilterContext::Home);
	}

	let action = match action_choice.get_selection() {
		Some(1) => FilterAction::Hide,
		Some(2) => FilterAction::Blur,
		Some(3) => custom_action.map_or(FilterAction::Warn, FilterAction::Other),
		_ => FilterAction::Warn,
	};

	let expires_in = match expiry_choice.get_selection() {
		Some(1) => Some(30 * 60),
		Some(2) => Some(60 * 60),
		Some(3) => Some(6 * 60 * 60),
		Some(4) => Some(12 * 60 * 60),
		Some(5) => Some(24 * 60 * 60),
		Some(6) => Some(7 * 24 * 60 * 60),
		_ => None,
	};

	let final_keywords: Vec<(String, String, bool, bool)> =
		keywords.borrow().iter().map(|k| (k.id.clone(), k.keyword.clone(), k.whole_word, k.destroyed)).collect();

	Some(FilterDialogResult { title, contexts, action, keywords: final_keywords, expires_in })
}

#[derive(Clone)]
pub struct ProfileDialog {
	dialog: Dialog,
	relationship: Rc<RefCell<Option<crate::mastodon::Relationship>>>,
	profile_text: TextCtrl,
	account: Rc<RefCell<MastodonAccount>>,
}

impl ProfileDialog {
	pub fn new<F, C>(
		frame: &Frame,
		account: MastodonAccount,
		net_tx: std::sync::mpsc::Sender<NetworkCommand>,
		on_view_timeline: F,
		on_close: C,
	) -> Self
	where
		F: Fn() + 'static + Clone,
		C: Fn() + 'static + Clone,
	{
		let title = format!("Profile for {}", account.display_name_or_username());
		let dialog = Dialog::builder(frame, &title).with_size(500, 400).build();
		let panel = Panel::builder(&dialog).build();
		let main_sizer = BoxSizer::builder(Orientation::Vertical).build();
		let profile_text = TextCtrl::builder(&panel)
			.with_style(TextCtrlStyle::MultiLine | TextCtrlStyle::ReadOnly | TextCtrlStyle::DontWrap)
			.build();
		profile_text.set_value(&account.profile_display());
		let button_sizer = BoxSizer::builder(Orientation::Horizontal).build();
		let actions_button = Button::builder(&panel).with_label("Actions...").build();
		let timeline_button = Button::builder(&panel).with_id(ID_OK).with_label("View &Timeline").build();
		let close_button = Button::builder(&panel).with_id(ID_CANCEL).with_label("&Close").build();
		close_button.set_default();
		button_sizer.add(&actions_button, 0, SizerFlag::Right, 8);
		button_sizer.add(&timeline_button, 0, SizerFlag::Right, 8);
		button_sizer.add_stretch_spacer(1);
		button_sizer.add(&close_button, 0, SizerFlag::Right, 8);
		main_sizer.add(&profile_text, 1, SizerFlag::Expand | SizerFlag::All, 8);
		main_sizer.add_sizer(
			&button_sizer,
			0,
			SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Bottom,
			8,
		);
		panel.set_sizer(main_sizer, true);
		let dialog_sizer = BoxSizer::builder(Orientation::Vertical).build();
		dialog_sizer.add(&panel, 1, SizerFlag::Expand, 0);
		dialog.set_sizer(dialog_sizer, true);
		dialog.set_escape_id(ID_CANCEL);

		let relationship: Rc<RefCell<Option<crate::mastodon::Relationship>>> = Rc::new(RefCell::new(None));
		let account_rc = Rc::new(RefCell::new(account));
		let relationship_action = relationship.clone();
		let actions_btn = actions_button;

		actions_btn.on_click(move |_| {
			let mut menu = Menu::builder().build();
			{
				let rel = relationship_action.borrow();
				if let Some(r) = rel.as_ref() {
					if r.following {
						menu.append(ID_ACTION_UNFOLLOW, "Unfollow", "", ItemKind::Normal);
						if r.showing_reblogs {
							menu.append(ID_ACTION_HIDE_BOOSTS, "Hide Boosts", "", ItemKind::Normal);
						} else {
							menu.append(ID_ACTION_SHOW_BOOSTS, "Show Boosts", "", ItemKind::Normal);
						}
					} else {
						menu.append(ID_ACTION_FOLLOW, "Follow", "", ItemKind::Normal);
					}
					if r.muting {
						menu.append(ID_ACTION_UNMUTE, "Unmute", "", ItemKind::Normal);
					} else {
						menu.append(ID_ACTION_MUTE, "Mute", "", ItemKind::Normal);
					}
					if r.blocking {
						menu.append(ID_ACTION_UNBLOCK, "Unblock", "", ItemKind::Normal);
					} else {
						menu.append(ID_ACTION_BLOCK, "Block", "", ItemKind::Normal);
					}
					menu.append_separator();
				}
			}
			menu.append(ID_ACTION_OPEN_BROWSER, "Open in Browser", "", ItemKind::Normal);
			menu.append_separator();
			menu.append(ID_ACTION_VIEW_FOLLOWERS, "View Followers", "", ItemKind::Normal);
			menu.append(ID_ACTION_VIEW_FOLLOWING, "View Following", "", ItemKind::Normal);
			panel.popup_menu(&mut menu, None);
		});

		let account_handler = account_rc.clone();
		let panel_handler = panel;
		let net_tx_handler = net_tx;

		panel_handler.on_menu_selected(move |event| {
			let id = event.get_id();
			let account = account_handler.borrow();
			let account_id = account.id.clone();
			let target_name = account.display_name_or_username().to_string();

			if id == ID_ACTION_OPEN_BROWSER {
				let _ =
					wxdragon::utils::launch_default_browser(&account.url, wxdragon::utils::BrowserLaunchFlags::Default);
				return;
			}
			if id == ID_ACTION_VIEW_FOLLOWERS {
				let _ = net_tx_handler.send(NetworkCommand::FetchFollowers { account_id });
				return;
			}
			if id == ID_ACTION_VIEW_FOLLOWING {
				let _ = net_tx_handler.send(NetworkCommand::FetchFollowing { account_id });
				return;
			}

			let cmd = match id {
				ID_ACTION_FOLLOW => NetworkCommand::FollowAccount {
					account_id,
					target_name,
					reblogs: true,
					action: crate::network::RelationshipAction::Follow,
				},
				ID_ACTION_UNFOLLOW => NetworkCommand::UnfollowAccount { account_id, target_name },
				ID_ACTION_SHOW_BOOSTS => NetworkCommand::FollowAccount {
					account_id,
					target_name,
					reblogs: true,
					action: crate::network::RelationshipAction::ShowBoosts,
				},
				ID_ACTION_HIDE_BOOSTS => NetworkCommand::FollowAccount {
					account_id,
					target_name,
					reblogs: false,
					action: crate::network::RelationshipAction::HideBoosts,
				},
				ID_ACTION_BLOCK => {
					let confirm = MessageDialog::builder(
						&panel_handler,
						"Are you sure you want to block this user?",
						"Block User",
					)
					.with_style(MessageDialogStyle::YesNo | MessageDialogStyle::IconWarning)
					.build();
					if confirm.show_modal() != ID_YES {
						return;
					}
					NetworkCommand::BlockAccount { account_id, target_name }
				}
				ID_ACTION_UNBLOCK => NetworkCommand::UnblockAccount { account_id, target_name },
				ID_ACTION_MUTE => NetworkCommand::MuteAccount { account_id, target_name },
				ID_ACTION_UNMUTE => NetworkCommand::UnmuteAccount { account_id, target_name },
				_ => return,
			};
			let _ = net_tx_handler.send(cmd);
		});
		let dlg_timeline = dialog;
		let on_view_timeline = on_view_timeline;
		timeline_button.on_click(move |_| {
			on_view_timeline();
			dlg_timeline.close(true);
		});

		let dlg_close = dialog;
		close_button.on_click(move |_| {
			dlg_close.close(true);
		});

		let on_close_win = on_close;
		dialog.on_close(move |_| {
			on_close_win();
		});

		dialog.centre();
		Self { dialog, relationship, profile_text, account: account_rc }
	}

	pub fn show(&self) {
		self.dialog.show(true);
	}

	pub fn update_account(&self, account: &MastodonAccount) {
		self.account.replace(account.clone());
		self.dialog.set_label(&format!("Profile for {}", account.display_name_or_username()));

		let mut text = account.profile_display();

		if let Some(rel) = self.relationship.borrow().clone() {
			text.push_str("\r\n\r\nRelationship:\r\n");
			let follow_status = match (rel.following, rel.followed_by) {
				(true, true) => "You follow each other.",
				(true, false) => "You follow this person.",
				(false, true) => "This person follows you.",
				(false, false) => "You do not follow each other.",
			};
			let _ = writeln!(text, "{follow_status}");

			if rel.requested {
				text.push_str("You have requested to follow this person.\r\n");
			}
			if rel.blocking {
				text.push_str("You have blocked this person.\r\n");
			}
			if rel.muting {
				text.push_str("You have muted this person.\r\n");
			}
			if rel.domain_blocking {
				text.push_str("You have blocked this person's domain.\r\n");
			}

			if !rel.note.is_empty() {
				let note = crate::html::strip_html(&rel.note);
				if !note.trim().is_empty() {
					text.push_str("\r\nNote:\r\n");
					text.push_str(&note);
				}
			}
		}

		self.profile_text.set_value(&text);
	}

	pub fn update_relationship(&self, relationship: &crate::mastodon::Relationship) {
		*self.relationship.borrow_mut() = Some(relationship.clone());
		let account = self.account.borrow();
		let mut text = account.profile_display();
		text.push_str("\r\n\r\nRelationship:\r\n");

		let follow_status = match (relationship.following, relationship.followed_by) {
			(true, true) => "You follow each other.",
			(true, false) => "You follow this person.",
			(false, true) => "This person follows you.",
			(false, false) => "You do not follow each other.",
		};
		let _ = writeln!(text, "{follow_status}");

		if relationship.requested {
			text.push_str("You have requested to follow this person.\r\n");
		}
		if relationship.blocking {
			text.push_str("You have blocked this person.\r\n");
		}
		if relationship.muting {
			text.push_str("You have muted this person.\r\n");
		}
		if relationship.domain_blocking {
			text.push_str("You have blocked this person's domain.\r\n");
		}

		if !relationship.note.is_empty() {
			let note = crate::html::strip_html(&relationship.note);
			if !note.trim().is_empty() {
				text.push_str("\r\nNote:\r\n");
				text.push_str(&note);
			}
		}
		self.profile_text.set_value(&text);
	}
}

pub fn prompt_for_link_selection(frame: &Frame, links: &[Link]) -> Option<String> {
	let dialog = Dialog::builder(frame, "Select Link").with_size(500, 300).build();
	let panel = Panel::builder(&dialog).build();
	let main_sizer = BoxSizer::builder(Orientation::Vertical).build();
	let list_label = StaticText::builder(&panel).with_label("Links found in post:").build();
	let link_list = ListBox::builder(&panel).build();
	for link in links {
		link_list.append(&link.url);
	}
	if !links.is_empty() {
		link_list.set_selection(0, true);
	}
	let button_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	let open_button = Button::builder(&panel).with_id(ID_OK).with_label("Open").build();
	open_button.set_default();
	let copy_button = Button::builder(&panel).with_label("Copy").build();
	let close_button = Button::builder(&panel).with_id(ID_CANCEL).with_label("Close").build();
	button_sizer.add(&open_button, 0, SizerFlag::Right, 8);
	button_sizer.add(&copy_button, 0, SizerFlag::Right, 8);
	button_sizer.add_stretch_spacer(1);
	button_sizer.add(&close_button, 0, SizerFlag::Right, 8);
	main_sizer.add(&list_label, 0, SizerFlag::Expand | SizerFlag::All, 8);
	main_sizer.add(&link_list, 1, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
	main_sizer.add_sizer(&button_sizer, 0, SizerFlag::Expand | SizerFlag::All, 8);
	panel.set_sizer(main_sizer, true);
	let dialog_sizer = BoxSizer::builder(Orientation::Vertical).build();
	dialog_sizer.add(&panel, 1, SizerFlag::Expand, 0);
	dialog.set_sizer(dialog_sizer, true);
	dialog.set_affirmative_id(ID_OK);
	dialog.set_escape_id(ID_CANCEL);

	let links_copy = links.to_vec();
	let link_list_copy = link_list;
	let copy_action = move || {
		if let Some(sel) = link_list_copy.get_selection()
			&& let Some(link) = links_copy.get(sel as usize)
		{
			let clipboard = Clipboard::get();
			let _ = clipboard.set_text(&link.url);
		}
	};
	let copy_action_btn = copy_action.clone();
	copy_button.on_click(move |_| {
		copy_action_btn();
	});
	let copy_action_key = copy_action;
	link_list_copy.on_key_down(move |event| {
		if let WindowEventData::Keyboard(ref key_event) = event {
			if key_event.control_down() && key_event.get_key_code() == Some(67) {
				// C
				copy_action_key();
			} else {
				event.skip(true);
			}
		} else {
			event.skip(true);
		}
	});

	dialog.centre();
	let result = dialog.show_modal();
	if result != ID_OK {
		return None;
	}
	link_list.get_selection().and_then(|sel| links.get(sel as usize).map(|l| l.url.clone()))
}

pub fn prompt_for_mentions(
	frame: &Frame,
	mentions: &[crate::mastodon::Mention],
) -> Option<(Mention, UserLookupAction)> {
	const ID_VIEW_TIMELINE: i32 = 10041;
	let dialog = Dialog::builder(frame, "Mentions").with_size(500, 300).build();
	let panel = Panel::builder(&dialog).build();
	let main_sizer = BoxSizer::builder(Orientation::Vertical).build();
	let list_label = StaticText::builder(&panel).with_label("Users mentioned in post:").build();
	let mention_list = ListBox::builder(&panel).build();
	for mention in mentions {
		mention_list.append(&format!("@{}", mention.acct));
	}
	if !mentions.is_empty() {
		mention_list.set_selection(0, true);
	}
	let button_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	let open_button = Button::builder(&panel).with_id(ID_OK).with_label("View &Profile").build();
	open_button.set_default();
	let timeline_button = Button::builder(&panel).with_id(ID_VIEW_TIMELINE).with_label("View &Timeline").build();
	let close_button = Button::builder(&panel).with_id(ID_CANCEL).with_label("Close").build();
	button_sizer.add(&open_button, 0, SizerFlag::Right, 8);
	button_sizer.add(&timeline_button, 0, SizerFlag::Right, 8);
	button_sizer.add_stretch_spacer(1);
	button_sizer.add(&close_button, 0, SizerFlag::Right, 8);
	main_sizer.add(&list_label, 0, SizerFlag::Expand | SizerFlag::All, 8);
	main_sizer.add(&mention_list, 1, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
	main_sizer.add_sizer(&button_sizer, 0, SizerFlag::Expand | SizerFlag::All, 8);
	panel.set_sizer(main_sizer, true);
	let dialog_sizer = BoxSizer::builder(Orientation::Vertical).build();
	dialog_sizer.add(&panel, 1, SizerFlag::Expand, 0);
	dialog.set_sizer(dialog_sizer, true);
	dialog.set_affirmative_id(ID_OK);
	dialog.set_escape_id(ID_CANCEL);

	let dialog_timeline = dialog;
	timeline_button.on_click(move |_| {
		dialog_timeline.end_modal(ID_VIEW_TIMELINE);
	});

	dialog.centre();
	let result = dialog.show_modal();
	if result == ID_CANCEL {
		return None;
	}
	let mention = mention_list.get_selection().and_then(|sel| mentions.get(sel as usize).cloned())?;
	let action = if result == ID_VIEW_TIMELINE { UserLookupAction::Timeline } else { UserLookupAction::Profile };
	Some((mention, action))
}

pub fn prompt_for_account_list(
	frame: &Frame,
	title: &str,
	label: &str,
	accounts: &[MastodonAccount],
) -> Option<(MastodonAccount, UserLookupAction)> {
	const ID_VIEW_TIMELINE: i32 = 10043;
	let dialog = Dialog::builder(frame, title).with_size(500, 300).build();
	let panel = Panel::builder(&dialog).build();
	let main_sizer = BoxSizer::builder(Orientation::Vertical).build();
	let list_label = StaticText::builder(&panel).with_label(label).build();
	let account_list = ListBox::builder(&panel).build();
	for account in accounts {
		let name = account.display_name_or_username();
		let entry =
			if name.is_empty() { format!("@{}", account.acct) } else { format!("{} (@{})", name, account.acct) };
		account_list.append(&entry);
	}
	if !accounts.is_empty() {
		account_list.set_selection(0, true);
	}
	let button_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	let open_button = Button::builder(&panel).with_id(ID_OK).with_label("View &Profile").build();
	open_button.set_default();
	let timeline_button = Button::builder(&panel).with_id(ID_VIEW_TIMELINE).with_label("View &Timeline").build();
	let close_button = Button::builder(&panel).with_id(ID_CANCEL).with_label("Close").build();
	button_sizer.add(&open_button, 0, SizerFlag::Right, 8);
	button_sizer.add(&timeline_button, 0, SizerFlag::Right, 8);
	button_sizer.add_stretch_spacer(1);
	button_sizer.add(&close_button, 0, SizerFlag::Right, 8);
	main_sizer.add(&list_label, 0, SizerFlag::Expand | SizerFlag::All, 8);
	main_sizer.add(&account_list, 1, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
	main_sizer.add_sizer(&button_sizer, 0, SizerFlag::Expand | SizerFlag::All, 8);
	panel.set_sizer(main_sizer, true);
	let dialog_sizer = BoxSizer::builder(Orientation::Vertical).build();
	dialog_sizer.add(&panel, 1, SizerFlag::Expand, 0);
	dialog.set_sizer(dialog_sizer, true);
	dialog.set_affirmative_id(ID_OK);
	dialog.set_escape_id(ID_CANCEL);
	let dialog_timeline = dialog;
	timeline_button.on_click(move |_| {
		dialog_timeline.end_modal(ID_VIEW_TIMELINE);
	});
	dialog.centre();
	let result = dialog.show_modal();
	if result == ID_CANCEL {
		return None;
	}
	let account = account_list.get_selection().and_then(|sel| accounts.get(sel as usize).cloned())?;
	let action = if result == ID_VIEW_TIMELINE { UserLookupAction::Timeline } else { UserLookupAction::Profile };
	Some((account, action))
}

pub fn prompt_for_follow_list(
	frame: &Frame,
	title: &str,
	label: &str,
	accounts: &[MastodonAccount],
) -> Option<MastodonAccount> {
	const ID_VIEW_TIMELINE: i32 = 10044;
	let dialog = Dialog::builder(frame, title).with_size(600, 400).build();
	let panel = Panel::builder(&dialog).build();
	let main_sizer = BoxSizer::builder(Orientation::Vertical).build();
	let list_label = StaticText::builder(&panel).with_label(label).build();
	let account_list = ListBox::builder(&panel).build();
	let profile_text = TextCtrl::builder(&panel)
		.with_style(TextCtrlStyle::MultiLine | TextCtrlStyle::ReadOnly | TextCtrlStyle::DontWrap)
		.build();
	for account in accounts {
		let name = account.display_name_or_username();
		let entry =
			if name.is_empty() { format!("@{}", account.acct) } else { format!("{} (@{})", name, account.acct) };
		account_list.append(&entry);
	}
	if let Some(first) = accounts.first() {
		account_list.set_selection(0, true);
		profile_text.set_value(&first.profile_display());
	}
	let button_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	let timeline_button = Button::builder(&panel).with_id(ID_VIEW_TIMELINE).with_label("View &Timeline").build();
	let close_button = Button::builder(&panel).with_id(ID_CANCEL).with_label("Close").build();
	button_sizer.add(&timeline_button, 0, SizerFlag::Right, 8);
	button_sizer.add_stretch_spacer(1);
	button_sizer.add(&close_button, 0, SizerFlag::Right, 8);
	main_sizer.add(&list_label, 0, SizerFlag::Expand | SizerFlag::All, 8);
	main_sizer.add(&account_list, 1, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
	main_sizer.add(&profile_text, 1, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
	main_sizer.add_sizer(&button_sizer, 0, SizerFlag::Expand | SizerFlag::All, 8);
	panel.set_sizer(main_sizer, true);
	let dialog_sizer = BoxSizer::builder(Orientation::Vertical).build();
	dialog_sizer.add(&panel, 1, SizerFlag::Expand, 0);
	dialog.set_sizer(dialog_sizer, true);
	dialog.set_escape_id(ID_CANCEL);
	let accounts_rc: Rc<RefCell<Vec<MastodonAccount>>> = Rc::new(RefCell::new(accounts.to_vec()));
	let list_sel = account_list;
	let text_sel = profile_text;
	let accounts_sel = accounts_rc.clone();
	list_sel.on_selection_changed(move |_| {
		let selection = list_sel.get_selection().map(|sel| sel as usize);
		if let Some(index) = selection
			&& let Some(account) = accounts_sel.borrow().get(index)
		{
			text_sel.set_value(&account.profile_display());
		}
	});
	let dialog_timeline = dialog;
	timeline_button.on_click(move |_| {
		dialog_timeline.end_modal(ID_VIEW_TIMELINE);
	});
	dialog.centre();
	let result = dialog.show_modal();
	if result != ID_VIEW_TIMELINE {
		return None;
	}
	account_list.get_selection().and_then(|sel| accounts_rc.borrow().get(sel as usize).cloned())
}

pub fn prompt_for_account_selection(
	frame: &Frame,
	accounts: &[&MastodonAccount],
	labels: &[&str],
) -> Option<(MastodonAccount, UserLookupAction)> {
	const ID_VIEW_TIMELINE: i32 = 10042;
	let dialog = Dialog::builder(frame, "Select User").with_size(400, 150).build();
	let panel = Panel::builder(&dialog).build();
	let main_sizer = BoxSizer::builder(Orientation::Vertical).build();
	let list_label = StaticText::builder(&panel).with_label("User:").build();
	let choices: Vec<String> = labels.iter().map(std::string::ToString::to_string).collect();
	let combo = ComboBox::builder(&panel).with_choices(choices).with_style(ComboBoxStyle::ReadOnly).build();
	combo.set_selection(0);
	let button_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	let profile_button = Button::builder(&panel).with_id(ID_OK).with_label("View &Profile").build();
	profile_button.set_default();
	let timeline_button = Button::builder(&panel).with_id(ID_VIEW_TIMELINE).with_label("View &Timeline").build();
	let cancel_button = Button::builder(&panel).with_id(ID_CANCEL).with_label("Cancel").build();
	button_sizer.add(&profile_button, 0, SizerFlag::Right, 8);
	button_sizer.add(&timeline_button, 0, SizerFlag::Right, 8);
	button_sizer.add_stretch_spacer(1);
	button_sizer.add(&cancel_button, 0, SizerFlag::Right, 8);
	main_sizer.add(&list_label, 0, SizerFlag::Expand | SizerFlag::All, 8);
	main_sizer.add(&combo, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
	main_sizer.add_sizer(&button_sizer, 0, SizerFlag::Expand | SizerFlag::All, 8);
	panel.set_sizer(main_sizer, true);
	let dialog_sizer = BoxSizer::builder(Orientation::Vertical).build();
	dialog_sizer.add(&panel, 1, SizerFlag::Expand, 0);
	dialog.set_sizer(dialog_sizer, true);
	dialog.set_affirmative_id(ID_OK);
	dialog.set_escape_id(ID_CANCEL);
	let dialog_timeline = dialog;
	timeline_button.on_click(move |_| {
		dialog_timeline.end_modal(ID_VIEW_TIMELINE);
	});
	dialog.centre();
	let result = dialog.show_modal();
	if result == ID_CANCEL {
		return None;
	}
	let account = combo.get_selection().and_then(|sel| accounts.get(sel as usize).copied()).cloned()?;
	let action = if result == ID_VIEW_TIMELINE { UserLookupAction::Timeline } else { UserLookupAction::Profile };
	Some((account, action))
}

pub fn prompt_for_account_choice(
	frame: &Frame,
	accounts: &[&MastodonAccount],
	labels: &[&str],
) -> Option<MastodonAccount> {
	let dialog = Dialog::builder(frame, "Select User").with_size(400, 150).build();
	let panel = Panel::builder(&dialog).build();
	let main_sizer = BoxSizer::builder(Orientation::Vertical).build();
	let list_label = StaticText::builder(&panel).with_label("User:").build();
	let choices: Vec<String> = labels.iter().map(std::string::ToString::to_string).collect();
	let combo = ComboBox::builder(&panel).with_choices(choices).with_style(ComboBoxStyle::ReadOnly).build();
	combo.set_selection(0);
	let button_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	let ok_button = Button::builder(&panel).with_id(ID_OK).with_label("OK").build();
	ok_button.set_default();
	let cancel_button = Button::builder(&panel).with_id(ID_CANCEL).with_label("Cancel").build();
	button_sizer.add_stretch_spacer(1);
	button_sizer.add(&ok_button, 0, SizerFlag::Right, 8);
	button_sizer.add(&cancel_button, 0, SizerFlag::Right, 8);
	main_sizer.add(&list_label, 0, SizerFlag::Expand | SizerFlag::All, 8);
	main_sizer.add(&combo, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
	main_sizer.add_sizer(&button_sizer, 0, SizerFlag::Expand | SizerFlag::All, 8);
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
	combo.get_selection().and_then(|sel| accounts.get(sel as usize).copied()).cloned()
}

#[derive(Clone)]
pub struct HashtagDialog {
	dialog: Dialog,
	list: ListBox,
	action_button: Button,
	tags: Rc<RefCell<Vec<crate::mastodon::Tag>>>,
}

impl HashtagDialog {
	pub fn new<F>(frame: &Frame, tags: Vec<Tag>, net_tx: Sender<NetworkCommand>, on_close: F) -> Self
	where
		F: Fn() + 'static + Clone,
	{
		let dialog = Dialog::builder(frame, "Hashtags").with_size(500, 300).build();
		let panel = Panel::builder(&dialog).build();
		let main_sizer = BoxSizer::builder(Orientation::Vertical).build();
		let list_label = StaticText::builder(&panel).with_label("Hashtags in post:").build();
		let tag_list = ListBox::builder(&panel).build();
		let format_tag = |tag: &crate::mastodon::Tag| -> String {
			let status = if tag.following { " (Following)" } else { "" };
			format!("#{}{}", tag.name, status)
		};
		for tag in &tags {
			tag_list.append(&format_tag(tag));
		}
		if !tags.is_empty() {
			tag_list.set_selection(0, true);
		}
		let button_sizer = BoxSizer::builder(Orientation::Horizontal).build();
		let action_button = Button::builder(&panel).with_label("Follow").build();
		let close_button = Button::builder(&panel).with_id(ID_CANCEL).with_label("Close").build();
		close_button.set_default();
		button_sizer.add(&action_button, 0, SizerFlag::Right, 8);
		button_sizer.add_stretch_spacer(1);
		button_sizer.add(&close_button, 0, SizerFlag::Right, 8);
		main_sizer.add(&list_label, 0, SizerFlag::Expand | SizerFlag::All, 8);
		main_sizer.add(&tag_list, 1, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
		main_sizer.add_sizer(&button_sizer, 0, SizerFlag::Expand | SizerFlag::All, 8);
		panel.set_sizer(main_sizer, true);
		let dialog_sizer = BoxSizer::builder(Orientation::Vertical).build();
		dialog_sizer.add(&panel, 1, SizerFlag::Expand, 0);
		dialog.set_sizer(dialog_sizer, true);
		let tags_rc = Rc::new(RefCell::new(tags));
		let handle = Self { dialog, list: tag_list, action_button, tags: tags_rc.clone() };
		let update_button_state = {
			let tags = tags_rc.clone();
			let btn = action_button;
			let list = tag_list;
			move || {
				if let Some(sel) = list.get_selection() {
					if let Some(tag) = tags.borrow().get(sel as usize) {
						btn.enable(true);
						if tag.following {
							btn.set_label("Unfollow");
						} else {
							btn.set_label("Follow");
						}
					} else {
						btn.enable(false);
					}
				} else {
					btn.enable(false);
				}
			}
		};
		update_button_state();
		let update_on_sel = update_button_state;
		tag_list.on_selection_changed(move |_| {
			update_on_sel();
		});
		let tags_action = tags_rc;
		let list_action = tag_list;
		let net_tx_action = net_tx;
		action_button.on_click(move |_| {
			if let Some(sel) = list_action.get_selection() {
				let index = sel as usize;
				let tags_borrow = tags_action.borrow();
				if let Some(tag) = tags_borrow.get(index) {
					let cmd = if tag.following {
						NetworkCommand::UnfollowTag { name: tag.name.clone() }
					} else {
						NetworkCommand::FollowTag { name: tag.name.clone() }
					};
					let _ = net_tx_action.send(cmd);
				}
			}
		});
		let dlg = dialog;
		close_button.on_click(move |_| {
			dlg.close(true);
		});
		let on_close_win = on_close;
		dialog.on_close(move |_| {
			on_close_win();
		});
		handle
	}

	pub fn show(&self) {
		self.dialog.show(true);
	}

	pub fn update_tag(&self, name: &str, following: bool) {
		let mut tags = self.tags.borrow_mut();
		let mut index = None;
		for (i, tag) in tags.iter_mut().enumerate() {
			if tag.name.eq_ignore_ascii_case(name) {
				tag.following = following;
				index = Some(i);
			}
		}
		if let Some(i) = index {
			let format_tag = |tag: &crate::mastodon::Tag| -> String {
				let status = if tag.following { " (Following)" } else { "" };
				format!("#{} {}", tag.name, status)
			};
			let sel = self.list.get_selection();
			self.list.clear();
			for t in tags.iter() {
				self.list.append(&format_tag(t));
			}
			if let Some(s) = sel {
				self.list.set_selection(s, true);
			}
			if let Ok(i_u32) = u32::try_from(i)
				&& sel == Some(i_u32)
			{
				if following {
					self.action_button.set_label("Unfollow");
				} else {
					self.action_button.set_label("Follow");
				}
			}
		}
	}
}

pub fn prompt_for_profile_edit(frame: &Frame, current: &MastodonAccount) -> Option<ProfileUpdate> {
	let dialog = Dialog::builder(frame, "Edit Profile").with_size(600, 600).build();
	let panel = Panel::builder(&dialog).build();
	let main_sizer = BoxSizer::builder(Orientation::Vertical).build();
	let scroll_win = ScrolledWindow::builder(&panel).build();
	scroll_win.set_scroll_rate(0, 10);
	let content_sizer = BoxSizer::builder(Orientation::Vertical).build();
	let name_label = StaticText::builder(&scroll_win).with_label("Display Name").build();
	let name_text = TextCtrl::builder(&scroll_win).with_value(current.display_name_or_username()).build();
	name_text.set_name("Display Name");
	content_sizer.add(&name_label, 0, SizerFlag::All, 5);
	content_sizer.add(&name_text, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 5);
	let note_label = StaticText::builder(&scroll_win).with_label("Bio").build();
	let note_text = TextCtrl::builder(&scroll_win)
		.with_value(&crate::html::strip_html(&current.note))
		.with_style(TextCtrlStyle::MultiLine)
		.with_size(Size::new(-1, 100))
		.build();
	note_text.set_name("Bio");
	content_sizer.add(&note_label, 0, SizerFlag::All, 5);
	content_sizer.add(&note_text, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 5);
	let images_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	let avatar_sizer = BoxSizer::builder(Orientation::Vertical).build();
	let avatar_label = StaticText::builder(&scroll_win).with_label("Avatar:").build();
	let avatar_path = TextCtrl::builder(&scroll_win).with_style(TextCtrlStyle::ReadOnly).build();
	avatar_path.set_name("Avatar Path");
	let avatar_btn = Button::builder(&scroll_win).with_label("Change Avatar...").build();
	avatar_sizer.add(&avatar_label, 0, SizerFlag::All, 5);
	avatar_sizer.add(&avatar_path, 0, SizerFlag::Expand | SizerFlag::All, 5);
	avatar_sizer.add(&avatar_btn, 0, SizerFlag::All, 5);
	let header_sizer = BoxSizer::builder(Orientation::Vertical).build();
	let header_label = StaticText::builder(&scroll_win).with_label("Header:").build();
	let header_path = TextCtrl::builder(&scroll_win).with_style(TextCtrlStyle::ReadOnly).build();
	header_path.set_name("Header Path");
	let header_btn = Button::builder(&scroll_win).with_label("Change Header...").build();
	header_sizer.add(&header_label, 0, SizerFlag::All, 5);
	header_sizer.add(&header_path, 0, SizerFlag::Expand | SizerFlag::All, 5);
	header_sizer.add(&header_btn, 0, SizerFlag::All, 5);
	images_sizer.add_sizer(&avatar_sizer, 1, SizerFlag::Expand, 0);
	images_sizer.add_sizer(&header_sizer, 1, SizerFlag::Expand, 0);
	content_sizer.add_sizer(&images_sizer, 0, SizerFlag::Expand, 0);
	let locked_cb = CheckBox::builder(&scroll_win).with_label("Require &follow approval").build();
	locked_cb.set_value(current.locked);
	content_sizer.add(&locked_cb, 0, SizerFlag::All, 5);
	let bot_cb = CheckBox::builder(&scroll_win).with_label("&Bot account").build();
	bot_cb.set_value(current.bot);
	content_sizer.add(&bot_cb, 0, SizerFlag::All, 5);
	let discoverable_cb = CheckBox::builder(&scroll_win).with_label("&Discoverable in directory").build();
	discoverable_cb.set_value(current.discoverable.unwrap_or(false));
	content_sizer.add(&discoverable_cb, 0, SizerFlag::All, 5);
	let mut field_controls = Vec::new();
	for i in 0..4 {
		let row_sizer = BoxSizer::builder(Orientation::Horizontal).build();
		let (name_val, val_val) = if i < current.fields.len() {
			(current.fields[i].name.clone(), html::strip_html(&current.fields[i].value))
		} else {
			(String::new(), String::new())
		};
		let title_lbl = format!("Field {} label", i + 1);
		let content_lbl = format!("Field {} content", i + 1);
		let field_sizer = BoxSizer::builder(Orientation::Vertical).build();
		let title_text = StaticText::builder(&scroll_win).with_label(&title_lbl).build();
		let name_ctrl = TextCtrl::builder(&scroll_win).with_value(&name_val).build();
		name_ctrl.set_name(&title_lbl);
		field_sizer.add(&title_text, 0, SizerFlag::All, 2);
		field_sizer.add(&name_ctrl, 0, SizerFlag::Expand | SizerFlag::All, 2);
		let content_sizer_inner = BoxSizer::builder(Orientation::Vertical).build();
		let content_text = StaticText::builder(&scroll_win).with_label(&content_lbl).build();
		let val_ctrl = TextCtrl::builder(&scroll_win).with_value(&val_val).build();
		val_ctrl.set_name(&content_lbl);
		content_sizer_inner.add(&content_text, 0, SizerFlag::All, 2);
		content_sizer_inner.add(&val_ctrl, 0, SizerFlag::Expand | SizerFlag::All, 2);
		row_sizer.add_sizer(&field_sizer, 1, SizerFlag::Expand, 0);
		row_sizer.add_sizer(&content_sizer_inner, 2, SizerFlag::Expand, 0);
		content_sizer.add_sizer(&row_sizer, 0, SizerFlag::Expand | SizerFlag::All, 5);
		field_controls.push((name_ctrl, val_ctrl));
	}
	let (privacy_choice_opt, sensitive_cb_opt, lang_text_opt) =
		current.source.as_ref().map_or((None, None, None), |source| {
			let privacy_sizer = BoxSizer::builder(Orientation::Horizontal).build();
			let privacy_label = StaticText::builder(&scroll_win).with_label("Default post visibility").build();
			let privacy_choices: Vec<String> =
				vec!["Public".to_string(), "Unlisted".to_string(), "Followers only".to_string()];
			let privacy_choice = Choice::builder(&scroll_win).with_choices(privacy_choices).build();
			let sel = match source.privacy.as_deref() {
				Some("unlisted") => 1,
				Some("private") => 2,
				_ => 0,
			};
			privacy_choice.set_selection(sel);
			privacy_sizer.add(&privacy_label, 0, SizerFlag::AlignCenterVertical | SizerFlag::Right, 5);
			privacy_sizer.add(&privacy_choice, 1, SizerFlag::Expand, 0);
			content_sizer.add_sizer(&privacy_sizer, 0, SizerFlag::Expand | SizerFlag::All, 5);
			let sensitive_cb = CheckBox::builder(&scroll_win).with_label("&Mark media as sensitive by default").build();
			sensitive_cb.set_value(source.sensitive.unwrap_or(false));
			content_sizer.add(&sensitive_cb, 0, SizerFlag::All, 5);
			let lang_sizer = BoxSizer::builder(Orientation::Horizontal).build();
			let lang_label = StaticText::builder(&scroll_win).with_label("Language (ISO code):").build();
			let lang_text = TextCtrl::builder(&scroll_win).with_value(source.language.as_deref().unwrap_or("")).build();
			lang_sizer.add(&lang_label, 0, SizerFlag::AlignCenterVertical | SizerFlag::Right, 5);
			lang_sizer.add(&lang_text, 1, SizerFlag::Expand, 0);
			content_sizer.add_sizer(&lang_sizer, 0, SizerFlag::Expand | SizerFlag::All, 5);
			(Some(privacy_choice), Some(sensitive_cb), Some(lang_text))
		});
	scroll_win.set_sizer(content_sizer, true);
	main_sizer.add(&scroll_win, 1, SizerFlag::Expand, 0);
	let btn_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	let ok_btn = Button::builder(&panel).with_id(ID_OK).with_label("Save Changes").build();
	ok_btn.set_default();
	let cancel_btn = Button::builder(&panel).with_id(ID_CANCEL).with_label("Cancel").build();
	btn_sizer.add_stretch_spacer(1);
	btn_sizer.add(&ok_btn, 0, SizerFlag::Right, 8);
	btn_sizer.add(&cancel_btn, 0, SizerFlag::Right, 8);
	main_sizer.add_sizer(&btn_sizer, 0, SizerFlag::Expand | SizerFlag::All, 10);
	panel.set_sizer(main_sizer, true);
	let dlg_sizer = BoxSizer::builder(Orientation::Vertical).build();
	dlg_sizer.add(&panel, 1, SizerFlag::Expand, 0);
	dialog.set_sizer(dlg_sizer, true);
	dialog.centre();
	let avatar_path_ref = avatar_path;
	let panel_ref = panel;
	avatar_btn.on_click(move |_| {
		let dlg = FileDialog::builder(&panel_ref)
			.with_message("Select Avatar")
			.with_wildcard("Image files|*.png;*.jpg;*.jpeg;*.gif")
			.with_style(FileDialogStyle::Open | FileDialogStyle::FileMustExist)
			.build();
		if dlg.show_modal() == ID_OK
			&& let Some(path) = dlg.get_path()
		{
			avatar_path_ref.set_value(&path);
		}
	});
	let header_path_ref = header_path;
	let panel_ref = panel;
	header_btn.on_click(move |_| {
		let dlg = FileDialog::builder(&panel_ref)
			.with_message("Select Header")
			.with_wildcard("Image files|*.png;*.jpg;*.jpeg;*.gif")
			.with_style(FileDialogStyle::Open | FileDialogStyle::FileMustExist)
			.build();
		if dlg.show_modal() == ID_OK
			&& let Some(path) = dlg.get_path()
		{
			header_path_ref.set_value(&path);
		}
	});
	if dialog.show_modal() != ID_OK {
		return None;
	}
	let display_name = name_text.get_value();
	let note = note_text.get_value();
	let avatar = avatar_path.get_value();
	let header = header_path.get_value();
	let locked = locked_cb.get_value();
	let bot = bot_cb.get_value();
	let discoverable = discoverable_cb.get_value();
	let mut fields_attributes = Vec::new();
	for (name_ctrl, val_ctrl) in &field_controls {
		let name = name_ctrl.get_value();
		let val = val_ctrl.get_value();
		// Always send all fields to preserve indices (0..3) so the server knows which to update/clear
		fields_attributes.push((name, val));
	}
	let source = if let (Some(privacy_choice), Some(sensitive_cb), Some(lang_text)) =
		(privacy_choice_opt, sensitive_cb_opt, lang_text_opt)
	{
		let privacy = match privacy_choice.get_selection() {
			Some(1) => "unlisted",
			Some(2) => "private",
			_ => "public",
		}
		.to_string();
		Some(crate::mastodon::Source {
			privacy: Some(privacy),
			sensitive: Some(sensitive_cb.get_value()),
			language: Some(lang_text.get_value()),
		})
	} else {
		None
	};
	Some(ProfileUpdate {
		display_name: Some(display_name),
		note: Some(note),
		avatar: if avatar.is_empty() { None } else { Some(avatar) },
		header: if header.is_empty() { None } else { Some(header) },
		locked: Some(locked),
		bot: Some(bot),
		discoverable: Some(discoverable),
		fields_attributes: Some(fields_attributes),
		source,
	})
}

pub fn show_update_dialog(parent: &dyn WxWidget, new_version: &str, changelog: &str) -> bool {
	let padding = 10;
	let dialog_title = format!("Update to {new_version}");
	let dialog = Dialog::builder(parent, &dialog_title).build();
	let panel = Panel::builder(&dialog).build();

	let message =
		StaticText::builder(&panel).with_label("A new version of Fedra is available. Here's what's new:").build();
	let changelog_ctrl = TextCtrl::builder(&panel)
		.with_value(changelog)
		.with_style(TextCtrlStyle::MultiLine | TextCtrlStyle::ReadOnly | TextCtrlStyle::Rich2)
		.with_size(Size::new(500, 300))
		.build();
	let yes_button = Button::builder(&panel).with_id(ID_OK).with_label("Yes").build();
	let no_button = Button::builder(&panel).with_id(ID_CANCEL).with_label("No").build();
	dialog.set_escape_id(ID_CANCEL);
	dialog.set_affirmative_id(ID_OK);

	let content_sizer = BoxSizer::builder(Orientation::Vertical).build();
	content_sizer.add(&message, 0, SizerFlag::All, padding);
	content_sizer.add(
		&changelog_ctrl,
		1,
		SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Bottom,
		padding,
	);

	let button_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	button_sizer.add_stretch_spacer(1);
	button_sizer.add(&yes_button, 0, SizerFlag::Right, padding);
	button_sizer.add(&no_button, 0, SizerFlag::Right, padding);

	content_sizer.add_sizer(&button_sizer, 0, SizerFlag::Expand | SizerFlag::All, 0);
	panel.set_sizer(content_sizer, true);

	let dialog_sizer = BoxSizer::builder(Orientation::Vertical).build();
	dialog_sizer.add(&panel, 1, SizerFlag::Expand, 0);
	dialog.set_sizer_and_fit(dialog_sizer, true);

	dialog.centre();
	dialog.raise();
	changelog_ctrl.set_focus();
	dialog.show_modal() == ID_OK
}

pub fn show_post_view_dialog(parent: &Frame, status: &Status) -> Option<UiCommand> {
	let title = format!("Post by {}", status.account.display_name_or_username());
	let dialog = Dialog::builder(parent, &title).with_size(600, 500).build();
	let panel = Panel::builder(&dialog).build();
	let sizer = BoxSizer::builder(Orientation::Vertical).build();
	let web_view = WebView::builder(&panel).build();
	web_view.add_script_message_handler("wx");
	let dialog_close_msg = dialog;
	web_view.on_script_message_received(move |event: WebViewEventData| {
		if let Some(msg) = event.get_string() {
			if msg == "close_dialog" {
				dialog_close_msg.end_modal(ID_CANCEL);
			} else if let Some(url) = msg.strip_prefix("open_link:") {
				let _ = wxdragon::utils::launch_default_browser(url, wxdragon::utils::BrowserLaunchFlags::Default);
			}
		}
	});

	let content = if status.spoiler_text.is_empty() {
		status.content.clone()
	} else {
		format!("<p><strong>Content Warning: {}</strong></p><hr>{}", status.spoiler_text, status.content)
	};

	let html = format!(
		"<html>
		<head>
			<title>{}</title>
			<style>
				body {{ font-family: sans-serif; padding: 10px; }}
				img {{ max-width: 100%; height: auto; }}
				video {{ max-width: 100%; height: auto; }}
			</style>
		</head>
		<body>
			<h2>{} <small>({})</small></h2>
			{}
		</body>
		</html>",
		title,
		status.account.display_name_or_username(),
		status.account.acct,
		content
	);

	web_view.set_page(&html, "");

	let web_view_for_load = web_view;
	let timer = Rc::new(Timer::new(&dialog));
	let timer_copy = Rc::clone(&timer);
	web_view.on_loaded(move |_| {
		let web_view_for_timer = web_view_for_load;
		timer_copy.on_tick(move |_| {
			let pos = web_view_for_timer.client_to_screen(Point::new(0, 0));
			let size = web_view_for_timer.get_size();
			let x = pos.x + size.width / 2;
			let y = pos.y + size.height / 2;
			let sim = UIActionSimulator::new();
			sim.mouse_move(x, y);
			sim.mouse_click(MouseButton::Left);
		});
		timer_copy.start(100, true);
		web_view_for_load.run_script(
			"function addEvent(elem, event, handler) { \
				if (elem.addEventListener) { \
					elem.addEventListener(event, handler, false); \
				} else if (elem.attachEvent) { \
					elem.attachEvent('on' + event, handler); \
				} \
			} \
			addEvent(document, 'keydown', function(event) { \
				if (event.key === 'Escape' || event.keyCode === 27) { \
					window.wx.postMessage('close_dialog'); \
				} \
			}); \
			addEvent(document, 'click', function(event) { \
				event = event || window.event; \
				var target = event.target || event.srcElement; \
				while (target && target.tagName !== 'A') { target = target.parentNode; } \
				if (target && target.tagName === 'A' && target.href) { \
					if (event.preventDefault) event.preventDefault(); \
					else event.returnValue = false; \
					window.wx.postMessage('open_link:' + target.href); \
				} \
			});",
		);
	});
	let button_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	let reply_btn = Button::builder(&panel).with_id(ID_REPLY).with_label("Reply").build();
	let boost_btn = Button::builder(&panel)
		.with_id(ID_BOOST)
		.with_label(if status.reblogged { "Unboost" } else { "Boost" })
		.build();
	let fav_btn = Button::builder(&panel)
		.with_id(ID_FAVORITE)
		.with_label(if status.favourited { "Unfavorite" } else { "Favorite" })
		.build();
	let close_btn = Button::builder(&panel).with_id(ID_CANCEL).with_label("Close").build();
	close_btn.set_default();
	button_sizer.add(&reply_btn, 0, SizerFlag::All, 5);
	button_sizer.add(&boost_btn, 0, SizerFlag::All, 5);
	button_sizer.add(&fav_btn, 0, SizerFlag::All, 5);
	button_sizer.add_stretch_spacer(1);
	button_sizer.add(&close_btn, 0, SizerFlag::All, 5);
	sizer.add(&web_view, 1, SizerFlag::Expand | SizerFlag::All, 5);
	sizer.add_sizer(&button_sizer, 0, SizerFlag::Expand | SizerFlag::All, 5);
	panel.set_sizer(sizer, true);
	let dialog_sizer = BoxSizer::builder(Orientation::Vertical).build();
	dialog_sizer.add(&panel, 1, SizerFlag::Expand, 0);
	dialog.set_sizer(dialog_sizer, true);
	dialog.set_escape_id(ID_CANCEL);
	dialog.centre();
	let dialog_reply = dialog;
	reply_btn.on_click(move |_| {
		dialog_reply.end_modal(ID_REPLY);
	});
	let dialog_boost = dialog;
	boost_btn.on_click(move |_| {
		dialog_boost.end_modal(ID_BOOST);
	});
	let dialog_fav = dialog;
	fav_btn.on_click(move |_| {
		dialog_fav.end_modal(ID_FAVORITE);
	});
	let dialog_close = dialog;
	close_btn.on_click(move |_| {
		dialog_close.end_modal(ID_CANCEL);
	});
	let result = dialog.show_modal();
	match result {
		ID_REPLY => Some(UiCommand::Reply { reply_all: true }),
		ID_BOOST => Some(UiCommand::Boost),
		ID_FAVORITE => Some(UiCommand::Favorite),
		_ => None,
	}
}

pub fn prompt_for_find(parent: &dyn WxWidget) -> Option<String> {
	let dialog = Dialog::builder(parent, "Find").with_size(350, 150).build();
	let panel = Panel::builder(&dialog).build();
	let main_sizer = BoxSizer::builder(Orientation::Vertical).build();

	let label = StaticText::builder(&panel).with_label("Find text in timeline:").build();
	let input = TextCtrl::builder(&panel).with_style(TextCtrlStyle::ProcessEnter).build();

	let button_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	let find_button = Button::builder(&panel).with_id(ID_OK).with_label("Find").build();
	find_button.set_default();
	let cancel_button = Button::builder(&panel).with_id(ID_CANCEL).with_label("Cancel").build();

	button_sizer.add_stretch_spacer(1);
	button_sizer.add(&find_button, 0, SizerFlag::Right, 8);
	button_sizer.add(&cancel_button, 0, SizerFlag::Right, 8);

	main_sizer.add(&label, 0, SizerFlag::Expand | SizerFlag::All, 8);
	main_sizer.add(&input, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
	main_sizer.add_sizer(&button_sizer, 0, SizerFlag::Expand | SizerFlag::All, 8);

	panel.set_sizer(main_sizer, true);
	let dialog_sizer = BoxSizer::builder(Orientation::Vertical).build();
	dialog_sizer.add(&panel, 1, SizerFlag::Expand, 0);
	dialog.set_sizer(dialog_sizer, true);

	dialog.set_affirmative_id(ID_OK);
	dialog.set_escape_id(ID_CANCEL);

	input.on_key_down(move |event| {
		if let WindowEventData::Keyboard(ref key_event) = event {
			if key_event.get_key_code() == Some(KEY_RETURN) && !key_event.shift_down() && !key_event.control_down() {
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
	input.set_focus();

	if dialog.show_modal() == ID_OK {
		let text = input.get_value();
		if !text.is_empty() {
			return Some(text);
		}
	}
	None
}
