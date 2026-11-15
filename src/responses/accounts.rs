use anyhow::Result;
use url::Url;

use super::{NetworkResponseContext, helpers::summarize_api_error};
use crate::{
	AppState, UiCommand,
	mastodon::{Account, InstanceInfo, Relationship},
	network::{AccountQuery, RelationshipAction, fetch_credentials, fetch_relationship, find_account, update_profile},
	timeline::TimelineType,
	ui::{
		commands::Requests,
		dialogs::{self, UserLookupAction},
	},
};

pub(super) fn instance_info_fetched(state: &mut AppState, info: InstanceInfo) {
	state.max_post_chars = Some(info.max_post_chars);
	state.max_media_description_chars = Some(info.max_media_description_chars);
	state.poll_limits = info.poll_limits;
	let streaming_url = info.streaming_url.and_then(|url| Url::parse(&url).ok());
	if streaming_url.is_some() && streaming_url != state.streaming_url {
		state.streaming_url = streaming_url;
		// Streams opened before this arrived went to the instance's own host.
		crate::accounts::apply_streaming_setting(state);
	}
}

/// Finds the account `query` names, then opens it as `action` says.
pub fn look_up(requests: &Requests, query: AccountQuery, action: UserLookupAction) {
	let asked = query.clone();
	requests.request(
		move |client, token| find_account(client, token, &asked),
		move |ctx, result| match result {
			Ok(account) => open_account(ctx, &account, action),
			Err(err) => ctx.announce(&format!("Failed to find user {query}: {}", summarize_api_error(&err))),
		},
	);
}

/// Fetches the signed-in account, then opens it for editing.
pub fn edit_profile(requests: &Requests) {
	requests.request(fetch_credentials, |ctx, result| match result {
		Ok((account, max_profile_fields)) => credentials_fetched(ctx, &account, max_profile_fields),
		Err(err) => ctx.announce_failure("Failed to fetch profile", &err),
	});
}

/// Shows an account either as a profile dialog or as its own timeline.
pub fn open_account(ctx: &mut NetworkResponseContext<'_>, account: &Account, action: UserLookupAction) {
	let timeline_type =
		TimelineType::User { id: account.id.clone(), name: account.display_name_or_username().to_string() };
	match action {
		UserLookupAction::Timeline => ctx.dispatch(UiCommand::OpenTimeline(timeline_type)),
		UserLookupAction::Profile => {
			let Some(requests) = ctx.requests() else {
				ctx.announce("Network not available");
				return;
			};
			fill_in_profile(&requests, &account.id, false);
			let ui_tx_timeline = ctx.ui_tx.clone();
			let ui_tx_close = ctx.ui_tx.clone();
			let dlg = dialogs::ProfileDialog::new(
				ctx.frame,
				account.clone(),
				ctx.state.current_user_id.as_deref(),
				requests,
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

/// Fills the open profile dialog in with the relationship to `account_id`, and with the account
/// itself when `refresh_account` says what's shown may be out of date.
pub fn fill_in_profile(requests: &Requests, account_id: &str, refresh_account: bool) {
	let id = account_id.to_owned();
	requests.request(
		move |client, token| fetch_relationship(client, token, &id),
		|ctx, result| {
			if let (Ok(relationship), Some(dlg)) = (result, &ctx.state.profile_dialog) {
				dlg.update_relationship(&relationship);
			}
		},
	);
	if refresh_account {
		let id = account_id.to_owned();
		requests.request(
			move |client, token| client.get_account(token, &id),
			|ctx, result| {
				if let (Ok(account), Some(dlg)) = (result, &ctx.state.profile_dialog) {
					dlg.update_account(&account);
				}
			},
		);
	}
}

pub fn relationship_updated(
	ctx: &NetworkResponseContext<'_>,
	target_name: &str,
	action: RelationshipAction,
	result: Result<Relationship>,
) {
	let rel = match result {
		Ok(rel) => rel,
		Err(err) => {
			ctx.announce_failure(&relationship_failure_message(action, target_name), &err);
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

fn relationship_failure_message(action: RelationshipAction, target_name: &str) -> String {
	match action {
		RelationshipAction::Follow => format!("Could not follow {target_name}"),
		RelationshipAction::Unfollow => format!("Could not unfollow {target_name}"),
		RelationshipAction::CancelFollowRequest => format!("Could not cancel the follow request to {target_name}"),
		RelationshipAction::AcceptFollowRequest => format!("Could not accept the follow request from {target_name}"),
		RelationshipAction::RejectFollowRequest => format!("Could not reject the follow request from {target_name}"),
		RelationshipAction::Block => format!("Could not block {target_name}"),
		RelationshipAction::Unblock => format!("Could not unblock {target_name}"),
		RelationshipAction::Mute => format!("Could not mute {target_name}"),
		RelationshipAction::Unmute => format!("Could not unmute {target_name}"),
		RelationshipAction::ShowBoosts => format!("Could not show boosts from {target_name}"),
		RelationshipAction::HideBoosts => format!("Could not hide boosts from {target_name}"),
		RelationshipAction::EnableNotifications => format!("Could not turn on notifications for {target_name}"),
		RelationshipAction::DisableNotifications => format!("Could not turn off notifications for {target_name}"),
		RelationshipAction::ToggleFollow => format!("Could not follow or unfollow {target_name}"),
	}
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
		RelationshipAction::EnableNotifications => format!("Turned on notifications for {target_name}"),
		RelationshipAction::DisableNotifications => format!("Turned off notifications for {target_name}"),
		RelationshipAction::ToggleFollow => format!("Toggled follow for {target_name}"),
	}
}

fn credentials_fetched(ctx: &NetworkResponseContext<'_>, account: &Account, max_profile_fields: Option<usize>) {
	if let Some(update) = dialogs::show_profile_edit_dialog(ctx.frame, account, max_profile_fields)
		&& let Some(requests) = ctx.requests()
	{
		requests.request(
			move |client, token| update_profile(client, token, &update),
			|ctx, result| match result {
				Ok(account) => profile_updated(ctx, account),
				Err(err) => ctx.announce_failure("Failed to update profile", &err),
			},
		);
	}
}

fn profile_updated(ctx: &mut NetworkResponseContext<'_>, account: Account) {
	ctx.announce("Profile updated");
	if let Some(active) = ctx.state.active_account_mut() {
		active.default_post_visibility = account.source.and_then(|s| s.privacy);
	}
	let _ = ctx.state.save_config();
	let _ = ctx.ui_tx.send(UiCommand::Refresh);
}
