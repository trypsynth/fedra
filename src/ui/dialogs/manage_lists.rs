use std::{cell::RefCell, rc::Rc, sync::mpsc::Sender};

use wxdragon::prelude::*;

use crate::{network::NetworkCommand, ui::dialogs::show_list_edit_dialog};

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
		let lists_label = StaticText::builder(&panel).with_label("&Lists:").build();
		let lists_ctrl = ListBox::builder(&panel).build();
		let buttons_sizer = BoxSizer::builder(Orientation::Horizontal).build();
		let add_button = Button::builder(&panel).with_label("&Add...").build();
		let edit_button = Button::builder(&panel).with_label("&Edit...").build();
		let members_button = Button::builder(&panel).with_label("&Members...").build();
		let remove_button = Button::builder(&panel).with_label("&Delete").build();
		let close_button = Button::builder(&panel).with_id(ID_CANCEL).with_label("&Close").build();
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
			if let Some((title, policy, exclusive)) = show_list_edit_dialog(&dialog_add, None, None, false) {
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
					&& let Some((title, policy, exclusive)) = show_list_edit_dialog(
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

	pub fn get_dialog(&self) -> &Dialog {
		&self.dialog
	}
}
