use std::{cell::RefCell, rc::Rc};

use wxdragon::prelude::*;

use crate::{
	html::Link,
	mastodon::{Account as MastodonAccount, SearchType},
};

pub(crate) const KEY_RETURN: i32 = 13;

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

pub fn show_warning_widget(parent: &dyn WxWidget, message: &str, title: &str) {
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
