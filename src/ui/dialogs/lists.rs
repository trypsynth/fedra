use std::{cell::RefCell, rc::Rc, sync::mpsc::Sender};

use wxdragon::prelude::*;

use crate::{ui::dialogs::prompt_for_account_search, network::NetworkCommand};

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

	pub fn get_dialog(&self) -> &Dialog {
		&self.dialog
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
		parent: &dyn WxWidget,
		list_id: String,
		list_title: &str,
		members: Vec<crate::mastodon::Account>,
		net_tx: Sender<NetworkCommand>,
		on_close: F,
	) -> Self
	where
		F: Fn() + 'static + Clone,
	{
		let dialog = Dialog::builder(parent, &format!("Manage Members: {list_title}")).with_size(450, 400).build();
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

	pub fn get_dialog(&self) -> &Dialog {
		&self.dialog
	}
}
