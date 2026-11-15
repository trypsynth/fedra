//! Commands that act on users, profiles, and hashtags.

use anyhow::Result;
use wx_utils::tasks::spawn_for;
use wxdragon::prelude::*;

use super::{
	UiCommand, UiCommandContext,
	post::post_result_to_data,
	selection::{acct_from_mention_link, foreign_url, get_selected_entry, get_selected_status},
	timeline::{open_timeline, refresh_active_timeline},
};
use crate::{
	html,
	mastodon::{Account, MastodonClient, Status},
	network::{self, AccountQuery, Session, fetch_tags},
	responses::{fill_in_profile, lists, look_up, relationship_updated, spoken_failure, statuses, tags_fetched},
	timeline::{TimelineEntry, TimelineType},
	ui::{
		dialogs::{self, FollowList},
		timeline_list::TimelineList,
	},
};

pub(super) fn view_profile(ctx: &mut UiCommandContext<'_>) {
	lookup_involved_account(ctx, dialogs::UserLookupAction::Profile);
}

pub(super) fn open_user_timeline(ctx: &mut UiCommandContext<'_>) {
	lookup_involved_account(ctx, dialogs::UserLookupAction::Timeline);
}

/// Opens someone the selected item is about, asking which one when there's more than one.
fn lookup_involved_account(ctx: &mut UiCommandContext<'_>, action: dialogs::UserLookupAction) {
	let requests = ctx.requests();
	let state = &mut *ctx.state;
	let frame = ctx.frame;
	let live_region = ctx.live_region;
	let ui_tx = ctx.ui_tx;
	let Some(entry) = get_selected_entry(state) else {
		live_region.announce("No item selected");
		return;
	};
	let mut involved: Vec<(Account, String)> = involved_accounts(entry)
		.into_iter()
		.map(|(account, roles)| {
			let label = format!("{} ({})", account.display_name_or_username(), roles.join(", "));
			(account.clone(), label)
		})
		.collect();
	// You're rarely who you're looking for, as when someone boosts or quotes your post.
	if involved.len() > 1 {
		involved.retain(|(account, _)| state.current_user_id.as_ref() != Some(&account.id));
	}
	let (account, action) = match involved.as_slice() {
		[] => {
			live_region.announce("No users for this item");
			return;
		}
		[(account, _)] => (account.clone(), action),
		_ => {
			let accounts: Vec<&Account> = involved.iter().map(|(account, _)| account).collect();
			let labels: Vec<&str> = involved.iter().map(|(_, label)| label.as_str()).collect();
			let picked = match action {
				dialogs::UserLookupAction::Profile => dialogs::prompt_for_account_selection(frame, &accounts, &labels),
				dialogs::UserLookupAction::Timeline => {
					dialogs::prompt_for_account_choice(frame, &accounts, &labels).map(|account| (account, action))
				}
			};
			let Some(picked) = picked else { return };
			picked
		}
	};
	if let Some(url) = foreign_url(state, Some(&account.url)) {
		if let Some(requests) = &requests {
			look_up(requests, AccountQuery::Url(url), action);
		}
		return;
	}
	let timeline_type =
		TimelineType::User { id: account.id.clone(), name: account.display_name_or_username().to_string() };
	match action {
		dialogs::UserLookupAction::Profile => {
			let Some(requests) = requests else {
				live_region.announce("Network not available");
				return;
			};
			fill_in_profile(&requests, &account.id, true);
			let ui_tx_timeline = ui_tx.clone();
			let ui_tx_close = ui_tx.clone();
			let dlg = dialogs::ProfileDialog::new(
				frame,
				account,
				state.current_user_id.as_deref(),
				requests,
				ui_tx.clone(),
				move || {
					let _ = ui_tx_timeline.send(UiCommand::OpenTimeline(timeline_type.clone()));
				},
				move || {
					let _ = ui_tx_close.send(UiCommand::ProfileDialogClosed);
				},
			);
			dlg.show();
			state.profile_dialog = Some(dlg);
		}
		dialogs::UserLookupAction::Timeline => {
			open_timeline(
				state,
				ctx.timelines_selector,
				&ctx.timeline_list,
				&timeline_type,
				ctx.suppress_selection,
				live_region,
				frame,
			);
		}
	}
}

