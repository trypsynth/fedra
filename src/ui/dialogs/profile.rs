use std::{cell::RefCell, fmt::Write, rc::Rc, sync::mpsc::Sender};

use wxdragon::prelude::*;

use super::common::KEY_RETURN;
use crate::{
	mastodon::{Account as MastodonAccount, Tag},
	network::{NetworkCommand, ProfileUpdate},
};

const ID_ACTION_FOLLOW: i32 = 6001;
const ID_ACTION_UNFOLLOW: i32 = 6002;
const ID_ACTION_BLOCK: i32 = 6003;
const ID_ACTION_UNBLOCK: i32 = 6004;
const ID_ACTION_MUTE: i32 = 6005;
const ID_ACTION_UNMUTE: i32 = 6006;
const ID_ACTION_OPEN_BROWSER: i32 = 6007;
const ID_ACTION_SHOW_BOOSTS: i32 = 6008;
const ID_ACTION_HIDE_BOOSTS: i32 = 6009;
const ID_ACTION_VIEW_FOLLOWERS: i32 = 6010;
const ID_ACTION_VIEW_FOLLOWING: i32 = 6011;
const ID_ACTION_ACCEPT_FOLLOW_REQUEST: i32 = 6012;
const ID_ACTION_REJECT_FOLLOW_REQUEST: i32 = 6013;

pub struct ProfileDialog {
	dialog: Dialog,
	relationship: Rc<RefCell<Option<crate::mastodon::Relationship>>>,
	profile_text: TextCtrl,
	account: Rc<RefCell<MastodonAccount>>,
}

