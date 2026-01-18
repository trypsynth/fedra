use std::{cell::RefCell, path::Path, rc::Rc};

use url::Url;
use wxdragon::prelude::*;

use crate::{
	error,
	mastodon::{PollLimits, Status},
};

pub fn parse_instance_url(value: &str) -> Option<Url> {
	let trimmed = value.trim();
	if trimmed.is_empty() {
		return None;
	}
	let candidate = if trimmed.contains("://") { trimmed.to_string() } else { format!("https://{}", trimmed) };
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
	pub fn as_api_str(&self) -> &'static str {
		match self {
			Self::Public => "public",
			Self::Unlisted => "unlisted",
			Self::Private => "private",
			Self::Direct => "direct",
		}
	}

	fn display_name(&self) -> &'static str {
		match self {
			Self::Public => "Public",
			Self::Unlisted => "Unlisted",
			Self::Private => "Followers only",
			Self::Direct => "Mentioned only",
		}
	}

	fn all() -> &'static [PostVisibility] {
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
}

#[derive(Debug, Clone)]
pub struct PostPoll {
	pub options: Vec<String>,
	pub expires_in: u32,
	pub multiple: bool,
}

const DEFAULT_MAX_POST_CHARS: usize = 500;
const KEY_RETURN: i32 = 13;

