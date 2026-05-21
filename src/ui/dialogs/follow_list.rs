use std::{cell::RefCell, rc::Rc, sync::mpsc::Sender};

use wxdragon::prelude::*;

use crate::{
	mastodon::{Account, Relationship},
	network::NetworkCommand,
};

use super::user_actions;

pub struct FollowListDialog {
	dialog: Dialog,
	token: u64,
	relationship: Rc<RefCell<Option<Relationship>>>,
	profile_text: TextCtrl,
	accounts: Rc<RefCell<Vec<Account>>>,
	account_list: ListBox,
	current_account: Rc<RefCell<Account>>,
}

impl FollowListDialog {
	pub fn new<F, C>(
		frame: &Frame,
		title: &str,
		label: &str,
		accounts: &[Account],
		net_tx: Sender<NetworkCommand>,
		token: u64,
		on_view_timeline: F,
		on_close: C,
	) -> Option<Self>
	where
		F: Fn(Account) + 'static,
		C: Fn() + 'static,
	{
		if accounts.is_empty() {
			return None;
		}
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

		let accounts_rc: Rc<RefCell<Vec<Account>>> = Rc::new(RefCell::new(accounts.to_vec()));
		let relationship: Rc<RefCell<Option<Relationship>>> = Rc::new(RefCell::new(None));
		let current_account_rc: Rc<RefCell<Account>> = Rc::new(RefCell::new(accounts[0].clone()));

		account_list.set_selection(0, true);
		profile_text.set_value(&accounts[0].profile_display());

		let button_sizer = BoxSizer::builder(Orientation::Horizontal).build();
		let actions_button = Button::builder(&panel).with_label("Actions...").build();
		let timeline_button = Button::builder(&panel).with_id(ID_OK).with_label("View &Timeline").build();
		let close_button = Button::builder(&panel).with_id(ID_CANCEL).with_label("Close").build();
		button_sizer.add(&actions_button, 0, SizerFlag::Right, 8);
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

		let accounts_sel = accounts_rc.clone();
		let text_sel = profile_text;
		let relationship_sel = relationship.clone();
		let current_account_sel = current_account_rc.clone();
		let net_tx_sel = net_tx.clone();
		account_list.on_selection_changed(move |_| {
			let selection = account_list.get_selection().map(|sel| sel as usize);
			if let Some(index) = selection
				&& let Some(account) = accounts_sel.borrow().get(index)
			{
				text_sel.set_value(&account.profile_display());
				*relationship_sel.borrow_mut() = None;
				*current_account_sel.borrow_mut() = account.clone();
				if !account.acct.contains('@') {
					let _ = net_tx_sel.send(NetworkCommand::FetchRelationship { account_id: account.id.clone() });
				}
			}
		});

		user_actions::setup_actions_button(
			panel,
			actions_button,
			current_account_rc.clone(),
			relationship.clone(),
			net_tx.clone(),
		);

		let accounts_timeline = accounts_rc.clone();
		let dlg_timeline = dialog;
		timeline_button.on_click(move |_| {
			let selection = account_list.get_selection().map(|sel| sel as usize);
			if let Some(index) = selection
				&& let Some(account) = accounts_timeline.borrow().get(index)
			{
				on_view_timeline(account.clone());
				dlg_timeline.close(true);
			}
		});

		let dlg_close = dialog;
		close_button.on_click(move |_| {
			dlg_close.close(true);
		});

		dialog.on_close(move |_| {
			on_close();
		});

		dialog.centre();
		Some(Self {
			dialog,
			token,
			relationship,
			profile_text,
			accounts: accounts_rc,
			account_list,
			current_account: current_account_rc,
		})
	}

	pub fn show(&self) {
		self.dialog.show(true);
	}

	pub fn token(&self) -> u64 {
		self.token
	}

	pub fn update_relationship(&self, account_id: &str, rel: &Relationship) {
		if self.current_account.borrow().id != account_id {
			return;
		}
		*self.relationship.borrow_mut() = Some(rel.clone());
		let selection = self.account_list.get_selection().map(|sel| sel as usize);
		if let Some(index) = selection
			&& let Some(account) = self.accounts.borrow().get(index)
		{
			let mut text = account.profile_display();
			user_actions::append_relationship_text(&mut text, rel);
			self.profile_text.set_value(&text);
		}
	}
}