fn append_relationship_text(text: &mut String, relationship: &crate::mastodon::Relationship) {
	text.push_str("\r\n\r\nRelationship:\r\n");
	let follow_status = match (relationship.following, relationship.followed_by) {
		(true, true) => "You follow each other.",
		(true, false) => "You follow this person.",
		(false, true) => "This person follows you.",
		(false, false) => "You do not follow each other.",
	};
	let _ = writeln!(text, "{follow_status}");

	if relationship.requested {
		text.push_str("You have requested to follow this person.\r\n");
	}
	if relationship.requested_by {
		text.push_str("This person has requested to follow you.\r\n");
	}
	if relationship.blocking {
		text.push_str("You have blocked this person.\r\n");
	}
	if relationship.muting {
		text.push_str("You have muted this person.\r\n");
	}
	if relationship.domain_blocking {
		text.push_str("You have blocked this person's domain.\r\n");
	}

	if !relationship.note.is_empty() {
		let note = crate::html::strip_html(&relationship.note);
		if !note.trim().is_empty() {
			text.push_str("\r\nNote:\r\n");
			text.push_str(&note);
		}
	}
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
		let relationship_action = relationship.clone();
		let actions_btn = actions_button;

		actions_btn.on_click(move |_| {
			let mut menu = Menu::builder().build();
			{
				let rel = relationship_action.borrow();
				if let Some(r) = rel.as_ref() {
					if r.following {
						menu.append(ID_ACTION_UNFOLLOW, "Unfollow", "", ItemKind::Normal);
						if r.showing_reblogs {
							menu.append(ID_ACTION_HIDE_BOOSTS, "Hide Boosts", "", ItemKind::Normal);
						} else {
							menu.append(ID_ACTION_SHOW_BOOSTS, "Show Boosts", "", ItemKind::Normal);
						}
					} else if r.requested {
						menu.append(ID_ACTION_UNFOLLOW, "Cancel Follow Request", "", ItemKind::Normal);
					} else {
						menu.append(ID_ACTION_FOLLOW, "Follow", "", ItemKind::Normal);
					}
					if r.requested_by {
						menu.append(ID_ACTION_ACCEPT_FOLLOW_REQUEST, "Accept Follow Request", "", ItemKind::Normal);
						menu.append(ID_ACTION_REJECT_FOLLOW_REQUEST, "Reject Follow Request", "", ItemKind::Normal);
					}
					if r.muting {
						menu.append(ID_ACTION_UNMUTE, "Unmute", "", ItemKind::Normal);
					} else {
						menu.append(ID_ACTION_MUTE, "Mute", "", ItemKind::Normal);
					}
					if r.blocking {
						menu.append(ID_ACTION_UNBLOCK, "Unblock", "", ItemKind::Normal);
					} else {
						menu.append(ID_ACTION_BLOCK, "Block", "", ItemKind::Normal);
					}
					menu.append_separator();
				}
			}
			menu.append(ID_ACTION_OPEN_BROWSER, "Open in Browser", "", ItemKind::Normal);
			menu.append_separator();
			menu.append(ID_ACTION_VIEW_FOLLOWERS, "View Followers", "", ItemKind::Normal);
			menu.append(ID_ACTION_VIEW_FOLLOWING, "View Following", "", ItemKind::Normal);
			panel.popup_menu(&mut menu, None);
		});

		let account_handler = account_rc.clone();
		let relationship_handler = relationship.clone();
		let panel_handler = panel;
		let net_tx_handler = net_tx;

		panel_handler.on_menu_selected(move |event| {
			let id = event.get_id();
			let account = account_handler.borrow();
			let account_id = account.id.clone();
			let target_name = account.display_name_or_username().to_string();

			if id == ID_ACTION_OPEN_BROWSER {
				let _ =
					wxdragon::utils::launch_default_browser(&account.url, wxdragon::utils::BrowserLaunchFlags::Default);
				return;
			}
			if id == ID_ACTION_VIEW_FOLLOWERS {
				let _ = net_tx_handler.send(NetworkCommand::FetchFollowers { account_id });
				return;
			}
			if id == ID_ACTION_VIEW_FOLLOWING {
				let _ = net_tx_handler.send(NetworkCommand::FetchFollowing { account_id });
				return;
			}

			let cmd = match id {
				ID_ACTION_FOLLOW => NetworkCommand::FollowAccount {
					account_id,
					target_name,
					reblogs: true,
					action: crate::network::RelationshipAction::Follow,
				},
				ID_ACTION_UNFOLLOW => NetworkCommand::UnfollowAccount {
					account_id,
					target_name,
					action: if relationship_handler.borrow().as_ref().is_some_and(|r| !r.following && r.requested) {
						crate::network::RelationshipAction::CancelFollowRequest
					} else {
						crate::network::RelationshipAction::Unfollow
					},
				},
				ID_ACTION_SHOW_BOOSTS => NetworkCommand::FollowAccount {
					account_id,
					target_name,
					reblogs: true,
					action: crate::network::RelationshipAction::ShowBoosts,
				},
				ID_ACTION_HIDE_BOOSTS => NetworkCommand::FollowAccount {
					account_id,
					target_name,
					reblogs: false,
					action: crate::network::RelationshipAction::HideBoosts,
				},
				ID_ACTION_BLOCK => {
					let confirm = MessageDialog::builder(
						&panel_handler,
						"Are you sure you want to block this user?",
						"Block User",
					)
					.with_style(MessageDialogStyle::YesNo | MessageDialogStyle::IconWarning)
					.build();
					if confirm.show_modal() != ID_YES {
						return;
					}
					NetworkCommand::BlockAccount { account_id, target_name }
				}
				ID_ACTION_UNBLOCK => NetworkCommand::UnblockAccount { account_id, target_name },
				ID_ACTION_MUTE => NetworkCommand::MuteAccount { account_id, target_name },
				ID_ACTION_UNMUTE => NetworkCommand::UnmuteAccount { account_id, target_name },
				ID_ACTION_ACCEPT_FOLLOW_REQUEST => NetworkCommand::AuthorizeFollowRequest { account_id, target_name },
				ID_ACTION_REJECT_FOLLOW_REQUEST => NetworkCommand::RejectFollowRequest { account_id, target_name },
				_ => return,
			};
			let _ = net_tx_handler.send(cmd);
		});
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
			append_relationship_text(&mut text, &rel);
		}

		self.profile_text.set_value(&text);
	}

	pub fn update_relationship(&self, relationship: &crate::mastodon::Relationship) {
		*self.relationship.borrow_mut() = Some(relationship.clone());
		let account = self.account.borrow();
		let mut text = account.profile_display();
		append_relationship_text(&mut text, relationship);
		self.profile_text.set_value(&text);
	}
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