/// Everyone an item is about, in reading order, with the parts they play in it. Someone in
/// several parts, like a person quoting their own post, is listed once with all of them.
fn involved_accounts(entry: &TimelineEntry) -> Vec<(&Account, Vec<&'static str>)> {
	let mut involved = Vec::new();
	match entry {
		TimelineEntry::Status(status) => add_status_accounts(&mut involved, status),
		TimelineEntry::Notification(notification) => {
			let role = match notification.kind.as_str() {
				"mention" | "status" | "quote" | "update" | "poll" => "author",
				"reblog" => "booster",
				"favourite" => "favoriter",
				"follow" | "follow_request" => "follower",
				"admin.sign_up" => "new user",
				"admin.report" => "reporter",
				_ => "notifier",
			};
			add_role(&mut involved, &notification.account, role);
			if let Some(status) = &notification.status {
				add_status_accounts(&mut involved, status);
			}
			if let Some(target) = notification.report.as_ref().and_then(|report| report.target_account.as_ref()) {
				add_role(&mut involved, target, "reported");
			}
		}
		TimelineEntry::Account(account) => add_role(&mut involved, account, "account"),
		TimelineEntry::Hashtag(_) => {}
	}
	involved
}

fn add_status_accounts<'a>(involved: &mut Vec<(&'a Account, Vec<&'static str>)>, status: &'a Status) {
	if status.reblog.is_some() {
		add_role(involved, &status.account, "booster");
	}
	let status = status.reblog.as_deref().unwrap_or(status);
	add_role(involved, &status.account, "author");
	let mut role = "quoted";
	let mut quoted = status.quote.as_ref().and_then(|quote| quote.quoted_status.as_deref());
	while let Some(quoted_status) = quoted {
		add_role(involved, &quoted_status.account, role);
		role = "quoted in the quote";
		quoted = quoted_status.quote.as_ref().and_then(|quote| quote.quoted_status.as_deref());
	}
}

fn add_role<'a>(involved: &mut Vec<(&'a Account, Vec<&'static str>)>, account: &'a Account, role: &'static str) {
	match involved.iter_mut().find(|(existing, _)| existing.id == account.id) {
		Some((_, roles)) if !roles.contains(&role) => roles.push(role),
		Some(_) => {}
		None => involved.push((account, vec![role])),
	}
}

pub(super) fn open_user_timeline_by_input(ctx: &mut UiCommandContext<'_>) {
	let requests = ctx.requests();
	let state = &mut *ctx.state;
	let frame = ctx.frame;
	let live_region = ctx.live_region;
	let suggestions: Vec<String> =
		dialogs::timeline_people(state).iter().map(|person| format!("@{}", person.acct)).collect();
	let default_value = get_selected_entry(state).and_then(|entry| match entry {
		TimelineEntry::Status(status) => Some(status.account.full_acct()),
		TimelineEntry::Notification(notification) => Some(notification.account.full_acct()),
		TimelineEntry::Account(account) => Some(account.full_acct()),
		TimelineEntry::Hashtag(_) => None,
	});
	let default_value = default_value.map(|acct| format!("@{acct}"));
	if let Some((input, action)) = dialogs::prompt_for_user_lookup(frame, &suggestions, default_value.as_deref()) {
		let handle: String = input.chars().filter(|c| !c.is_whitespace()).collect();
		if let Some(requests) = requests {
			look_up(&requests, AccountQuery::Handle(handle), action);
		} else {
			live_region.announce("Network not available");
		}
	}
}

