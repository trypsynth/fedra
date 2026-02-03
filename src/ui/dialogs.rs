use std::{cell::RefCell, path::Path, rc::Rc, sync::mpsc::Sender};

use url::Url;
use wxdragon::prelude::*;

use crate::{
	config::{Account, AutoloadMode, ContentWarningDisplay, SortOrder, TimestampFormat},
	html::{self, Link},
	mastodon::{Account as MastodonAccount, Mention, PollLimits, SearchType, Status, Tag},
	network::{NetworkCommand, ProfileUpdate},
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
	pub const fn as_api_str(&self) -> &'static str {
		match self {
			Self::Public => "public",
			Self::Unlisted => "unlisted",
			Self::Private => "private",
			Self::Direct => "direct",
		}
	}

	const fn display_name(&self) -> &'static str {
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
	pub media: Vec<PostMedia>,
	pub poll: Option<PostPoll>,
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
}

const DEFAULT_MAX_POST_CHARS: usize = 500;
const KEY_RETURN: i32 = 13;

struct ComposeDialogConfig {
	title_prefix: String,
	ok_label: String,
	initial_content: String,
	initial_cw: Option<String>,
	default_visibility: PostVisibility,
}

const fn visibility_index(visibility: PostVisibility) -> usize {
	match visibility {
		PostVisibility::Public => 0,
		PostVisibility::Unlisted => 1,
		PostVisibility::Private => 2,
		PostVisibility::Direct => 3,
	}
}

fn refresh_media_list(media_list: &ListBox, items: &[PostMedia]) {
	media_list.clear();
	for item in items {
		let label = if item.is_existing {
			if let Some(desc) = &item.description { format!("Existing: {desc}") } else { "Existing Media".to_string() }
		} else {
			Path::new(&item.path).file_name().and_then(|name| name.to_str()).unwrap_or(&item.path).to_string()
		};
		media_list.append(&label);
	}
}

