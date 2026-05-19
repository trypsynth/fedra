use std::{cell::RefCell, rc::Rc};

use wxdragon::prelude::*;

use crate::mastodon::Account;

pub struct FollowListDialog {
	dialog: Dialog,
	account_list: ListBox,
	accounts: Rc<RefCell<Vec<Account>>>,
	title_base: String,
	total_count: u64,
	loaded: Rc<RefCell<bool>>,
	pub account_id: Option<String>,
}

impl FollowListDialog {
	pub fn new<F, C>(
		frame: &Frame,
		title: &str,
		label: &str,
		first_page: &[Account],
		total_count: u64,
		account_id: Option<String>,
		on_view_timeline: F,
		on_close: C,
	) -> Self
	where
		F: Fn(Account) + 'static,
		C: Fn() + 'static,
	{
		const ID_VIEW_TIMELINE: i32 = 10044;

		let dialog_title = Self::make_title(title, first_page.len() as u64, total_count, false);
		let dialog = Dialog::builder(frame, &dialog_title).with_size(600, 400).build();
		let panel = Panel::builder(&dialog).build();
		let main_sizer = BoxSizer::builder(Orientation::Vertical).build();
		let list_label = StaticText::builder(&panel).with_label(label).build();
		let account_list = ListBox::builder(&panel).build();
		let profile_text = TextCtrl::builder(&panel)
			.with_style(TextCtrlStyle::MultiLine | TextCtrlStyle::ReadOnly | TextCtrlStyle::DontWrap)
			.build();

		for account in first_page {
			account_list.append(&Self::account_label(account));
		}
		if let Some(first) = first_page.first() {
			account_list.set_selection(0, true);
			profile_text.set_value(&first.profile_display());
		}

		let button_sizer = BoxSizer::builder(Orientation::Horizontal).build();
		let timeline_button =
			Button::builder(&panel).with_id(ID_VIEW_TIMELINE).with_label("View &Timeline").build();
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

		let accounts_rc: Rc<RefCell<Vec<Account>>> = Rc::new(RefCell::new(first_page.to_vec()));
		let list_sel = account_list.clone();
		let text_sel = profile_text.clone();
		let accounts_sel = accounts_rc.clone();
		list_sel.on_selection_changed(move |_| {
			let selection = list_sel.get_selection().map(|sel| sel as usize);
			if let Some(index) = selection
				&& let Some(account) = accounts_sel.borrow().get(index)
			{
				text_sel.set_value(&account.profile_display());
			}
		});

		let accounts_btn = accounts_rc.clone();
		let list_btn = account_list.clone();
		let dlg_timeline = dialog;
		timeline_button.on_click(move |_| {
			let selection = list_btn.get_selection().map(|s| s as usize);
			if let Some(account) = selection.and_then(|i| accounts_btn.borrow().get(i).cloned()) {
				on_view_timeline(account);
			}
			dlg_timeline.close(true);
		});

		let dlg_close = dialog;
		close_button.on_click(move |_| {
			dlg_close.close(true);
		});

		dialog.on_close(move |_| {
			on_close();
		});

		dialog.centre();
		Self {
			dialog,
			account_list,
			accounts: accounts_rc,
			title_base: title.to_string(),
			total_count,
			loaded: Rc::new(RefCell::new(false)),
			account_id,
		}
	}

	pub fn show(&self) {
		self.dialog.show(true);
	}

	pub fn append_accounts(&self, accounts: &[Account]) {
		for account in accounts {
			self.account_list.append(&Self::account_label(account));
		}
		self.accounts.borrow_mut().extend_from_slice(accounts);
		let shown = self.accounts.borrow().len() as u64;
		let is_loaded = *self.loaded.borrow();
		self.dialog.set_label(&Self::make_title(&self.title_base, shown, self.total_count, is_loaded));
	}

	pub fn mark_loaded(&self) {
		*self.loaded.borrow_mut() = true;
		let shown = self.accounts.borrow().len() as u64;
		self.dialog.set_label(&Self::make_title(&self.title_base, shown, self.total_count, true));
	}

	fn account_label(account: &Account) -> String {
		let name = account.display_name_or_username();
		if name.is_empty() { format!("@{}", account.acct) } else { format!("{} (@{})", name, account.acct) }
	}

	fn make_title(base: &str, shown: u64, total: u64, loaded: bool) -> String {
		if loaded || total == 0 {
			if shown == total && total > 0 {
				format!("{base} ({total})")
			} else {
				format!("{base} ({shown})")
			}
		} else {
			format!("{base} ({shown} of {total}, loading\u{2026})")
		}
	}
}
