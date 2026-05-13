use std::{cell::RefCell, rc::Rc};

use wxdragon::prelude::*;

use crate::mastodon::Account;

pub fn show_follow_list_dialog(frame: &Frame, title: &str, label: &str, accounts: &[Account]) -> Option<Account> {
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
	let accounts_rc: Rc<RefCell<Vec<Account>>> = Rc::new(RefCell::new(accounts.to_vec()));
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