fn refresh_poll_list(poll_list: &ListBox, items: &[String]) {
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

fn prompt_for_poll(parent: &dyn WxWidget, existing: Option<PostPoll>, limits: &PollLimits) -> Option<PollDialogResult> {
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
	let duration_label = StaticText::builder(&panel)
		.with_label(&format!(
			"Duration in minutes (min {}, max {}):",
			limits.min_expiration / 60,
			limits.max_expiration / 60
		))
		.build();
	let min_minutes = (limits.min_expiration / 60).max(1) as i32;
	let max_minutes = (limits.max_expiration / 60).max(min_minutes as u32) as i32;
	let duration_spin = SpinCtrl::builder(&panel).with_range(min_minutes, max_minutes).build();
	let multiple_checkbox = CheckBox::builder(&panel).with_label("Allow multiple selections").build();
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
	main_sizer.add(&duration_spin, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
	main_sizer.add(&multiple_checkbox, 0, SizerFlag::Expand | SizerFlag::All, 8);
	main_sizer.add_sizer(&buttons_sizer, 0, SizerFlag::Expand | SizerFlag::All, 8);
	panel.set_sizer(main_sizer, true);
	let dialog_sizer = BoxSizer::builder(Orientation::Vertical).build();
	dialog_sizer.add(&panel, 1, SizerFlag::Expand, 0);
	dialog.set_sizer(dialog_sizer, true);
	dialog.set_affirmative_id(ID_OK);
	dialog.set_escape_id(ID_CANCEL);
	let options: Rc<RefCell<Vec<String>>> =
		Rc::new(RefCell::new(existing.as_ref().map(|poll| poll.options.clone()).unwrap_or_default()));
	refresh_poll_list(&poll_list, &options.borrow());
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
	let default_minutes = existing.as_ref().map_or(min_minutes as u32, |poll| poll.expires_in / 60) as i32;
	duration_spin.set_value(default_minutes.clamp(min_minutes, max_minutes));
	if let Some(existing) = existing.as_ref() {
		multiple_checkbox.set_value(existing.multiple);
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
		refresh_poll_list(&poll_list_add, &items_snapshot);
		poll_list_add.set_selection((new_len - 1) as u32, true);
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
			refresh_poll_list(&poll_list_remove, &items_snapshot);
			if items_snapshot.is_empty() {
				remove_button_remove.enable(false);
				option_text_remove.set_value("");
				option_text_remove.enable(false);
				option_label_remove.enable(false);
			} else {
				let next = index.min(items_snapshot.len() - 1);
				poll_list_remove.set_selection(next as u32, true);
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
			let items = match options_select.try_borrow() {
				Ok(items) => items,
				Err(_) => return,
			};
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
			let mut items = match options_edit.try_borrow_mut() {
				Ok(items) => items,
				Err(_) => return,
			};
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
			refresh_poll_list(&poll_list_edit, &items_snapshot);
			poll_list_edit.set_selection(index as u32, true);
		}
	});
	const ID_REMOVE_POLL: i32 = 20_001;
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
	let minutes = duration_spin.value().max(min_minutes) as u32;
	let expires_in = minutes.saturating_mul(60);
	if expires_in < limits.min_expiration || expires_in > limits.max_expiration {
		show_warning_widget(parent, "Poll duration is outside this instance's limits.", "Poll");
		return None;
	}
	Some(PollDialogResult::Updated(PostPoll { options, expires_in, multiple: multiple_checkbox.get_value() }))
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
	refresh_media_list(&media_list, &items.borrow());
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
	add_button.on_click(move |_| {
		let file_dialog = FileDialog::builder(&panel)
			.with_message("Select media to attach")
			.with_wildcard("Media files|*.png;*.jpg;*.jpeg;*.gif;*.mp4;*.webm;*.mov|All files|*.*")
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
					refresh_media_list(&media_list_add, &items);
					items.len()
				};
				if new_len > 0 {
					media_list_add.set_selection((new_len - 1) as u32, true);
					remove_button_add.enable(true);
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
				refresh_media_list(&media_list_remove, &items);
				(items.len(), index.min(items.len().saturating_sub(1)))
			};
			if items_len > 0 {
				media_list_remove.set_selection(next_index as u32, true);
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
		let total_votes = poll.votes_count.max(1) as f32;
		let results_sizer = BoxSizer::builder(Orientation::Vertical).build();
		for option in &poll.options {
			let votes = option.votes_count.unwrap_or(0);
			let percent = (votes as f32 / total_votes * 100.0) as i32;
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

pub fn prompt_for_options(
	frame: &Frame,
	enter_to_send: bool,
	always_show_link_dialog: bool,
	quick_action_keys: bool,
	autoload: AutoloadMode,
	fetch_limit: u8,
	content_warning_display: ContentWarningDisplay,
	sort_order: SortOrder,
	timestamp_format: TimestampFormat,
) -> Option<(bool, bool, bool, AutoloadMode, u8, ContentWarningDisplay, SortOrder, TimestampFormat)> {
	let dialog = Dialog::builder(frame, "Options").with_size(400, 400).build();
	let panel = Panel::builder(&dialog).build();
	let main_sizer = BoxSizer::builder(Orientation::Vertical).build();

	// Create notebook for tabs
	let notebook = Notebook::builder(&panel).build();

	// === General Tab ===
	let general_panel = Panel::builder(&notebook).with_style(PanelStyle::TabTraversal).build();
	let general_sizer = BoxSizer::builder(Orientation::Vertical).build();

	let enter_checkbox = CheckBox::builder(&general_panel).with_label("Use &enter to send posts").build();
	enter_checkbox.set_value(enter_to_send);
	let link_checkbox = CheckBox::builder(&general_panel).with_label("Always prompt to open &links").build();
	link_checkbox.set_value(always_show_link_dialog);
	let quick_action_checkbox =
		CheckBox::builder(&general_panel).with_label("Use &quick action keys in timelines").build();
	quick_action_checkbox.set_value(quick_action_keys);

	general_sizer.add(&enter_checkbox, 0, SizerFlag::Expand | SizerFlag::All, 8);
	general_sizer.add(&link_checkbox, 0, SizerFlag::Expand | SizerFlag::All, 8);
	general_sizer.add(&quick_action_checkbox, 0, SizerFlag::Expand | SizerFlag::All, 8);
	general_sizer.add_stretch_spacer(1);
	general_panel.set_sizer(general_sizer, true);
	notebook.add_page(&general_panel, "General", true, None);

	// === Timeline Tab ===
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

	let timestamp_checkbox = CheckBox::builder(&timeline_panel).with_label("Show relative &timestamps").build();
	timestamp_checkbox.set_value(timestamp_format == TimestampFormat::Relative);
	let sort_checkbox = CheckBox::builder(&timeline_panel).with_label("Show oldest timeline entries &first").build();
	sort_checkbox.set_value(sort_order == SortOrder::OldestToNewest);

	timeline_sizer.add_sizer(&autoload_sizer, 0, SizerFlag::Expand | SizerFlag::All, 8);
	timeline_sizer.add_sizer(&fetch_limit_sizer, 0, SizerFlag::Expand | SizerFlag::All, 8);
	timeline_sizer.add_sizer(&cw_sizer, 0, SizerFlag::Expand | SizerFlag::All, 8);
	timeline_sizer.add(&timestamp_checkbox, 0, SizerFlag::Expand | SizerFlag::All, 8);
	timeline_sizer.add(&sort_checkbox, 0, SizerFlag::Expand | SizerFlag::All, 8);
	timeline_sizer.add_stretch_spacer(1);
	timeline_panel.set_sizer(timeline_sizer, true);
	notebook.add_page(&timeline_panel, "Timeline", false, None);

	// === Buttons ===
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
	let new_timestamp =
		if timestamp_checkbox.get_value() { TimestampFormat::Relative } else { TimestampFormat::Absolute };
	let new_cw_display = match cw_choice.get_selection() {
		Some(0) => ContentWarningDisplay::Inline,
		Some(1) => ContentWarningDisplay::Hidden,
		Some(2) => ContentWarningDisplay::WarningOnly,
		_ => content_warning_display,
	};
	let new_autoload = match autoload_choice.get_selection() {
		Some(0) => AutoloadMode::Never,
		Some(1) => AutoloadMode::AtEnd,
		Some(2) => AutoloadMode::AtBoundary,
		_ => autoload,
	};
	let new_fetch_limit = (fetch_limit_spin.value() as u8).clamp(1, 40);

	Some((
		enter_checkbox.get_value(),
		link_checkbox.get_value(),
		quick_action_checkbox.get_value(),
		new_autoload,
		new_fetch_limit,
		new_cw_display,
		new_sort,
		new_timestamp,
	))
}

fn prompt_for_compose(
	frame: &Frame,
	max_chars: Option<usize>,
	poll_limits: &PollLimits,
	enter_to_send: bool,
	config: ComposeDialogConfig,
	initial_media: Vec<PostMedia>,
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
	visibility_choice.set_selection(visibility_index(default_visibility) as u32);
	let visibility_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	visibility_sizer.add(&visibility_label, 0, SizerFlag::AlignCenterVertical | SizerFlag::Right, 8);
	visibility_sizer.add(&visibility_choice, 1, SizerFlag::Expand, 0);
	let content_type_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	content_type_sizer.add(&content_type_label, 0, SizerFlag::AlignCenterVertical | SizerFlag::Right, 8);
	content_type_sizer.add(&content_type_choice, 1, SizerFlag::Expand, 0);
	let media_label = StaticText::builder(&panel).with_label("Media:").build();
	let media_button = Button::builder(&panel).with_label("Manage Media...").build();
	let media_count_label = StaticText::builder(&panel).with_label("No media attached.").build();
	let poll_label = StaticText::builder(&panel).with_label("Poll:").build();
	let poll_button = Button::builder(&panel).with_label("Add Poll...").build();
	let poll_summary_label = StaticText::builder(&panel).with_label("No poll attached.").build();
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
	main_sizer.add(&media_label, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Top, 8);
	main_sizer.add(&media_button, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
	main_sizer.add(&media_count_label, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
	main_sizer.add(&poll_label, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Top, 8);
	main_sizer.add(&poll_button, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
	main_sizer.add(&poll_summary_label, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
	main_sizer.add_sizer(&button_sizer, 0, SizerFlag::Expand | SizerFlag::All, 8);
	panel.set_sizer(main_sizer, true);
	let dialog_sizer = BoxSizer::builder(Orientation::Vertical).build();
	dialog_sizer.add(&panel, 1, SizerFlag::Expand, 0);
	dialog.set_sizer(dialog_sizer, true);
	dialog.set_affirmative_id(ID_OK);
	dialog.set_escape_id(ID_CANCEL);
	let media_items: Rc<RefCell<Vec<PostMedia>>> = Rc::new(RefCell::new(initial_media));
	{
		let count = media_items.borrow().len();
		let label = if count == 0 {
			"No media attached.".to_string()
		} else if count == 1 {
			"1 item attached.".to_string()
		} else {
			format!("{count} items attached.")
		};
		media_count_label.set_label(&label);
	}
	let media_items_manage = media_items.clone();
	let media_count_update = media_count_label;
	let media_parent = dialog;
	media_button.on_click(move |_| {
		let current = media_items_manage.borrow().clone();
		if let Some(updated) = prompt_for_media(&media_parent, current) {
			let count = updated.len();
			*media_items_manage.borrow_mut() = updated;
			let label = if count == 0 {
				"No media attached.".to_string()
			} else if count == 1 {
				"1 item attached.".to_string()
			} else {
				format!("{count} items attached.")
			};
			media_count_update.set_label(&label);
		}
	});
	let poll_state: Rc<RefCell<Option<PostPoll>>> = Rc::new(RefCell::new(None));
	let poll_state_manage = poll_state.clone();
	let poll_summary_update = poll_summary_label;
	let poll_button_update = poll_button;
	let poll_parent = dialog;
	let poll_limits = poll_limits.clone();
	poll_button_update.on_click(move |_| {
		let current = poll_state_manage.borrow().clone();
		match prompt_for_poll(&poll_parent, current, &poll_limits) {
			Some(PollDialogResult::Updated(poll)) => {
				let option_count = poll.options.len();
				*poll_state_manage.borrow_mut() = Some(poll);
				poll_button_update.set_label("Edit Poll...");
				let label = if option_count == 1 {
					"Poll with 1 option.".to_string()
				} else {
					format!("Poll with {option_count} options.")
				};
				poll_summary_update.set_label(&label);
			}
			Some(PollDialogResult::Removed) => {
				*poll_state_manage.borrow_mut() = None;
				poll_button_update.set_label("Add Poll...");
				poll_summary_update.set_label("No poll attached.");
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
	let timer = Timer::new(&dialog);
	let title_prefix_timer = title_prefix.clone();
	timer.on_tick(move |_| {
		let text = content_text.get_value();
		let char_count = text.chars().count();
		dialog.set_label(&format!("{title_prefix_timer} - {char_count} of {max_chars} characters"));
	});
	timer.start(100, false);
	dialog.centre();
	content_text.set_focus();
	if !initial_content.is_empty() {
		content_text.set_insertion_point_end();
	}
	let result = dialog.show_modal();
	timer.stop();
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
	let media = media_items.borrow().clone();
	let poll = poll_state.borrow().clone();
	if trimmed.is_empty() && media.is_empty() && poll.is_none() {
		return None;
	}
	Some(PostResult { content: trimmed.to_string(), visibility, spoiler_text, content_type, media, poll })
}

pub fn prompt_for_post(
	frame: &Frame,
	max_chars: Option<usize>,
	poll_limits: &PollLimits,
	enter_to_send: bool,
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
			default_visibility: PostVisibility::Public,
		},
		Vec::new(),
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
		"public" => PostVisibility::Public,
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
			default_visibility,
		},
		Vec::new(),
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
		"public" => PostVisibility::Public,
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
			default_visibility,
		},
		initial_media,
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
	let combo = ComboBox::builder(&panel).with_choices(suggestions.to_vec()).build();
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
		Some(0) => SearchType::All,
		Some(1) => SearchType::Accounts,
		Some(2) => SearchType::Hashtags,
		Some(3) => SearchType::Statuses,
		_ => SearchType::All,
	};
	Some((trimmed.to_string(), search_type))
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
	if let Some(index) = active_index {
		accounts_list.set_selection(index as u32, true);
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
		accounts_list.set_selection(idx as u32, true);
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

		const ID_ACTION_FOLLOW: i32 = 6001;
		const ID_ACTION_UNFOLLOW: i32 = 6002;
		const ID_ACTION_BLOCK: i32 = 6003;
		const ID_ACTION_UNBLOCK: i32 = 6004;
		const ID_ACTION_MUTE: i32 = 6005;
		const ID_ACTION_UNMUTE: i32 = 6006;
		const ID_ACTION_OPEN_BROWSER: i32 = 6007;
		const ID_ACTION_SHOW_BOOSTS: i32 = 6008;
		const ID_ACTION_HIDE_BOOSTS: i32 = 6009;

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

	pub fn update_account(&self, account: MastodonAccount) {
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
			text.push_str(&format!("{follow_status}\r\n"));

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

	pub fn update_relationship(&self, relationship: crate::mastodon::Relationship) {
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
		text.push_str(&format!("{follow_status}\r\n"));

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
			if sel == Some(i as u32) {
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
	let mut privacy_choice_opt: Option<Choice> = None;
	let mut sensitive_cb_opt: Option<CheckBox> = None;
	let mut lang_text_opt: Option<TextCtrl> = None;
	if let Some(source) = &current.source {
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
		privacy_choice_opt = Some(privacy_choice);
		sensitive_cb_opt = Some(sensitive_cb);
		lang_text_opt = Some(lang_text);
	}
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
			Some(0) => "public",
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
