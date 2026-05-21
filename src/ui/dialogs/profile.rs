use std::{cell::RefCell, rc::Rc, sync::mpsc::Sender};

use wxdragon::prelude::*;

use crate::{
	mastodon::{Account as MastodonAccount, Mention, Tag},
	network::NetworkCommand,
	ui::dialogs::UserLookupAction,
};

use super::user_actions;

pub struct ProfileDialog {
	dialog: Dialog,
	relationship: Rc<RefCell<Option<crate::mastodon::Relationship>>>,
	profile_text: TextCtrl,
	account: Rc<RefCell<MastodonAccount>>,
}

impl ProfileDialog {
	pub fn new<F, C>(
		frame: &Frame,
		account: MastodonAccount,
		net_tx: std::sync::mpsc::Sender<NetworkCommand>,
		on_view_timeline: F,
		on_close: C,
	) -> Self
	where
		F: Fn() + 'static + Clone,
		C: Fn() + 'static + Clone,
	{
		let title = format!("Profile for {}", account.display_name_or_username());
		let dialog = Dialog::builder(frame, &title).with_size(500, 400).build();
		let panel = Panel::builder(&dialog).build();
		let main_sizer = BoxSizer::builder(Orientation::Vertical).build();
		let profile_text = TextCtrl::builder(&panel)
			.with_style(TextCtrlStyle::MultiLine | TextCtrlStyle::ReadOnly | TextCtrlStyle::DontWrap)
			.build();
		profile_text.set_value(&account.profile_display());
		let button_sizer = BoxSizer::builder(Orientation::Horizontal).build();
		let actions_button = Button::builder(&panel).with_label("Actions...").build();
		let timeline_button = Button::builder(&panel).with_id(ID_OK).with_label("View &Timeline").build();
		let close_button = Button::builder(&panel).with_id(ID_CANCEL).with_label("&Close").build();
		close_button.set_default();
		button_sizer.add(&actions_button, 0, SizerFlag::Right, 8);
		button_sizer.add(&timeline_button, 0, SizerFlag::Right, 8);
		button_sizer.add_stretch_spacer(1);
		button_sizer.add(&close_button, 0, SizerFlag::Right, 8);
		main_sizer.add(&profile_text, 1, SizerFlag::Expand | SizerFlag::All, 8);
		main_sizer.add_sizer(
			&button_sizer,
			0,
			SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Bottom,
			8,
		);
		panel.set_sizer(main_sizer, true);
		let dialog_sizer = BoxSizer::builder(Orientation::Vertical).build();
		dialog_sizer.add(&panel, 1, SizerFlag::Expand, 0);
		dialog.set_sizer(dialog_sizer, true);
		dialog.set_escape_id(ID_CANCEL);

		let relationship: Rc<RefCell<Option<crate::mastodon::Relationship>>> = Rc::new(RefCell::new(None));
		let account_rc = Rc::new(RefCell::new(account));

		user_actions::setup_actions_button(panel, actions_button, account_rc.clone(), relationship.clone(), net_tx);
		let dlg_timeline = dialog;
		let on_view_timeline = on_view_timeline;
		timeline_button.on_click(move |_| {
			on_view_timeline();
			dlg_timeline.close(true);
		});

		let dlg_close = dialog;
		close_button.on_click(move |_| {
			dlg_close.close(true);
		});

		let on_close_win = on_close;
		dialog.on_close(move |_| {
			on_close_win();
		});

		dialog.centre();
		Self { dialog, relationship, profile_text, account: account_rc }
	}

	pub fn show(&self) {
		self.dialog.show(true);
	}

	pub fn update_account(&self, account: &MastodonAccount) {
		self.account.replace(account.clone());
		self.dialog.set_label(&format!("Profile for {}", account.display_name_or_username()));

		let mut text = account.profile_display();

		if let Some(rel) = self.relationship.borrow().clone() {
			user_actions::append_relationship_text(&mut text, &rel);
		}

		self.profile_text.set_value(&text);
	}

