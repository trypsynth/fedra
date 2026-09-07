use anyhow::Result;
use wxdragon::prelude::*;

use super::NetworkResponseContext;
use crate::{
	AppState, UiCommand,
	config::ConfigStore,
	mastodon::{Account, Relationship},
	network::{NetworkCommand, RelationshipAction},
	timeline::TimelineType,
	ui::dialogs::{self, UserLookupAction},
};

/// The two account lists a profile can show. They differ only in wording and in
/// which endpoint and dialog slot they use.
#[derive(Clone, Copy)]
pub(super) enum FollowList {
	Followers,
	Following,
}

impl FollowList {
	const fn is_followers(self) -> bool {
		matches!(self, Self::Followers)
	}

	const fn title(self) -> &'static str {
		match self {
			Self::Followers => "Followers",
			Self::Following => "Following",
		}
	}

	const fn label(self) -> &'static str {
		match self {
			Self::Followers => "Users who follow this person:",
			Self::Following => "Users this person follows:",
		}
	}

	const fn empty_message(self) -> &'static str {
		match self {
			Self::Followers => "No followers found",
			Self::Following => "No following found",
		}
	}

	const fn load_failure(self) -> &'static str {
		match self {
			Self::Followers => "Failed to load followers",
			Self::Following => "Failed to load following",
		}
	}

	const fn page_failure(self) -> &'static str {
		match self {
			Self::Followers => "Failed to load more followers",
			Self::Following => "Failed to load more following",
		}
	}

	const fn closed_command(self) -> UiCommand {
		match self {
			Self::Followers => UiCommand::FollowersDialogClosed,
			Self::Following => UiCommand::FollowingDialogClosed,
		}
	}

	const fn next_page_command(self, account_id: String, max_id: String) -> NetworkCommand {
		match self {
			Self::Followers => NetworkCommand::FetchNextFollowersPage { account_id, max_id },
			Self::Following => NetworkCommand::FetchNextFollowingPage { account_id, max_id },
		}
	}

	const fn dialog(self, state: &AppState) -> Option<&dialogs::FollowListDialog> {
		match self {
			Self::Followers => state.followers_dialog.as_ref(),
			Self::Following => state.following_dialog.as_ref(),
		}
	}

	const fn dialog_slot(self, state: &mut AppState) -> &mut Option<dialogs::FollowListDialog> {
		match self {
			Self::Followers => &mut state.followers_dialog,
			Self::Following => &mut state.following_dialog,
		}
	}
}

pub(super) fn lookup_succeeded(ctx: &mut NetworkResponseContext<'_>, account: &Account) {
	let action = ctx.state.pending_user_lookup_action.take().unwrap_or(UserLookupAction::Timeline);
	open_account(ctx, account, action);
}

pub(super) fn pick_from_account_list(
	ctx: &mut NetworkResponseContext<'_>,
	accounts: &[Account],
	title: &str,
	label: &str,
) {
	if let Some((account, action)) = dialogs::prompt_for_account_list(ctx.frame, title, label, accounts) {
		open_account(ctx, &account, action);
	}
}

/// Shows an account either as a profile dialog or as its own timeline.
fn open_account(ctx: &mut NetworkResponseContext<'_>, account: &Account, action: UserLookupAction) {
	let timeline_type =
		TimelineType::User { id: account.id.clone(), name: account.display_name_or_username().to_string() };
	match action {
		UserLookupAction::Timeline => ctx.dispatch(UiCommand::OpenTimeline(timeline_type)),
		UserLookupAction::Profile => {
			let Some(net) = &ctx.state.network_handle else {
				ctx.announce("Network not available");
				return;
			};
			net.send(NetworkCommand::FetchRelationship { account_id: account.id.clone() });
			let net_tx = net.command_tx.clone();
			let ui_tx_timeline = ctx.ui_tx.clone();
			let ui_tx_close = ctx.ui_tx.clone();
			let dlg = dialogs::ProfileDialog::new(
				ctx.frame,
				account.clone(),
				ctx.state.current_user_id.as_deref(),
				net_tx,
				ctx.ui_tx.clone(),
				move || {
					let _ = ui_tx_timeline.send(UiCommand::OpenTimeline(timeline_type.clone()));
				},
				move || {
					let _ = ui_tx_close.send(UiCommand::ProfileDialogClosed);
				},
			);
			dlg.show();
			ctx.state.profile_dialog = Some(dlg);
		}
	}
}

