use std::{cell::RefCell, rc::Rc};

use url::Url;
use wxdragon::prelude::*;

use crate::config::Account;

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
	(*result.borrow()).clone()
}
