use std::{cell::RefCell, fmt::Write, rc::Rc, sync::mpsc::Sender};

use wxdragon::prelude::*;

use crate::{
	mastodon::{Account, Relationship},
	network::{NetworkCommand, RelationshipAction},
	ui::commands::UiCommand,
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
	if relationship.muting {
		body.push_str("You have muted this person.\r\n");
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
	net_tx: &Sender<NetworkCommand>,
	ui_tx: &UiCommandSender,
) {
	let account_id = account.id.clone();
	let target_name = account.display_name_or_username().to_string();
	let current_reblogs = relationship.is_none_or(|r| r.showing_reblogs);
	let current_notifying = relationship.is_some_and(|r| r.notifying);
	let follow = |reblogs, notify, action| NetworkCommand::FollowAccount {
		account_id: account_id.clone(),
		target_name: target_name.clone(),
		reblogs,
		notify,
		action,
	};
	let cmd = match id {
		ID_ACTION_OPEN_BROWSER => {
			let _ = wxdragon::utils::launch_default_browser(&account.url, wxdragon::utils::BrowserLaunchFlags::Default);
			return;
		}
		ID_ACTION_VIEW_FOLLOWERS => NetworkCommand::FetchFollowers {
			account_id,
			acct: account.acct.clone(),
			total_count: account.followers_count,
		},
		ID_ACTION_VIEW_FOLLOWING => NetworkCommand::FetchFollowing {
			account_id,
			acct: account.acct.clone(),
			total_count: account.following_count,
		},
		ID_ACTION_ADD_TO_LIST => {
			let _ = ui_tx.send(UiCommand::AddUserToList(account_id));
			return;
		}
		ID_ACTION_REMOVE_FROM_LIST => {
			let _ = ui_tx.send(UiCommand::RemoveUserFromList(account_id));
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
		ID_ACTION_FOLLOW => follow(true, false, RelationshipAction::Follow),
		ID_ACTION_SHOW_BOOSTS => follow(true, current_notifying, RelationshipAction::ShowBoosts),
		ID_ACTION_HIDE_BOOSTS => follow(false, current_notifying, RelationshipAction::HideBoosts),
		ID_ACTION_ENABLE_NOTIFICATIONS => follow(current_reblogs, true, RelationshipAction::EnableNotifications),
		ID_ACTION_DISABLE_NOTIFICATIONS => follow(current_reblogs, false, RelationshipAction::DisableNotifications),
		ID_ACTION_UNFOLLOW => NetworkCommand::UnfollowAccount {
			account_id,
			target_name,
			action: if relationship.is_some_and(|r| !r.following && r.requested) {
				RelationshipAction::CancelFollowRequest
			} else {
				RelationshipAction::Unfollow
			},
		},
		ID_ACTION_BLOCK => {
			let confirm = MessageDialog::builder(parent, "Are you sure you want to block this user?", "Block User")
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
	let _ = net_tx.send(cmd);
}

pub fn setup_actions_button(
	panel: Panel,
	button: Button,
	account: Rc<RefCell<Account>>,
	relationship: Rc<RefCell<Option<Relationship>>>,
	net_tx: Sender<NetworkCommand>,
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
		handle_action(event.get_id(), &account, relationship.as_ref(), &panel, &net_tx, &ui_tx);
	});
}