pub(super) fn follow_list_loaded(
	ctx: &mut NetworkResponseContext<'_>,
	list: FollowList,
	result: Result<(Vec<Account>, Option<String>)>,
	total_count: u64,
	account_id: String,
) {
	let (accounts, next_max_id) = match result {
		Ok(page) => page,
		Err(err) => {
			ctx.announce_failure(list.load_failure(), &err);
			return;
		}
	};
	if accounts.is_empty() && next_max_id.is_none() {
		ctx.announce(list.empty_message());
		return;
	}
	let Some(net_tx) = ctx.state.network_handle.as_ref().map(|h| h.command_tx.clone()) else { return };
	let ui_tx_timeline = ctx.ui_tx.clone();
	let ui_tx_close = ctx.ui_tx.clone();
	let account_id_opt = next_max_id.as_ref().map(|_| account_id.clone());
	let profile_dlg_handle = ctx.state.profile_dialog.as_ref().map(|pd| pd.dialog_handle());
	let parent: &dyn WxWidget = profile_dlg_handle.as_ref().map_or(ctx.frame as _, |d| d as _);
	let dlg = dialogs::FollowListDialog::new(
		parent,
		list.title(),
		list.label(),
		&accounts,
		total_count,
		account_id_opt,
		net_tx,
		ctx.ui_tx.clone(),
		move |account| {
			let timeline_type =
				TimelineType::User { id: account.id.clone(), name: account.display_name_or_username().to_string() };
			let _ = ui_tx_timeline.send(UiCommand::OpenTimeline(timeline_type));
		},
		move || {
			let _ = ui_tx_close.send(list.closed_command());
		},
	);
	dlg.show();
	if next_max_id.is_none() {
		dlg.mark_loaded();
	}
	let account_ids: Vec<String> = accounts.iter().map(|a| a.id.clone()).collect();
	*list.dialog_slot(ctx.state) = Some(dlg);
	if let Some(h) = &ctx.state.network_handle {
		let _ = h.send(NetworkCommand::FetchRelationshipsForList { account_ids, for_followers: list.is_followers() });
		if let Some(max_id) = next_max_id {
			let _ = h.send(list.next_page_command(account_id, max_id));
		}
	}
}

pub(super) fn follow_list_next_page(
	ctx: &mut NetworkResponseContext<'_>,
	list: FollowList,
	result: Result<(Vec<Account>, Option<String>)>,
) {
	let (accounts, next_max_id) = match result {
		Ok(page) => page,
		Err(err) => {
			if let Some(dlg) = list.dialog(ctx.state) {
				dlg.mark_loaded();
			}
			ctx.announce_failure(list.page_failure(), &err);
			return;
		}
	};
	let next_page = if let Some(dlg) = list.dialog(ctx.state) {
		if !accounts.is_empty() {
			dlg.append_accounts(&accounts);
		}
		if accounts.is_empty() || next_max_id.is_none() {
			dlg.mark_loaded();
			None
		} else {
			dlg.account_id.as_ref().map(|id| (id.clone(), next_max_id.unwrap()))
		}
	} else {
		None
	};
	if let Some(h) = &ctx.state.network_handle {
		if !accounts.is_empty() {
			let account_ids = accounts.iter().map(|a| a.id.clone()).collect();
			let _ =
				h.send(NetworkCommand::FetchRelationshipsForList { account_ids, for_followers: list.is_followers() });
		}
		if let Some((account_id, max_id)) = next_page {
			let _ = h.send(list.next_page_command(account_id, max_id));
		}
	}
}

pub(super) fn relationship_updated(
	ctx: &mut NetworkResponseContext<'_>,
	target_name: &str,
	action: RelationshipAction,
	result: Result<Relationship>,
) {
	let rel = match result {
		Ok(rel) => rel,
		Err(err) => {
			ctx.announce_failure("Failed to update relationship", &err);
			return;
		}
	};
	if let Some(dlg) = &ctx.state.profile_dialog {
		dlg.update_relationship(&rel);
	}
	if let Some(dlg) = &ctx.state.followers_dialog {
		dlg.update_relationships(std::slice::from_ref(&rel));
	}
	if let Some(dlg) = &ctx.state.following_dialog {
		dlg.update_relationships(std::slice::from_ref(&rel));
	}
	ctx.announce(&relationship_message(action, target_name));
}

fn relationship_message(action: RelationshipAction, target_name: &str) -> String {
	match action {
		RelationshipAction::Follow => format!("Followed {target_name}"),
		RelationshipAction::Unfollow => format!("Unfollowed {target_name}"),
		RelationshipAction::CancelFollowRequest => format!("Canceled follow request to {target_name}"),
		RelationshipAction::AcceptFollowRequest => format!("Accepted follow request from {target_name}"),
		RelationshipAction::RejectFollowRequest => format!("Rejected follow request from {target_name}"),
		RelationshipAction::Block => format!("Blocked {target_name}"),
		RelationshipAction::Unblock => format!("Unblocked {target_name}"),
		RelationshipAction::Mute => format!("Muted {target_name}"),
		RelationshipAction::Unmute => format!("Unmuted {target_name}"),
		RelationshipAction::ShowBoosts => format!("Showing boosts from {target_name}"),
		RelationshipAction::HideBoosts => format!("Hiding boosts from {target_name}"),
	}
}

pub(super) fn credentials_fetched(ctx: &mut NetworkResponseContext<'_>, account: &Account) {
	if let Some(update) = dialogs::show_profile_edit_dialog(ctx.frame, account)
		&& let Some(handle) = &ctx.state.network_handle
	{
		handle.send(NetworkCommand::UpdateProfile { update });
	}
}

pub(super) fn profile_updated(ctx: &mut NetworkResponseContext<'_>, account: Account) {
	ctx.announce("Profile updated");
	if let Some(active) = ctx.state.active_account_mut() {
		active.default_post_visibility = account.source.and_then(|s| s.privacy);
	}
	let _ = ConfigStore::new().save(&ctx.state.config);
	let _ = ctx.ui_tx.send(UiCommand::Refresh);
}
