use wxdragon::{
	event::{WebViewEventData, WebViewEvents},
	prelude::*,
	widgets::WebView,
};

use crate::{
	commands::UiCommand,
	html::{self, Link},
	mastodon::{Account as MastodonAccount, Mention, SearchType, Status},
	ui::ids::{ID_BOOST, ID_FAVORITE, ID_REPLY},
};

pub(crate) const KEY_RETURN: i32 = 13;

const KEY_RETURN: i32 = 13;

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

pub fn show_error(parent: &dyn WxWidget, err: &anyhow::Error) {
	let dialog = MessageDialog::builder(parent, &err.to_string(), "Fedra")
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
	parent: &dyn WxWidget,
	accounts: &[&MastodonAccount],
	labels: &[&str],
) -> Option<MastodonAccount> {
	let dialog = Dialog::builder(parent, "Select User").with_size(400, 150).build();
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
	web_view.on_loaded(move |_| {
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