fn refresh_media_list(media_list: &ListBox, items: &[PostMedia]) {
	media_list.clear();
	for item in items {
		let label = Path::new(&item.path).file_name().and_then(|name| name.to_str()).unwrap_or(&item.path);
		media_list.append(label);
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
	let ok_button = Button::builder(&panel).with_label("Done").build();
	let cancel_button = Button::builder(&panel).with_label("Cancel").build();
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

	let options: Rc<RefCell<Vec<String>>> =
		Rc::new(RefCell::new(existing.as_ref().map(|poll| poll.options.clone()).unwrap_or_default()));
	refresh_poll_list(&poll_list, &options.borrow());
	if !options.borrow().is_empty() {
		poll_list.set_selection(0, true);
		remove_button.enable(true);
		option_label.enable(true);
		option_text.enable(true);
		if let Some(first) = options.borrow().first() {
			option_text.set_value(first);
		}
	} else {
		remove_button.enable(false);
		option_label.enable(false);
		option_text.enable(false);
	}

	let default_minutes = existing.as_ref().map(|poll| poll.expires_in / 60).unwrap_or(min_minutes as u32) as i32;
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
			let items = options_select.borrow();
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
			let mut items = options_edit.borrow_mut();
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
	ok_button.on_click(move |_| {
		dialog.end_modal(ID_OK);
	});
	cancel_button.on_click(move |_| {
		dialog.end_modal(ID_CANCEL);
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
	let ok_button = Button::builder(&panel).with_label("Done").build();
	let cancel_button = Button::builder(&panel).with_label("Cancel").build();
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

	let items: Rc<RefCell<Vec<PostMedia>>> = Rc::new(RefCell::new(initial));
	refresh_media_list(&media_list, &items.borrow());
	if !items.borrow().is_empty() {
		media_list.set_selection(0, true);
		remove_button.enable(true);
		desc_label.enable(true);
		desc_text.enable(true);
		if let Some(first) = items.borrow().first() {
			desc_text.set_value(first.description.as_deref().unwrap_or(""));
		}
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
			if paths.is_empty() {
				if let Some(path) = file_dialog.get_path() {
					paths.push(path);
				}
			}
			if !paths.is_empty() {
				let mut items = items_add.borrow_mut();
				for path in paths {
					items.push(PostMedia { path, description: None });
				}
				refresh_media_list(&media_list_add, &items);
				media_list_add.set_selection((items.len() - 1) as u32, true);
				remove_button_add.enable(true);
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
			let mut items = items_remove.borrow_mut();
			if index < items.len() {
				items.remove(index);
				refresh_media_list(&media_list_remove, &items);
				if !items.is_empty() {
					let next = index.min(items.len() - 1);
					media_list_remove.set_selection(next as u32, true);
					remove_button_remove.enable(true);
				} else {
					remove_button_remove.enable(false);
				}
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
		let items = items_select.borrow();
		if let Some(index) = selection
			&& index < items.len()
		{
			desc_label_select.enable(true);
			desc_text_select.enable(true);
			desc_text_select.set_value(items[index].description.as_deref().unwrap_or(""));
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

	ok_button.on_click(move |_| {
		dialog.end_modal(ID_OK);
	});
	cancel_button.on_click(move |_| {
		dialog.end_modal(ID_CANCEL);
	});
	dialog.centre();
	let result = dialog.show_modal();
	if result != ID_OK {
		return None;
	}
	Some(items.borrow().clone())
}

pub fn prompt_for_post(frame: &Frame, max_chars: Option<usize>, poll_limits: &PollLimits) -> Option<PostResult> {
	let max_chars = max_chars.unwrap_or(DEFAULT_MAX_POST_CHARS);
	let dialog = Dialog::builder(frame, &format!("Post - 0 of {} characters", max_chars)).with_size(700, 560).build();
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
	let content_type_options = vec![
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
	visibility_choice.set_selection(0); // Default to Public
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
	let ok_button = Button::builder(&panel).with_label("Post").build();
	let cancel_button = Button::builder(&panel).with_label("Cancel").build();
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
	let media_items: Rc<RefCell<Vec<PostMedia>>> = Rc::new(RefCell::new(Vec::new()));
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
				format!("{} items attached.", count)
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
					format!("Poll with {} options.", option_count)
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
	// Enter sends, Shift+Enter or Ctrl+Enter inserts newline
	content_text.on_key_down(move |event| {
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
	let timer = Timer::new(&dialog);
	timer.on_tick(move |_| {
		let text = content_text.get_value();
		let char_count = text.chars().count();
		dialog.set_label(&format!("Post - {} of {} characters", char_count, max_chars));
	});
	timer.start(100, false);
	ok_button.on_click(move |_| {
		dialog.end_modal(ID_OK);
	});
	cancel_button.on_click(move |_| {
		dialog.end_modal(ID_CANCEL);
	});
	dialog.centre();
	content_text.set_focus();
	let result = dialog.show_modal();
	timer.stop();
	if result != ID_OK {
		return None;
	}
	let content = content_text.get_value();
	let trimmed = content.trim();
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

pub struct ReplyResult {
	pub content: String,
	pub visibility: PostVisibility,
	pub spoiler_text: Option<String>,
}

pub fn prompt_for_reply(frame: &Frame, replying_to: &Status, max_chars: Option<usize>) -> Option<ReplyResult> {
	let max_chars = max_chars.unwrap_or(DEFAULT_MAX_POST_CHARS);
	let author = replying_to.account.display_name_or_username();
	let dialog = Dialog::builder(frame, &format!("Reply to {} - 0 of {} characters", author, max_chars))
		.with_size(600, 400)
		.build();
	let panel = Panel::builder(&dialog).build();
	let main_sizer = BoxSizer::builder(Orientation::Vertical).build();
	// Show original post content
	let original_label = StaticText::builder(&panel).with_label("Replying to:").build();
	let original_content = replying_to.reblog.as_ref().map(|r| r.content.as_str()).unwrap_or(&replying_to.content);
	let original_text = strip_html(original_content);
	let preview = if original_text.len() > 200 { format!("{}...", &original_text[..200]) } else { original_text };
	let original_preview = StaticText::builder(&panel).with_label(&format!("{}: {}", author, preview)).build();
	let content_label = StaticText::builder(&panel).with_label("Your reply:").build();
	let content_text = TextCtrl::builder(&panel).with_style(TextCtrlStyle::MultiLine).build();
	// Pre-fill with @mention
	let mention = format!("@{} ", replying_to.account.acct);
	content_text.set_value(&mention);
	let cw_checkbox = CheckBox::builder(&panel).with_label("Content warning").build();
	let cw_label = StaticText::builder(&panel).with_label("Warning text:").build();
	let cw_text = TextCtrl::builder(&panel).build();
	cw_label.show(false);
	cw_text.show(false);
	// If original has CW, pre-fill it
	if !replying_to.spoiler_text.is_empty() {
		cw_checkbox.set_value(true);
		cw_label.show(true);
		cw_text.show(true);
		cw_text.set_value(&format!("re: {}", replying_to.spoiler_text));
	}
	let visibility_label = StaticText::builder(&panel).with_label("Visibility:").build();
	let visibility_choices: Vec<String> = PostVisibility::all().iter().map(|v| v.display_name().to_string()).collect();
	let visibility_choice = Choice::builder(&panel).with_choices(visibility_choices).build();
	// Match original visibility by default
	let default_visibility = match replying_to.visibility.as_str() {
		"public" => 0,
		"unlisted" => 1,
		"private" => 2,
		"direct" => 3,
		_ => 0,
	};
	visibility_choice.set_selection(default_visibility);
	let visibility_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	visibility_sizer.add(&visibility_label, 0, SizerFlag::AlignCenterVertical | SizerFlag::Right, 8);
	visibility_sizer.add(&visibility_choice, 1, SizerFlag::Expand, 0);
	let button_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	let ok_button = Button::builder(&panel).with_label("Reply").build();
	let cancel_button = Button::builder(&panel).with_label("Cancel").build();
	button_sizer.add_stretch_spacer(1);
	button_sizer.add(&ok_button, 0, SizerFlag::Right, 8);
	button_sizer.add(&cancel_button, 0, SizerFlag::Right, 8);
	main_sizer.add(&original_label, 0, SizerFlag::Expand | SizerFlag::All, 8);
	main_sizer.add(&original_preview, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
	main_sizer.add(&content_label, 0, SizerFlag::Expand | SizerFlag::All, 8);
	main_sizer.add(&content_text, 1, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
	main_sizer.add(&cw_checkbox, 0, SizerFlag::Expand | SizerFlag::All, 8);
	main_sizer.add(&cw_label, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
	main_sizer.add(&cw_text, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
	main_sizer.add_sizer(&visibility_sizer, 0, SizerFlag::Expand | SizerFlag::All, 8);
	main_sizer.add_sizer(&button_sizer, 0, SizerFlag::Expand | SizerFlag::All, 8);
	panel.set_sizer(main_sizer, true);
	let dialog_sizer = BoxSizer::builder(Orientation::Vertical).build();
	dialog_sizer.add(&panel, 1, SizerFlag::Expand, 0);
	dialog.set_sizer(dialog_sizer, true);
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
	// Enter sends, Shift+Enter or Ctrl+Enter inserts newline
	content_text.on_key_down(move |event| {
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
	let timer = Timer::new(&dialog);
	let author_timer = author.to_string();
	timer.on_tick(move |_| {
		let text = content_text.get_value();
		let char_count = text.chars().count();
		dialog.set_label(&format!("Reply to {} - {} of {} characters", author_timer, char_count, max_chars));
	});
	timer.start(100, false);
	ok_button.on_click(move |_| {
		dialog.end_modal(ID_OK);
	});
	cancel_button.on_click(move |_| {
		dialog.end_modal(ID_CANCEL);
	});
	dialog.centre();
	content_text.set_focus();
	// Move cursor to end of pre-filled mention
	content_text.set_insertion_point_end();
	let result = dialog.show_modal();
	timer.stop();
	if result != ID_OK {
		return None;
	}
	let content = content_text.get_value();
	let trimmed = content.trim();
	if trimmed.is_empty() {
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
	Some(ReplyResult { content: trimmed.to_string(), visibility, spoiler_text })
}

fn strip_html(html: &str) -> String {
	let fragment = scraper::Html::parse_fragment(html);
	let mut parts = Vec::new();
	for text in fragment.root_element().text() {
		let trimmed = text.trim();
		if !trimmed.is_empty() {
			parts.push(trimmed);
		}
	}
	let mut output = String::new();
	for part in parts {
		if output.is_empty() {
			output.push_str(part);
			continue;
		}
		if output.ends_with('@') {
			output.push_str(part);
		} else {
			output.push(' ');
			output.push_str(part);
		}
	}
	output
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

pub fn show_error(frame: &Frame, err: &anyhow::Error) {
	let dialog = MessageDialog::builder(frame, error::user_message(err), "Fedra")
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
