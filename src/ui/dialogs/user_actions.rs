use std::{cell::RefCell, fmt::Write, rc::Rc};

use wx_utils::{add_ok_cancel_footer, build_ok_cancel_buttons_on};
use wxdragon::prelude::*;

use super::FollowList;
use crate::{
	mastodon::{Account, Relationship},
	network::{FollowOptions, MUTE_DURATIONS, MuteOptions, RelationshipAction, change_relationship},
	responses::{note_saved, relationship_updated},
	ui::commands::{Requests, UiCommand},
	ui_wake::UiCommandSender,
};

pub const ID_ACTION_FOLLOW: i32 = 6001;
pub const ID_ACTION_UNFOLLOW: i32 = 6002;
pub const ID_ACTION_BLOCK: i32 = 6003;
pub const ID_ACTION_UNBLOCK: i32 = 6004;
pub const ID_ACTION_MUTE: i32 = 6005;
pub const ID_ACTION_UNMUTE: i32 = 6006;
pub const ID_ACTION_OPEN_BROWSER: i32 = 6007;
pub const ID_ACTION_SHOW_BOOSTS: i32 = 6008;
pub const ID_ACTION_HIDE_BOOSTS: i32 = 6009;
pub const ID_ACTION_VIEW_FOLLOWERS: i32 = 6010;
pub const ID_ACTION_VIEW_FOLLOWING: i32 = 6011;
pub const ID_ACTION_ACCEPT_FOLLOW_REQUEST: i32 = 6012;
pub const ID_ACTION_REJECT_FOLLOW_REQUEST: i32 = 6013;
pub const ID_ACTION_ADD_TO_LIST: i32 = 6014;
pub const ID_ACTION_ENABLE_NOTIFICATIONS: i32 = 6015;
pub const ID_ACTION_DISABLE_NOTIFICATIONS: i32 = 6016;
pub const ID_ACTION_SEND_DIRECT_MESSAGE: i32 = 6017;
pub const ID_ACTION_SET_ALIAS: i32 = 6018;
pub const ID_ACTION_REMOVE_FROM_LIST: i32 = 6019;
pub const ID_ACTION_EDIT_NOTE: i32 = 6020;
pub const ID_ACTION_REMOVE_FOLLOWER: i32 = 6021;

pub fn append_relationship_text(text: &mut String, relationship: &Relationship, is_own_account: bool) {
	let mut body = String::new();
	if !is_own_account {
		let follow_status = match (relationship.following, relationship.followed_by) {
			(true, true) => "You follow each other.",
			(true, false) => "You follow this person.",
			(false, true) => "This person follows you.",
			(false, false) => "You do not follow each other.",
		};
		let _ = writeln!(body, "{follow_status}");
		// Mastodon's default policy files private mentions from people the recipient doesn't
		// follow as message requests, and nothing tells the sender.
		if !relationship.followed_by && !relationship.blocking {
			body.push_str("Your direct messages may reach them as a message request, since they don't follow you.\r\n");
		}
	}
	if relationship.requested {
		body.push_str("You have requested to follow this person.\r\n");
	}
	if relationship.requested_by {
		body.push_str("This person has requested to follow you.\r\n");
	}
	if relationship.blocking {
		body.push_str("You have blocked this person.\r\n");
	}
	if relationship.blocked_by {
		body.push_str("This person has blocked you.\r\n");
	}
	if relationship.muting && relationship.muting_notifications {
		body.push_str("You have muted this person and their notifications.\r\n");
	} else if relationship.muting {
		body.push_str("You have muted this person, but not their notifications.\r\n");
	}
	if relationship.endorsed {
		body.push_str("You feature this person on your profile.\r\n");
	}
	if relationship.domain_blocking {
		body.push_str("You have blocked this person's domain.\r\n");
	}
	if !relationship.note.is_empty() {
		let note = crate::html::strip_html(&relationship.note);
		if !note.trim().is_empty() {
			body.push_str("\r\nNote:\r\n");
			body.push_str(&note);
		}
	}
	if !body.is_empty() {
		text.push_str("\r\n\r\nRelationship:\r\n");
		text.push_str(&body);
	}
}