pub(super) fn view_mentions(ctx: &mut UiCommandContext<'_>) {
	let requests = ctx.requests();
	let state = &mut *ctx.state;
	let frame = ctx.frame;
	let live_region = ctx.live_region;
	let Some(status) = get_selected_status(state) else {
		live_region.announce("No post selected");
		return;
	};
	let target = status.reblog.as_ref().map_or(status, std::convert::AsRef::as_ref);
	// Start with mentions the API resolved, then add any found only in the post HTML.
	// The API omits mentions for accounts the local instance hasn't federated yet
	// (e.g. brand-new or recently migrated accounts), so we need the HTML fallback.
	let mut all_mentions: Vec<crate::mastodon::Mention> = target.mentions.clone();
	for (url, text) in html::extract_mention_links(&target.content) {
		if all_mentions.iter().any(|m| m.url == url) {
			continue;
		}
		let acct = acct_from_mention_link(&text, &url);
		let username = acct.split('@').next().unwrap_or("").to_string();
		all_mentions.push(crate::mastodon::Mention {
			id: String::new(), // unknown to local instance; use lookup_account fallback
			username,
			acct,
			url,
		});
	}
	if all_mentions.is_empty() {
		live_region.announce("No mentions in this post");
		return;
	}
	if let Some((mention, action)) = dialogs::prompt_for_mentions(frame, &all_mentions) {
		let Some(requests) = requests else {
			live_region.announce("Network not available");
			return;
		};
		let query = foreign_url(state, Some(&mention.url))
			.map_or_else(|| AccountQuery::Handle(mention.full_acct()), AccountQuery::Url);
		look_up(&requests, query, action);
	}
}

pub(super) fn view_hashtags(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	let live_region = ctx.live_region;
	let Some(status) = get_selected_status(state) else {
		live_region.announce("No post selected");
		return;
	};
	let target = status.reblog.as_ref().map_or(status, std::convert::AsRef::as_ref);
	if target.tags.is_empty() {
		live_region.announce("No hashtags in this post");
		return;
	}
	let names: Vec<String> = target.tags.iter().map(|t| t.name.clone()).collect();
	let Some(session) = state.session() else {
		live_region.announce("Network not available");
		return;
	};
	ctx.handles.request(&session, move |client, token| fetch_tags(client, token, &names), tags_fetched);
}

pub(super) fn view_boosts(ctx: &mut UiCommandContext<'_>) {
	let Some(status) = get_selected_status(ctx.state) else {
		ctx.live_region.announce("No post selected");
		return;
	};
	let target = status.reblog.as_ref().map_or(status, AsRef::as_ref);
	if target.reblogs_count == 0 {
		ctx.live_region.announce("No boosts for this post");
		return;
	}
	let status_id = target.id.clone();
	let title = ("Boosts", "Users who boosted this post", "Failed to load boosts");
	pick_account(ctx, title, move |client, token| client.get_reblogged_by(token, &status_id));
}

pub(super) fn view_favorites(ctx: &mut UiCommandContext<'_>) {
	let Some(status) = get_selected_status(ctx.state) else {
		ctx.live_region.announce("No post selected");
		return;
	};
	let target = status.reblog.as_ref().map_or(status, AsRef::as_ref);
	if target.favourites_count == 0 {
		ctx.live_region.announce("No favorites for this post");
		return;
	}
	let status_id = target.id.clone();
	let title = ("Favorites", "Users who favorited this post", "Failed to load favorites");
	pick_account(ctx, title, move |client, token| client.get_favourited_by(token, &status_id));
}

/// Fetches a list of accounts and lets the user pick one to open. `wording` is the dialog's
/// title and label, and what to say if the list can't be fetched.
fn pick_account<F>(ctx: &UiCommandContext<'_>, wording: (&'static str, &'static str, &'static str), fetch: F)
where
	F: FnOnce(&MastodonClient, &str) -> Result<Vec<Account>> + Send + 'static,
{
	let Some(session) = ctx.state.session() else {
		ctx.live_region.announce("Network not available");
		return;
	};
	let (title, label, failure) = wording;
	let accounts = session.call(fetch);
	let frame = *ctx.frame;
	let live_region = ctx.live_region.clone();
	let ui_tx = ctx.ui_tx.clone();
	spawn_for(&frame, async move {
		match accounts.await {
			Ok(accounts) => {
				if let Some((account, action)) = dialogs::prompt_for_account_list(&frame, title, label, &accounts) {
					let _ = ui_tx.send(UiCommand::OpenAccount { account: Box::new(account), action });
				}
			}
			Err(err) => live_region.announce(&spoken_failure(failure, &err)),
		}
	});
}

