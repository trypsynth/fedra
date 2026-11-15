use std::{cell::RefCell, rc::Rc};

use wxdragon::prelude::*;

use super::prompt_for_account_search;
use crate::{
	mastodon::{Account, SearchType},
	responses::{lists, search},
	ui::commands::Requests,
};
#[derive(Clone)]
pub struct ManageListMembersDialog {
	dialog: Dialog,
	members_list: ListBox,
	remove_button: Button,
	members: Rc<RefCell<Vec<Account>>>,
	list_id: String,
}

impl ManageListMembersDialog {
	pub fn new<F>(
		parent: &dyn WxWidget,
		list_id: String,
		list_title: &str,
		members: Vec<Account>,
		requests: Requests,
		on_close: F,
	) -> Self
	where
		F: Fn() + 'static + Clone,
	{
		let dialog = Dialog::builder(parent, &format!("Manage Members: {list_title}")).with_size(450, 400).build();
		let panel = Panel::builder(&dialog).build();
		let main_sizer = BoxSizer::builder(Orientation::Vertical).build();
		let members_label = StaticText::builder(&panel).with_label("&Members:").build();
		let members_list = ListBox::builder(&panel).build();
		let buttons_sizer = BoxSizer::builder(Orientation::Horizontal).build();
		let add_button = Button::builder(&panel).with_label("&Add Member...").build();
		let remove_button = Button::builder(&panel).with_label("&Remove Member").build();
		let close_button = Button::builder(&panel).with_id(ID_CANCEL).with_label("&Close").build();
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
		let requests_search = requests.clone();
		add_button.on_click(move |_| {
			if let Some(query) = prompt_for_account_search(&dialog_add) {
				search(&requests_search, query, SearchType::Accounts, Some(20), None);
			}
		});
		let members_remove = handle.members.clone();
		let list_remove = handle.members_list;
		let list_id_remove = list_id;
		remove_button.on_click(move |_| {
			let member_id = list_remove
				.get_selection()
				.and_then(|sel| members_remove.borrow().get(sel as usize).map(|m| m.id.clone()));
			if let Some(member_id) = member_id {
				lists::remove_member(&requests, list_id_remove.clone(), member_id);
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

	pub fn destroy(&self) {
		self.dialog.destroy();
	}

	pub fn update_members(&self, new_members: Vec<crate::mastodon::Account>) {
		*self.members.borrow_mut() = new_members;
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

	pub const fn get_dialog(&self) -> &Dialog {
		&self.dialog
	}
}