pub fn prompt_for_profile_edit(frame: &Frame, current: &MastodonAccount) -> Option<ProfileUpdate> {
	let dialog = Dialog::builder(frame, "Edit Profile").with_size(600, 600).build();
	let panel = Panel::builder(&dialog).build();
	let main_sizer = BoxSizer::builder(Orientation::Vertical).build();
	let scroll_win = ScrolledWindow::builder(&panel).build();
	scroll_win.set_scroll_rate(0, 10);
	let content_sizer = BoxSizer::builder(Orientation::Vertical).build();
	let name_label = StaticText::builder(&scroll_win).with_label("Display Name").build();
	let name_text = TextCtrl::builder(&scroll_win).with_value(current.display_name_or_username()).build();
	name_text.set_name("Display Name");
	content_sizer.add(&name_label, 0, SizerFlag::All, 5);
	content_sizer.add(&name_text, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 5);
	let note_label = StaticText::builder(&scroll_win).with_label("Bio").build();
	let note_text = TextCtrl::builder(&scroll_win)
		.with_value(&crate::html::strip_html(&current.note))
		.with_style(TextCtrlStyle::MultiLine)
		.with_size(Size::new(-1, 100))
		.build();
	note_text.set_name("Bio");
	content_sizer.add(&note_label, 0, SizerFlag::All, 5);
	content_sizer.add(&note_text, 0, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 5);
	let images_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	let avatar_sizer = BoxSizer::builder(Orientation::Vertical).build();
	let avatar_label = StaticText::builder(&scroll_win).with_label("Avatar:").build();
	let avatar_path = TextCtrl::builder(&scroll_win).with_style(TextCtrlStyle::ReadOnly).build();
	avatar_path.set_name("Avatar Path");
	let avatar_btn = Button::builder(&scroll_win).with_label("Change Avatar...").build();
	avatar_sizer.add(&avatar_label, 0, SizerFlag::All, 5);
	avatar_sizer.add(&avatar_path, 0, SizerFlag::Expand | SizerFlag::All, 5);
	avatar_sizer.add(&avatar_btn, 0, SizerFlag::All, 5);
	let header_sizer = BoxSizer::builder(Orientation::Vertical).build();
	let header_label = StaticText::builder(&scroll_win).with_label("Header:").build();
	let header_path = TextCtrl::builder(&scroll_win).with_style(TextCtrlStyle::ReadOnly).build();
	header_path.set_name("Header Path");
	let header_btn = Button::builder(&scroll_win).with_label("Change Header...").build();
	header_sizer.add(&header_label, 0, SizerFlag::All, 5);
	header_sizer.add(&header_path, 0, SizerFlag::Expand | SizerFlag::All, 5);
	header_sizer.add(&header_btn, 0, SizerFlag::All, 5);
	images_sizer.add_sizer(&avatar_sizer, 1, SizerFlag::Expand, 0);
	images_sizer.add_sizer(&header_sizer, 1, SizerFlag::Expand, 0);
	content_sizer.add_sizer(&images_sizer, 0, SizerFlag::Expand, 0);
	let locked_cb = CheckBox::builder(&scroll_win).with_label("Require &follow approval").build();
	locked_cb.set_value(current.locked);
	content_sizer.add(&locked_cb, 0, SizerFlag::All, 5);
	let bot_cb = CheckBox::builder(&scroll_win).with_label("&Bot account").build();
	bot_cb.set_value(current.bot);
	content_sizer.add(&bot_cb, 0, SizerFlag::All, 5);
	let discoverable_cb = CheckBox::builder(&scroll_win).with_label("&Discoverable in directory").build();
	discoverable_cb.set_value(current.discoverable.unwrap_or(false));
	content_sizer.add(&discoverable_cb, 0, SizerFlag::All, 5);
	let mut field_controls = Vec::new();
	for i in 0..4 {
		let row_sizer = BoxSizer::builder(Orientation::Horizontal).build();
		let (name_val, val_val) = if i < current.fields.len() {
			(current.fields[i].name.clone(), html::strip_html(&current.fields[i].value))
		} else {
			(String::new(), String::new())
		};
		let title_lbl = format!("Field {} label", i + 1);
		let content_lbl = format!("Field {} content", i + 1);
		let field_sizer = BoxSizer::builder(Orientation::Vertical).build();
		let title_text = StaticText::builder(&scroll_win).with_label(&title_lbl).build();
		let name_ctrl = TextCtrl::builder(&scroll_win).with_value(&name_val).build();
		name_ctrl.set_name(&title_lbl);
		field_sizer.add(&title_text, 0, SizerFlag::All, 2);
		field_sizer.add(&name_ctrl, 0, SizerFlag::Expand | SizerFlag::All, 2);
		let content_sizer_inner = BoxSizer::builder(Orientation::Vertical).build();
		let content_text = StaticText::builder(&scroll_win).with_label(&content_lbl).build();
		let val_ctrl = TextCtrl::builder(&scroll_win).with_value(&val_val).build();
		val_ctrl.set_name(&content_lbl);
		content_sizer_inner.add(&content_text, 0, SizerFlag::All, 2);
		content_sizer_inner.add(&val_ctrl, 0, SizerFlag::Expand | SizerFlag::All, 2);
		row_sizer.add_sizer(&field_sizer, 1, SizerFlag::Expand, 0);
		row_sizer.add_sizer(&content_sizer_inner, 2, SizerFlag::Expand, 0);
		content_sizer.add_sizer(&row_sizer, 0, SizerFlag::Expand | SizerFlag::All, 5);
		field_controls.push((name_ctrl, val_ctrl));
	}
	let (privacy_choice_opt, sensitive_cb_opt, lang_text_opt) =
		current.source.as_ref().map_or((None, None, None), |source| {
			let privacy_sizer = BoxSizer::builder(Orientation::Horizontal).build();
			let privacy_label = StaticText::builder(&scroll_win).with_label("Default post visibility").build();
			let privacy_choices: Vec<String> =
				vec!["Public".to_string(), "Unlisted".to_string(), "Followers only".to_string()];
			let privacy_choice = Choice::builder(&scroll_win).with_choices(privacy_choices).build();
			let sel = match source.privacy.as_deref() {
				Some("unlisted") => 1,
				Some("private") => 2,
				_ => 0,
			};
			privacy_choice.set_selection(sel);
			privacy_sizer.add(&privacy_label, 0, SizerFlag::AlignCenterVertical | SizerFlag::Right, 5);
			privacy_sizer.add(&privacy_choice, 1, SizerFlag::Expand, 0);
			content_sizer.add_sizer(&privacy_sizer, 0, SizerFlag::Expand | SizerFlag::All, 5);
			let sensitive_cb = CheckBox::builder(&scroll_win).with_label("&Mark media as sensitive by default").build();
			sensitive_cb.set_value(source.sensitive.unwrap_or(false));
			content_sizer.add(&sensitive_cb, 0, SizerFlag::All, 5);
			let lang_sizer = BoxSizer::builder(Orientation::Horizontal).build();
			let lang_label = StaticText::builder(&scroll_win).with_label("Language (ISO code):").build();
			let lang_text = TextCtrl::builder(&scroll_win).with_value(source.language.as_deref().unwrap_or("")).build();
			lang_sizer.add(&lang_label, 0, SizerFlag::AlignCenterVertical | SizerFlag::Right, 5);
			lang_sizer.add(&lang_text, 1, SizerFlag::Expand, 0);
			content_sizer.add_sizer(&lang_sizer, 0, SizerFlag::Expand | SizerFlag::All, 5);
			(Some(privacy_choice), Some(sensitive_cb), Some(lang_text))
		});
	scroll_win.set_sizer(content_sizer, true);
	main_sizer.add(&scroll_win, 1, SizerFlag::Expand, 0);
	let btn_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	let ok_btn = Button::builder(&panel).with_id(ID_OK).with_label("Save Changes").build();
	ok_btn.set_default();
	let cancel_btn = Button::builder(&panel).with_id(ID_CANCEL).with_label("Cancel").build();
	btn_sizer.add_stretch_spacer(1);
	btn_sizer.add(&ok_btn, 0, SizerFlag::Right, 8);
	btn_sizer.add(&cancel_btn, 0, SizerFlag::Right, 8);
	main_sizer.add_sizer(&btn_sizer, 0, SizerFlag::Expand | SizerFlag::All, 10);
	panel.set_sizer(main_sizer, true);
	let dlg_sizer = BoxSizer::builder(Orientation::Vertical).build();
	dlg_sizer.add(&panel, 1, SizerFlag::Expand, 0);
	dialog.set_sizer(dlg_sizer, true);
	dialog.centre();
	let avatar_path_ref = avatar_path;
	let panel_ref = panel;
	avatar_btn.on_click(move |_| {
		let dlg = FileDialog::builder(&panel_ref)
			.with_message("Select Avatar")
			.with_wildcard("Image files|*.png;*.jpg;*.jpeg;*.gif")
			.with_style(FileDialogStyle::Open | FileDialogStyle::FileMustExist)
			.build();
		if dlg.show_modal() == ID_OK
			&& let Some(path) = dlg.get_path()
		{
			avatar_path_ref.set_value(&path);
		}
	});
	let header_path_ref = header_path;
	let panel_ref = panel;
	header_btn.on_click(move |_| {
		let dlg = FileDialog::builder(&panel_ref)
			.with_message("Select Header")
			.with_wildcard("Image files|*.png;*.jpg;*.jpeg;*.gif")
			.with_style(FileDialogStyle::Open | FileDialogStyle::FileMustExist)
			.build();
		if dlg.show_modal() == ID_OK
			&& let Some(path) = dlg.get_path()
		{
			header_path_ref.set_value(&path);
		}
	});
	if dialog.show_modal() != ID_OK {
		return None;
	}
	let display_name = name_text.get_value();
	let note = note_text.get_value();
	let avatar = avatar_path.get_value();
	let header = header_path.get_value();
	let locked = locked_cb.get_value();
	let bot = bot_cb.get_value();
	let discoverable = discoverable_cb.get_value();
	let mut fields_attributes = Vec::new();
	for (name_ctrl, val_ctrl) in &field_controls {
		let name = name_ctrl.get_value();
		let val = val_ctrl.get_value();
		// Always send all fields to preserve indices (0..3) so the server knows which to update/clear
		fields_attributes.push((name, val));
	}
	let source = if let (Some(privacy_choice), Some(sensitive_cb), Some(lang_text)) =
		(privacy_choice_opt, sensitive_cb_opt, lang_text_opt)
	{
		let privacy = match privacy_choice.get_selection() {
			Some(1) => "unlisted",
			Some(2) => "private",
			_ => "public",
		}
		.to_string();
		Some(crate::mastodon::Source {
			privacy: Some(privacy),
			sensitive: Some(sensitive_cb.get_value()),
			language: Some(lang_text.get_value()),
		})
	} else {
		None
	};
	Some(ProfileUpdate {
		display_name: Some(display_name),
		note: Some(note),
		avatar: if avatar.is_empty() { None } else { Some(avatar) },
		header: if header.is_empty() { None } else { Some(header) },
		locked: Some(locked),
		bot: Some(bot),
		discoverable: Some(discoverable),
		fields_attributes: Some(fields_attributes),
		source,
	})
}
