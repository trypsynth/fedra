use anyhow::Result;
use url::Url;

use super::{NetworkResponseContext, helpers::summarize_api_error};
use crate::{
	AppState, UiCommand,
	config::InstanceLimits,
	mastodon::{Account, InstanceInfo, MastodonClient, Relationship},
	network::{
		AccountQuery, MUTE_DURATIONS, RelationshipAction, fetch_credentials, fetch_relationship, find_account,
		update_profile,
	},
	notifications::notify_message_requests,
	read_position::apply_markers,
	timeline::TimelineType,
	ui::{
		commands::{Requests, deferred},
		dialogs::{self, UserLookupAction},
	},
};

/// Fetches what the account's instance allows, such as how long a post can be, and where it
/// streams from.
pub fn fetch_instance_info(requests: &Requests) {
	requests.request(
		|client, _| client.get_instance_info(),
		|ctx, result| {
			if let Ok(info) = result {
				instance_info_fetched(ctx.state, info);
			}
		},
	);
}

/// Fetches the account's default visibility and language, which can have changed in another app since Fedra last
/// looked.
pub fn fetch_posting_defaults(requests: &Requests) {
	requests.request(MastodonClient::verify_credentials, |ctx, result| {
		if let Ok(account) = result
			&& remember_posting_defaults(ctx.state, account)
		{
			let _ = ctx.state.save_config();
		}
	});
}

/// Keeps `account`'s posting defaults on the active account, if it's still the one they're for. Says whether they
/// changed.
fn remember_posting_defaults(state: &mut AppState, account: Account) -> bool {
	let Some(active) = state.active_account_mut().filter(|a| a.user_id.as_deref() == Some(account.id.as_str())) else {
		return false;
	};
	let source = account.source.unwrap_or_default();
	let language = Some(source.language.unwrap_or_default());
	let changed = active.default_post_visibility != source.privacy || active.default_post_language != language;
	active.default_post_visibility = source.privacy;
	active.default_post_language = language;
	changed
}

/// Fetches where the account last read up to, for timelines to restore.
pub fn fetch_markers(requests: &Requests) {
	requests.request(MastodonClient::get_markers, |ctx, result| {
		if let Ok(markers) = result {
			apply_markers(ctx.state, markers);
		}
	});
}

/// Fetches how many message requests are waiting. The first check after launch or an account
/// switch only sets the baseline, which the File menu shows, so requests that were already waiting
/// don't sound like new ones.
pub fn fetch_notification_policy(requests: &Requests) {
	// Servers before Mastodon 4.3 have no notification policy, and so no requests.
	requests.request(MastodonClient::get_notification_policy, |ctx, result| {
		let Ok(policy) = result else { return };
		let count = policy.summary.pending_requests_count;
		if ctx.state.pending_message_requests.replace(count).is_some_and(|previous| count > previous) {
			notify_message_requests(ctx.state, count);
		}
		ctx.refresh_menu_labels();
	});
}

fn instance_info_fetched(state: &mut AppState, info: InstanceInfo) {
	let limits = InstanceLimits {
		max_post_chars: info.max_post_chars,
		max_media_description_chars: info.max_media_description_chars,
		poll_limits: info.poll_limits,
		languages: info.languages,
	};
	state.max_post_chars = Some(limits.max_post_chars);
	state.max_media_description_chars = Some(limits.max_media_description_chars);
	state.poll_limits.clone_from(&limits.poll_limits);
	state.instance_languages.clone_from(&limits.languages);
	if let Some(account) = state.active_account_mut()
		&& account.instance_limits.as_ref() != Some(&limits)
	{
		account.instance_limits = Some(limits);
		let _ = state.save_config();
	}
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
	let id = account_id.to_owned();
	requests.request(
		move |client, token| client.get_featured_tags(token, &id),
		|ctx, result| {
			if let (Ok(tags), Some(dlg)) = (result, &ctx.state.profile_dialog) {
				dlg.update_featured_tags(tags.into_iter().map(|tag| tag.name).collect());
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
	show_relationship(ctx, &rel);
	ctx.announce(&relationship_message(action, target_name));
}

pub fn note_saved(ctx: &NetworkResponseContext<'_>, target_name: &str, result: Result<Relationship>) {
	match result {
		Ok(rel) => {
			show_relationship(ctx, &rel);
			ctx.announce(&format!("Saved your note on {target_name}"));
		}
		Err(err) => ctx.announce_failure(&format!("Could not save your note on {target_name}"), &err),
	}
}

/// Brings any open profile or user list up to date with `rel`.
fn show_relationship(ctx: &NetworkResponseContext<'_>, rel: &Relationship) {
	if let Some(dlg) = &ctx.state.profile_dialog {
		dlg.update_relationship(rel);
	}
	for dlg in ctx.state.follow_list_dialogs.values() {
		dlg.update_relationships(std::slice::from_ref(rel));
	}
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
		RelationshipAction::Mute(_) => format!("Could not mute {target_name}"),
		RelationshipAction::Unmute => format!("Could not unmute {target_name}"),
		RelationshipAction::RemoveFromFollowers => format!("Could not remove {target_name} from your followers"),
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
		RelationshipAction::Mute(mute) => match MUTE_DURATIONS.iter().find(|(secs, _)| *secs == mute.duration) {
			Some((secs, label)) if *secs > 0 => format!("Muted {target_name} for {label}"),
			_ => format!("Muted {target_name}"),
		},
		RelationshipAction::Unmute => format!("Unmuted {target_name}"),
		RelationshipAction::RemoveFromFollowers => format!("Removed {target_name} from your followers"),
		RelationshipAction::ShowBoosts => format!("Showing boosts from {target_name}"),
		RelationshipAction::HideBoosts => format!("Hiding boosts from {target_name}"),
		RelationshipAction::EnableNotifications => format!("Turned on notifications for {target_name}"),
		RelationshipAction::DisableNotifications => format!("Turned off notifications for {target_name}"),
		RelationshipAction::ToggleFollow => format!("Toggled follow for {target_name}"),
	}
}

fn credentials_fetched(ctx: &NetworkResponseContext<'_>, account: &Account, max_profile_fields: Option<usize>) {
	let account = account.clone();
	let languages = ctx.state.instance_languages.clone();
	deferred::dialog_some(
		ctx,
		move |frame| dialogs::show_profile_edit_dialog(frame, &account, max_profile_fields, &languages),
		|ctx, update| {
			if let Some(requests) = ctx.requests() {
				requests.request(
					move |client, token| update_profile(client, token, &update),
					|ctx, result| match result {
						Ok(account) => profile_updated(ctx, account),
						Err(err) => ctx.announce_failure("Failed to update profile", &err),
					},
				);
			}
		},
	);
}

fn profile_updated(ctx: &mut NetworkResponseContext<'_>, account: Account) {
	ctx.announce("Profile updated");
	remember_posting_defaults(ctx.state, account);
	let _ = ctx.state.save_config();
	let _ = ctx.ui_tx.send(UiCommand::Refresh);
}
