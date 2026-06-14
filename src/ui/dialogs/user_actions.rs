use std::{cell::RefCell, fmt::Write, rc::Rc, sync::mpsc::Sender};

use wxdragon::prelude::*;

use crate::{
	mastodon::{Account, Relationship},
	network::NetworkCommand,
};

pub(crate) const ID_ACTION_FOLLOW: i32 = 6001;
pub(crate) const ID_ACTION_UNFOLLOW: i32 = 6002;
pub(crate) const ID_ACTION_BLOCK: i32 = 6003;
pub(crate) const ID_ACTION_UNBLOCK: i32 = 6004;
pub(crate) const ID_ACTION_MUTE: i32 = 6005;
pub(crate) const ID_ACTION_UNMUTE: i32 = 6006;
pub(crate) const ID_ACTION_OPEN_BROWSER: i32 = 6007;
pub(crate) const ID_ACTION_SHOW_BOOSTS: i32 = 6008;
pub(crate) const ID_ACTION_HIDE_BOOSTS: i32 = 6009;
pub(crate) const ID_ACTION_VIEW_FOLLOWERS: i32 = 6010;
pub(crate) const ID_ACTION_VIEW_FOLLOWING: i32 = 6011;
pub(crate) const ID_ACTION_ACCEPT_FOLLOW_REQUEST: i32 = 6012;
pub(crate) const ID_ACTION_REJECT_FOLLOW_REQUEST: i32 = 6013;
pub(crate) const ID_ACTION_ADD_TO_LIST: i32 = 6014;

pub(crate) fn append_relationship_text(text: &mut String, relationship: &Relationship, is_own_account: bool) {
	text.push_str("\r\n\r\nRelationship:\r\n");
	if !is_own_account {
		let follow_status = match (relationship.following, relationship.followed_by) {
			(true, true) => "You follow each other.",
			(true, false) => "You follow this person.",
			(false, true) => "This person follows you.",
			(false, false) => "You do not follow each other.",
		};
		let _ = writeln!(text, "{follow_status}");
	}
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

pub(crate) fn setup_actions_button(
	panel: Panel,
	button: Button,
	account: Rc<RefCell<Account>>,
	relationship: Rc<RefCell<Option<Relationship>>>,
	net_tx: Sender<NetworkCommand>,
	ui_tx: crate::ui_wake::UiCommandSender,
) {
	let relationship_click = relationship.clone();
	button.on_click(move |_| {
		let mut menu = Menu::builder().build();
		{
			let rel = relationship_click.borrow();
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
		menu.append_separator();
		menu.append(ID_ACTION_ADD_TO_LIST, "Add to List...", "", ItemKind::Normal);
		panel.popup_menu(&mut menu, None);
	});

	panel.on_menu_selected(move |event| {
		let id = event.get_id();
		let account = account.borrow();
		let account_id = account.id.clone();
		let target_name = account.display_name_or_username().to_string();
		if id == ID_ACTION_OPEN_BROWSER {
			let _ = wxdragon::utils::launch_default_browser(&account.url, wxdragon::utils::BrowserLaunchFlags::Default);
			return;
		}
		if id == ID_ACTION_VIEW_FOLLOWERS {
			let acct = account.acct.clone();
			let total_count = account.followers_count;
			let _ = net_tx.send(NetworkCommand::FetchFollowers { account_id, acct, total_count });
			return;
		}
		if id == ID_ACTION_VIEW_FOLLOWING {
			let acct = account.acct.clone();
			let total_count = account.following_count;
			let _ = net_tx.send(NetworkCommand::FetchFollowing { account_id, acct, total_count });
			return;
		}
		if id == ID_ACTION_ADD_TO_LIST {
			let _ = ui_tx.send(crate::commands::UiCommand::AddUserToList(account_id));
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
				action: if relationship.borrow().as_ref().is_some_and(|r| !r.following && r.requested) {
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
				let confirm = MessageDialog::builder(&panel, "Are you sure you want to block this user?", "Block User")
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
	});
}