	pub fn update_relationship(&self, relationship: &crate::mastodon::Relationship) {
		*self.relationship.borrow_mut() = Some(relationship.clone());
		let account = self.account.borrow();
		let mut text = account.profile_display();
		user_actions::append_relationship_text(&mut text, relationship);
		self.profile_text.set_value(&text);
	}
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

#[derive(Clone)]
pub struct HashtagDialog {
	dialog: Dialog,
	list: ListBox,
	action_button: Button,
	tags: Rc<RefCell<Vec<crate::mastodon::Tag>>>,
}

impl HashtagDialog {
	pub fn new<F>(frame: &Frame, tags: Vec<Tag>, net_tx: Sender<NetworkCommand>, on_close: F) -> Self
	where
		F: Fn() + 'static + Clone,
	{
		let dialog = Dialog::builder(frame, "Hashtags").with_size(500, 300).build();
		let panel = Panel::builder(&dialog).build();
		let main_sizer = BoxSizer::builder(Orientation::Vertical).build();
		let list_label = StaticText::builder(&panel).with_label("Hashtags in post:").build();
		let tag_list = ListBox::builder(&panel).build();
		let format_tag = |tag: &crate::mastodon::Tag| -> String {
			let status = if tag.following { " (Following)" } else { "" };
			format!("#{}{}", tag.name, status)
		};
		for tag in &tags {
			tag_list.append(&format_tag(tag));
		}
		if !tags.is_empty() {
			tag_list.set_selection(0, true);
		}
		let button_sizer = BoxSizer::builder(Orientation::Horizontal).build();
		let action_button = Button::builder(&panel).with_label("Follow").build();
		let close_button = Button::builder(&panel).with_id(ID_CANCEL).with_label("Close").build();
		close_button.set_default();
		button_sizer.add(&action_button, 0, SizerFlag::Right, 8);
		button_sizer.add_stretch_spacer(1);
		button_sizer.add(&close_button, 0, SizerFlag::Right, 8);
		main_sizer.add(&list_label, 0, SizerFlag::Expand | SizerFlag::All, 8);
		main_sizer.add(&tag_list, 1, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
		main_sizer.add_sizer(&button_sizer, 0, SizerFlag::Expand | SizerFlag::All, 8);
		panel.set_sizer(main_sizer, true);
		let dialog_sizer = BoxSizer::builder(Orientation::Vertical).build();
		dialog_sizer.add(&panel, 1, SizerFlag::Expand, 0);
		dialog.set_sizer(dialog_sizer, true);
		let tags_rc = Rc::new(RefCell::new(tags));
		let handle = Self { dialog, list: tag_list, action_button, tags: tags_rc.clone() };
		let update_button_state = {
			let tags = tags_rc.clone();
			let btn = action_button;
			let list = tag_list;
			move || {
				if let Some(sel) = list.get_selection() {
					if let Some(tag) = tags.borrow().get(sel as usize) {
						btn.enable(true);
						if tag.following {
							btn.set_label("Unfollow");
						} else {
							btn.set_label("Follow");
						}
					} else {
						btn.enable(false);
					}
				} else {
					btn.enable(false);
				}
			}
		};
		update_button_state();
		let update_on_sel = update_button_state;
		tag_list.on_selection_changed(move |_| {
			update_on_sel();
		});
		let tags_action = tags_rc;
		let list_action = tag_list;
		let net_tx_action = net_tx;
		action_button.on_click(move |_| {
			if let Some(sel) = list_action.get_selection() {
				let index = sel as usize;
				let tags_borrow = tags_action.borrow();
				if let Some(tag) = tags_borrow.get(index) {
					let cmd = if tag.following {
						NetworkCommand::UnfollowTag { name: tag.name.clone() }
					} else {
						NetworkCommand::FollowTag { name: tag.name.clone() }
					};
					let _ = net_tx_action.send(cmd);
				}
			}
		});
		let dlg = dialog;
		close_button.on_click(move |_| {
			dlg.close(true);
		});
		let on_close_win = on_close;
		dialog.on_close(move |_| {
			on_close_win();
		});
		handle
	}

	pub fn show(&self) {
		self.dialog.show(true);
	}

	pub fn update_tag(&self, name: &str, following: bool) {
		let mut tags = self.tags.borrow_mut();
		let mut index = None;
		for (i, tag) in tags.iter_mut().enumerate() {
			if tag.name.eq_ignore_ascii_case(name) {
				tag.following = following;
				index = Some(i);
			}
		}
		if let Some(i) = index {
			let format_tag = |tag: &crate::mastodon::Tag| -> String {
				let status = if tag.following { " (Following)" } else { "" };
				format!("#{} {}", tag.name, status)
			};
			let sel = self.list.get_selection();
			self.list.clear();
			for t in tags.iter() {
				self.list.append(&format_tag(t));
			}
			if let Some(s) = sel {
				self.list.set_selection(s, true);
			}
			if let Ok(i_u32) = u32::try_from(i)
				&& sel == Some(i_u32)
			{
				if following {
					self.action_button.set_label("Unfollow");
				} else {
					self.action_button.set_label("Follow");
				}
			}
		}
	}
}
