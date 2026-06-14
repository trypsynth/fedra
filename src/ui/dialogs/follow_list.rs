use std::{cell::RefCell, collections::HashMap, rc::Rc, sync::mpsc::Sender};

use wxdragon::{event::MenuEvents, prelude::*};

use super::user_actions;
use crate::{
	mastodon::{Account, Relationship},
	network::NetworkCommand,
};

pub struct FollowListDialog {
	dialog: Dialog,
	account_list: ListBox,
	profile_text: TextCtrl,
	accounts: Rc<RefCell<Vec<Account>>>,
	relationships: Rc<RefCell<HashMap<String, Relationship>>>,
	title_base: String,
	total_count: u64,
	loaded: Rc<RefCell<bool>>,
	pub account_id: Option<String>,
}

impl FollowListDialog {
	pub fn new<F, C>(
		parent: &dyn WxWidget,
		title: &str,
		label: &str,
		first_page: &[Account],
		total_count: u64,
		account_id: Option<String>,
		net_tx: Sender<NetworkCommand>,
		ui_tx: crate::ui_wake::UiCommandSender,
		on_view_timeline: F,
		on_close: C,
	) -> Self
	where
		F: Fn(Account) + 'static,
		C: Fn() + 'static,
	{
		const ID_VIEW_TIMELINE: i32 = 10044;

		let dialog_title = Self::make_title(title, first_page.len() as u64, total_count, false);
		let dialog = Dialog::builder(parent, &dialog_title).with_size(600, 400).build();
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
		let actions_button = Button::builder(&panel).with_label("&Actions...").build();
		let timeline_button = Button::builder(&panel).with_id(ID_VIEW_TIMELINE).with_label("View &Timeline").build();
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

		let accounts_rc: Rc<RefCell<Vec<Account>>> = Rc::new(RefCell::new(first_page.to_vec()));
		let relationships_rc: Rc<RefCell<HashMap<String, Relationship>>> = Rc::new(RefCell::new(HashMap::new()));
		let current_account_rc: Rc<RefCell<Option<Account>>> = Rc::new(RefCell::new(first_page.first().cloned()));

		let list_sel = account_list;
		let text_sel = profile_text;
		let accounts_sel = accounts_rc.clone();
		let relationships_sel = relationships_rc.clone();
		let current_account_sel = current_account_rc.clone();
		list_sel.on_selection_changed(move |_| {
			let selection = list_sel.get_selection().map(|sel| sel as usize);
			if let Some(index) = selection
				&& let Some(account) = accounts_sel.borrow().get(index)
			{
				let mut text = account.profile_display();
				if let Some(rel) = relationships_sel.borrow().get(&account.id) {
					user_actions::append_relationship_text(&mut text, rel, false);
				}
				text_sel.set_value(&text);
				*current_account_sel.borrow_mut() = Some(account.clone());
			}
		});

		let relationships_click = relationships_rc.clone();
		let current_account_click = current_account_rc.clone();
		let panel_clone = panel.clone();

		let show_menu = Rc::new(move || {
			let current = current_account_click.borrow();
			let Some(account) = current.as_ref() else { return };
			let rel = relationships_click.borrow().get(&account.id).cloned();
			let mut menu = Menu::builder().build();
			if let Some(r) = &rel {
				if r.following {
					menu.append(user_actions::ID_ACTION_UNFOLLOW, "Unfollow", "", ItemKind::Normal);
					if r.showing_reblogs {
						menu.append(user_actions::ID_ACTION_HIDE_BOOSTS, "Hide Boosts", "", ItemKind::Normal);
					} else {
						menu.append(user_actions::ID_ACTION_SHOW_BOOSTS, "Show Boosts", "", ItemKind::Normal);
					}
				} else if r.requested {
					menu.append(user_actions::ID_ACTION_UNFOLLOW, "Cancel Follow Request", "", ItemKind::Normal);
				} else {
					menu.append(user_actions::ID_ACTION_FOLLOW, "Follow", "", ItemKind::Normal);
				}
				if r.requested_by {
					menu.append(
						user_actions::ID_ACTION_ACCEPT_FOLLOW_REQUEST,
						"Accept Follow Request",
						"",
						ItemKind::Normal,
					);
					menu.append(
						user_actions::ID_ACTION_REJECT_FOLLOW_REQUEST,
						"Reject Follow Request",
						"",
						ItemKind::Normal,
					);
				}
				if r.muting {
					menu.append(user_actions::ID_ACTION_UNMUTE, "Unmute", "", ItemKind::Normal);
				} else {
					menu.append(user_actions::ID_ACTION_MUTE, "Mute", "", ItemKind::Normal);
				}
				if r.blocking {
					menu.append(user_actions::ID_ACTION_UNBLOCK, "Unblock", "", ItemKind::Normal);
				} else {
					menu.append(user_actions::ID_ACTION_BLOCK, "Block", "", ItemKind::Normal);
				}
				menu.append_separator();
			}
			menu.append(user_actions::ID_ACTION_OPEN_BROWSER, "Open in Browser", "", ItemKind::Normal);
			menu.append_separator();
			menu.append(user_actions::ID_ACTION_VIEW_FOLLOWERS, "View Followers", "", ItemKind::Normal);
			menu.append(user_actions::ID_ACTION_VIEW_FOLLOWING, "View Following", "", ItemKind::Normal);
			menu.append_separator();
			menu.append(user_actions::ID_ACTION_ADD_TO_LIST, "Add to List...", "", ItemKind::Normal);
			panel_clone.popup_menu(&mut menu, None);
		});

		let show_menu_btn = show_menu.clone();
		actions_button.on_click(move |_| {
			show_menu_btn();
		});

		let show_menu_ctx = show_menu.clone();
		panel.on_context_menu(move |_| {
			show_menu_ctx();
		});

		let relationships_handler = relationships_rc.clone();
		let current_account_handler = current_account_rc.clone();
		panel.on_menu_selected(move |event| {
			let id = event.get_id();
			let current = current_account_handler.borrow();
			let Some(account) = current.as_ref() else { return };
			let account_id = account.id.clone();
			let target_name = account.display_name_or_username().to_string();
			if id == user_actions::ID_ACTION_OPEN_BROWSER {
				let _ =
					wxdragon::utils::launch_default_browser(&account.url, wxdragon::utils::BrowserLaunchFlags::Default);
				return;
			}
			if id == user_actions::ID_ACTION_VIEW_FOLLOWERS {
				let acct = account.acct.clone();
				let total_count = account.followers_count;
				let _ = net_tx.send(NetworkCommand::FetchFollowers { account_id, acct, total_count });
				return;
			}
			if id == user_actions::ID_ACTION_VIEW_FOLLOWING {
				let acct = account.acct.clone();
				let total_count = account.following_count;
				let _ = net_tx.send(NetworkCommand::FetchFollowing { account_id, acct, total_count });
				return;
			}
			if id == user_actions::ID_ACTION_ADD_TO_LIST {
				let _ = ui_tx.send(crate::commands::UiCommand::AddUserToList(account_id));
				return;
			}
			let rel = relationships_handler.borrow().get(&account_id).cloned();
			let cmd = match id {
				user_actions::ID_ACTION_FOLLOW => NetworkCommand::FollowAccount {
					account_id,
					target_name,
					reblogs: true,
					action: crate::network::RelationshipAction::Follow,
				},
				user_actions::ID_ACTION_UNFOLLOW => NetworkCommand::UnfollowAccount {
					account_id,
					target_name,
					action: if rel.as_ref().is_some_and(|r| !r.following && r.requested) {
						crate::network::RelationshipAction::CancelFollowRequest
					} else {
						crate::network::RelationshipAction::Unfollow
					},
				},
				user_actions::ID_ACTION_SHOW_BOOSTS => NetworkCommand::FollowAccount {
					account_id,
					target_name,
					reblogs: true,
					action: crate::network::RelationshipAction::ShowBoosts,
				},
				user_actions::ID_ACTION_HIDE_BOOSTS => NetworkCommand::FollowAccount {
					account_id,
					target_name,
					reblogs: false,
					action: crate::network::RelationshipAction::HideBoosts,
				},
				user_actions::ID_ACTION_BLOCK => {
					let confirm =
						MessageDialog::builder(&panel, "Are you sure you want to block this user?", "Block User")
							.with_style(MessageDialogStyle::YesNo | MessageDialogStyle::IconWarning)
							.build();
					if confirm.show_modal() != ID_YES {
						return;
					}
					NetworkCommand::BlockAccount { account_id, target_name }
				}
				user_actions::ID_ACTION_UNBLOCK => NetworkCommand::UnblockAccount { account_id, target_name },
				user_actions::ID_ACTION_MUTE => NetworkCommand::MuteAccount { account_id, target_name },
				user_actions::ID_ACTION_UNMUTE => NetworkCommand::UnmuteAccount { account_id, target_name },
				user_actions::ID_ACTION_ACCEPT_FOLLOW_REQUEST => {
					NetworkCommand::AuthorizeFollowRequest { account_id, target_name }
				}
				user_actions::ID_ACTION_REJECT_FOLLOW_REQUEST => {
					NetworkCommand::RejectFollowRequest { account_id, target_name }
				}
				_ => return,
			};
			let _ = net_tx.send(cmd);
		});

		let accounts_btn = accounts_rc.clone();
		let list_btn = account_list;
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
			profile_text,
			accounts: accounts_rc,
			relationships: relationships_rc,
			title_base: title.to_string(),
			total_count,
			loaded: Rc::new(RefCell::new(false)),
			account_id,
		}
	}

	pub fn show(&self) {
		self.dialog.show(true);
	}

	pub fn update_relationships(&self, relationships: &[Relationship]) {
		{
			let mut map = self.relationships.borrow_mut();
			for rel in relationships {
				map.insert(rel.id.clone(), rel.clone());
			}
		}
		if let Some(sel) = self.account_list.get_selection() {
			let accounts = self.accounts.borrow();
			if let Some(account) = accounts.get(sel as usize) {
				let mut text = account.profile_display();
				if let Some(rel) = self.relationships.borrow().get(&account.id) {
					user_actions::append_relationship_text(&mut text, rel, false);
				}
				self.profile_text.set_value(&text);
			}
		}
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
			if shown == total && total > 0 { format!("{base} ({total})") } else { format!("{base} ({shown})") }
		} else {
			format!("{base} ({shown} of {total}, loading\u{2026})")
		}
	}
}