/// The account actions menu, worded for `relationship`, which is `None` until it loads.
pub fn build_actions_menu(relationship: Option<&Relationship>) -> Menu {
	let menu = Menu::builder().build();
	if let Some(r) = relationship {
		if r.following {
			menu.append(ID_ACTION_UNFOLLOW, "Unfollow", "", ItemKind::Normal);
			if r.showing_reblogs {
				menu.append(ID_ACTION_HIDE_BOOSTS, "Hide Boosts", "", ItemKind::Normal);
			} else {
				menu.append(ID_ACTION_SHOW_BOOSTS, "Show Boosts", "", ItemKind::Normal);
			}
			if r.notifying {
				menu.append(ID_ACTION_DISABLE_NOTIFICATIONS, "Turn Off Notifications", "", ItemKind::Normal);
			} else {
				menu.append(ID_ACTION_ENABLE_NOTIFICATIONS, "Turn On Notifications", "", ItemKind::Normal);
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
		if r.followed_by {
			menu.append(ID_ACTION_REMOVE_FOLLOWER, "Remove from Followers", "", ItemKind::Normal);
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
	menu.append(ID_ACTION_SEND_DIRECT_MESSAGE, "Send Direct Message...", "", ItemKind::Normal);
	menu.append(ID_ACTION_OPEN_BROWSER, "Open in Browser", "", ItemKind::Normal);
	menu.append(ID_ACTION_SET_ALIAS, "Set Alias...", "", ItemKind::Normal);
	// Only once the note is known, so saving can't replace one the user didn't see.
	if relationship.is_some() {
		menu.append(ID_ACTION_EDIT_NOTE, "Edit Private Note...", "", ItemKind::Normal);
	}
	menu.append_separator();
	menu.append(ID_ACTION_VIEW_FOLLOWERS, "View Followers", "", ItemKind::Normal);
	menu.append(ID_ACTION_VIEW_FOLLOWING, "View Following", "", ItemKind::Normal);
	menu.append_separator();
	menu.append(ID_ACTION_ADD_TO_LIST, "Add to List...", "", ItemKind::Normal);
	menu.append(ID_ACTION_REMOVE_FROM_LIST, "Remove from List...", "", ItemKind::Normal);
	menu
}

/// Runs the action `id` from [`build_actions_menu`] on `account`.
pub fn handle_action(
	id: i32,
	account: &Account,
	relationship: Option<&Relationship>,
	parent: &dyn WxWidget,
	requests: &Requests,
	ui_tx: &UiCommandSender,
) {
	let current = FollowOptions {
		reblogs: relationship.is_none_or(|r| r.showing_reblogs),
		notify: relationship.is_some_and(|r| r.notifying),
	};
	let (action, options) = match id {
		ID_ACTION_OPEN_BROWSER => {
			let _ = wxdragon::utils::launch_default_browser(&account.url, wxdragon::utils::BrowserLaunchFlags::Default);
			return;
		}
		ID_ACTION_VIEW_FOLLOWERS | ID_ACTION_VIEW_FOLLOWING => {
			let list = if id == ID_ACTION_VIEW_FOLLOWERS { FollowList::Followers } else { FollowList::Following };
			let _ = ui_tx.send(UiCommand::ShowFollowList { account: Some(Box::new(account.clone())), list });
			return;
		}
		ID_ACTION_ADD_TO_LIST => {
			let _ = ui_tx.send(UiCommand::AddUserToList(account.id.clone()));
			return;
		}
		ID_ACTION_REMOVE_FROM_LIST => {
			let _ = ui_tx.send(UiCommand::RemoveUserFromList(account.id.clone()));
			return;
		}
		ID_ACTION_EDIT_NOTE => {
			let current = relationship.map(|r| r.note.as_str()).unwrap_or_default();
			let Some(note) = prompt_for_note(parent, account.display_name_or_username(), current) else { return };
			let account_id = account.id.clone();
			let target_name = account.display_name_or_username().to_string();
			requests.request(
				move |client, token| client.set_account_note(token, &account_id, &note),
				move |ctx, result| note_saved(ctx, &target_name, result),
			);
			return;
		}
		ID_ACTION_SET_ALIAS => {
			let _ = ui_tx.send(UiCommand::SetUserAlias(Box::new(account.clone())));
			return;
		}
		ID_ACTION_SEND_DIRECT_MESSAGE => {
			let _ = ui_tx.send(UiCommand::SendDirectMessage(Box::new(account.clone())));
			return;
		}
		ID_ACTION_FOLLOW => (RelationshipAction::Follow, FollowOptions::default()),
		ID_ACTION_SHOW_BOOSTS => (RelationshipAction::ShowBoosts, FollowOptions { reblogs: true, ..current }),
		ID_ACTION_HIDE_BOOSTS => (RelationshipAction::HideBoosts, FollowOptions { reblogs: false, ..current }),
		ID_ACTION_ENABLE_NOTIFICATIONS => {
			(RelationshipAction::EnableNotifications, FollowOptions { notify: true, ..current })
		}
		ID_ACTION_DISABLE_NOTIFICATIONS => {
			(RelationshipAction::DisableNotifications, FollowOptions { notify: false, ..current })
		}
		ID_ACTION_UNFOLLOW if relationship.is_some_and(|r| !r.following && r.requested) => {
			(RelationshipAction::CancelFollowRequest, current)
		}
		ID_ACTION_UNFOLLOW => (RelationshipAction::Unfollow, current),
		ID_ACTION_BLOCK => {
			if !wx_utils::confirm(parent, "Are you sure you want to block this user?", "Block User") {
				return;
			}
			(RelationshipAction::Block, current)
		}
		ID_ACTION_UNBLOCK => (RelationshipAction::Unblock, current),
		ID_ACTION_REMOVE_FOLLOWER => {
			let question = format!(
				"Remove {} from your followers? They can follow you again unless you block them.",
				account.display_name_or_username()
			);
			if !wx_utils::confirm(parent, question, "Remove Follower") {
				return;
			}
			(RelationshipAction::RemoveFromFollowers, current)
		}
		ID_ACTION_MUTE => {
			let Some(mute) = prompt_for_mute(parent, account.display_name_or_username()) else { return };
			(RelationshipAction::Mute(mute), current)
		}
		ID_ACTION_UNMUTE => (RelationshipAction::Unmute, current),
		ID_ACTION_ACCEPT_FOLLOW_REQUEST => (RelationshipAction::AcceptFollowRequest, current),
		ID_ACTION_REJECT_FOLLOW_REQUEST => (RelationshipAction::RejectFollowRequest, current),
		_ => return,
	};
	let account_id = account.id.clone();
	let target_name = account.display_name_or_username().to_string();
	requests.request(
		move |client, token| change_relationship(client, token, &account_id, action, options),
		move |ctx, result| relationship_updated(ctx, &target_name, action, result),
	);
}

/// Asks for the user's private note on `name`, starting from `current`.
fn prompt_for_note(parent: &dyn WxWidget, name: &str, current: &str) -> Option<String> {
	let dialog = Dialog::builder(parent, &format!("Note on {name}")).with_size(450, 250).build();
	let panel = Panel::builder(&dialog).build();
	let label = StaticText::builder(&panel).with_label("&Private note, which only you can see:").build();
	let text = TextCtrl::builder(&panel).with_style(TextCtrlStyle::MultiLine).with_value(current).build();
	let main_sizer = BoxSizer::builder(Orientation::Vertical).build();
	main_sizer.add(&label, 0, SizerFlag::Expand | SizerFlag::All, 8);
	main_sizer.add(&text, 1, SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right, 8);
	let (ok_button, cancel_button) = build_ok_cancel_buttons_on(&panel, &dialog, "&Save");
	add_ok_cancel_footer(main_sizer, ok_button, cancel_button);
	panel.set_sizer(main_sizer, true);
	let dialog_sizer = BoxSizer::builder(Orientation::Vertical).build();
	dialog_sizer.add(&panel, 1, SizerFlag::Expand, 0);
	dialog.set_sizer(dialog_sizer, true);
	dialog.centre();
	text.set_focus();
	text.set_insertion_point_end();
	let result = dialog.show_modal();
	let note = text.get_value().replace("\r\n", "\n");
	dialog.destroy();
	(result == ID_OK).then_some(note)
}

/// Asks how long to mute `name` for, and whether to hide their notifications as well.
fn prompt_for_mute(parent: &dyn WxWidget, name: &str) -> Option<MuteOptions> {
	let dialog = Dialog::builder(parent, &format!("Mute {name}")).build();
	let panel = Panel::builder(&dialog).build();
	let duration_label = StaticText::builder(&panel).with_label("&Duration:").build();
	let durations = MUTE_DURATIONS.iter().map(|(_, label)| (*label).to_string()).collect();
	let duration = Choice::builder(&panel).with_choices(durations).build();
	duration.set_selection(0);
	let notifications = CheckBox::builder(&panel).with_label("Also hide &notifications from this user").build();
	notifications.set_value(true);
	let row = BoxSizer::builder(Orientation::Horizontal).build();
	row.add(&duration_label, 0, SizerFlag::AlignCenterVertical | SizerFlag::Right, 8);
	row.add(&duration, 1, SizerFlag::Expand, 0);
	let main_sizer = BoxSizer::builder(Orientation::Vertical).build();
	main_sizer.add_sizer(&row, 0, SizerFlag::Expand | SizerFlag::All, 8);
	main_sizer.add(&notifications, 0, SizerFlag::Expand | SizerFlag::All, 8);
	let (ok_button, cancel_button) = build_ok_cancel_buttons_on(&panel, &dialog, "&Mute");
	add_ok_cancel_footer(main_sizer, ok_button, cancel_button);
	panel.set_sizer(main_sizer, true);
	let dialog_sizer = BoxSizer::builder(Orientation::Vertical).build();
	dialog_sizer.add(&panel, 1, SizerFlag::Expand, 0);
	dialog.set_sizer_and_fit(dialog_sizer, true);
	dialog.centre();
	duration.set_focus();
	let result = dialog.show_modal();
	let chosen = duration.get_selection().and_then(|i| MUTE_DURATIONS.get(i as usize)).map_or(0, |(secs, _)| *secs);
	let mute = MuteOptions { duration: chosen, notifications: notifications.get_value() };
	dialog.destroy();
	(result == ID_OK).then_some(mute)
}

pub fn setup_actions_button(
	panel: Panel,
	button: Button,
	account: Rc<RefCell<Account>>,
	relationship: Rc<RefCell<Option<Relationship>>>,
	requests: Requests,
	ui_tx: UiCommandSender,
) {
	let relationship_click = relationship.clone();
	button.on_click(move |_| {
		let mut menu = build_actions_menu(relationship_click.borrow().as_ref());
		panel.popup_menu(&mut menu, None);
	});
	panel.on_menu_selected(move |event| {
		// Copied out so nothing stays borrowed while the block confirmation runs its own event loop.
		let account = account.borrow().clone();
		let relationship = relationship.borrow().clone();
		handle_action(event.get_id(), &account, relationship.as_ref(), &panel, &requests, &ui_tx);
	});
}