/// Opens a dialog that fills with `account`'s followers or follows a page at a time. It appears
/// once the first page is in, and closing it stops the loading.
pub(super) fn show_follow_list(ctx: &mut UiCommandContext<'_>, account: &Account, list: FollowList) {
	let (Some(session), Some(requests)) = (ctx.state.session(), ctx.requests()) else {
		ctx.live_region.announce("Network not available");
		return;
	};
	let profile_dialog = ctx.state.profile_dialog.as_ref().map(dialogs::ProfileDialog::dialog_handle);
	let parent: &dyn WxWidget = profile_dialog.as_ref().map_or(ctx.frame as _, |d| d as _);
	let ui_tx_timeline = ctx.ui_tx.clone();
	let ui_tx_close = ctx.ui_tx.clone();
	let dlg = dialogs::FollowListDialog::new(
		parent,
		list.title(),
		list.label(),
		list.total(account),
		requests,
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
	*list.dialog_slot(ctx.state) = Some(dlg.clone());
	spawn_for(&dlg.dialog(), load_follow_list(dlg, session, account.clone(), list, ctx.live_region.clone()));
}

async fn load_follow_list(
	dlg: dialogs::FollowListDialog,
	session: Session,
	account: Account,
	list: FollowList,
	live_region: TimelineList,
) {
	let mut max_id: Option<String> = None;
	loop {
		let (page_account, page_max_id) = (account.clone(), max_id.clone());
		let page =
			session.call(move |client, token| list.fetch_page(client, token, &page_account, page_max_id.as_deref()));
		let first_page = max_id.is_none();
		let (accounts, next_max_id) = match page.await {
			Ok(page) => page,
			Err(err) if first_page => {
				live_region.announce(&spoken_failure(list.load_failure(), &err));
				dlg.dialog().close(true);
				return;
			}
			Err(err) => {
				dlg.mark_loaded();
				live_region.announce(&spoken_failure(list.page_failure(), &err));
				return;
			}
		};
		if first_page {
			if accounts.is_empty() && next_max_id.is_none() {
				live_region.announce(list.empty_message());
				dlg.dialog().close(true);
				return;
			}
			dlg.show();
		}
		dlg.append_accounts(&accounts);
		if !accounts.is_empty() {
			// Filled in alongside the next page rather than before it.
			let account_ids: Vec<String> = accounts.iter().map(|a| a.id.clone()).collect();
			let relationships = session.call(move |client, token| client.get_relationships(token, &account_ids));
			let dlg = dlg.clone();
			spawn_for(&dlg.dialog(), async move {
				if let Ok(relationships) = relationships.await {
					dlg.update_relationships(&relationships);
				}
			});
		}
		match next_max_id {
			Some(next) if !accounts.is_empty() => max_id = Some(next),
			_ => break,
		}
	}
	dlg.mark_loaded();
}

pub(super) fn hashtag_dialog_closed(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	if let Some(dialog) = state.hashtag_dialog.take() {
		dialog.destroy();
	}
}

pub(super) fn profile_dialog_closed(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	if let Some(dialog) = state.profile_dialog.take() {
		dialog.destroy();
	}
}

pub(super) fn followers_dialog_closed(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	if let Some(dialog) = state.followers_dialog.take() {
		dialog.destroy();
	}
}

pub(super) fn following_dialog_closed(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	if let Some(dialog) = state.following_dialog.take() {
		dialog.destroy();
	}
}

pub(super) fn toggle_follow(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	let frame = ctx.frame;
	let live_region = ctx.live_region;
	let Some(status) = get_selected_status(state) else {
		live_region.announce("No post selected");
		return;
	};
	let target = status.reblog.as_ref().map_or(status, std::convert::AsRef::as_ref);
	let mut all_users: Vec<crate::mastodon::Account> = Vec::new();
	if status.reblog.is_some() && status.account.id != target.account.id {
		all_users.push(status.account.clone());
	}
	all_users.push(target.account.clone());
	let mut all_mentions: Vec<crate::mastodon::Mention> = target.mentions.clone();
	for (url, text) in crate::html::extract_mention_links(&target.content) {
		if all_mentions.iter().any(|m| m.url == url) {
			continue;
		}
		let acct = acct_from_mention_link(&text, &url);
		let username = acct.split('@').next().unwrap_or("").to_string();
		all_mentions.push(crate::mastodon::Mention { id: String::new(), username, acct, url });
	}
	for mention in all_mentions {
		if !all_users.iter().any(|u| u.acct == mention.full_acct()) {
			all_users.push(crate::mastodon::Account {
				id: mention.id.clone(),
				username: mention.username.clone(),
				acct: mention.full_acct(),
				display_name: String::new(),
				url: mention.url,
				note: String::new(),
				followers_count: 0,
				following_count: 0,
				statuses_count: 0,
				fields: Vec::new(),
				created_at: String::new(),
				locked: false,
				bot: false,
				discoverable: None,
				source: None,
			});
		}
	}
	let selected_user = if all_users.len() == 1 {
		all_users[0].clone()
	} else if let Some((acc, _)) =
		dialogs::prompt_for_account_list(frame, "Select User", "Select user to follow/unfollow:", &all_users)
	{
		acc
	} else {
		return;
	};
	let Some(session) = state.session() else {
		live_region.announce("Network not available");
		return;
	};
	let account_id = Some(selected_user.id).filter(|id| !id.is_empty());
	let target_name = selected_user.username;
	ctx.handles.request(
		&session,
		move |client, token| network::toggle_follow(client, token, account_id, &selected_user.acct),
		move |ctx, (action, result)| relationship_updated(ctx, &target_name, action, result),
	);
}

pub(super) fn add_user_to_list(ctx: &mut UiCommandContext<'_>, account_id: String) {
	let Some(requests) = ctx.requests() else {
		ctx.live_region.announce("Network not available");
		return;
	};
	ctx.state.pending_add_to_list_user = Some(account_id);
	lists::fetch(&requests);
}

pub(super) fn remove_user_from_list(ctx: &mut UiCommandContext<'_>, account_id: String) {
	let Some(requests) = ctx.requests() else {
		ctx.live_region.announce("Network not available");
		return;
	};
	lists::fetch_account_lists(&requests, account_id);
}

pub(super) fn set_alias(ctx: &mut UiCommandContext<'_>, account: &Account) {
	let state = &mut *ctx.state;
	let key = account.alias_key();
	let current = state.config.user_aliases.get(&key).cloned().unwrap_or_default();
	let dialog = TextEntryDialog::builder(
		ctx.frame,
		&format!("Alias for @{}, or leave it empty to use their display name:", account.full_acct()),
		"Set Alias",
	)
	.with_default_value(&current)
	.build();
	if dialog.show_modal() != ID_OK {
		return;
	}
	let alias = dialog.get_value().unwrap_or_default().trim().to_string();
	if alias.is_empty() {
		state.config.user_aliases.remove(&key);
	} else {
		state.config.user_aliases.insert(key, alias);
	}
	if let Err(err) = state.save_config() {
		dialogs::show_error(ctx.frame, &err);
	}
	refresh_active_timeline(ctx);
}

pub(super) fn send_direct_message(ctx: &mut UiCommandContext<'_>, account: Account) {
	let requests = ctx.requests();
	let state = &mut *ctx.state;
	let frame = ctx.frame;
	let live_region = ctx.live_region;
	let (has_account, max_post_chars, poll_limits, enter_to_send) =
		(state.active_account().is_some(), state.max_post_chars, state.poll_limits.clone(), state.config.enter_to_send);
	if !has_account {
		live_region.announce("No account configured");
		return;
	}
	let Some((post, config)) = dialogs::prompt_for_direct_message(
		frame,
		&account,
		max_post_chars,
		state.max_media_description_chars,
		&poll_limits,
		enter_to_send,
		dialogs::MentionSource::from_state(state),
	) else {
		return;
	};
	if let Some(requests) = requests {
		state.pending_thread_continuation = post.continue_thread;
		state.pending_post =
			Some(crate::PendingPost { config, operation: crate::PostOperation::NewPost, last_result: post.clone() });
		statuses::post(&requests, post_result_to_data(post, None));
	} else {
		live_region.announce("Network not available");
	}
}
