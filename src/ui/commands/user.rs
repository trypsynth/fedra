//! Commands that act on users, profiles, and hashtags.

use std::rc::Rc;

use anyhow::Result;
use chrono::Local;
use wx_utils::{
	progress::{Ended, run_with_progress},
	tasks::spawn_for,
};
use wxdragon::prelude::*;

use super::{
	UiCommand, UiCommandContext, deferred,
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
	let state = &mut *ctx.state;
	let live_region = ctx.live_region;
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
	match involved.len() {
		0 => live_region.announce("No users for this item"),
		1 => {
			let (account, _) = involved.remove(0);
			open_involved_account(ctx, account, action);
		}
		_ => deferred::dialog_some(
			ctx,
			move |frame| {
				let accounts: Vec<&Account> = involved.iter().map(|(account, _)| account).collect();
				let labels: Vec<&str> = involved.iter().map(|(_, label)| label.as_str()).collect();
				match action {
					dialogs::UserLookupAction::Profile => {
						dialogs::prompt_for_account_selection(frame, &accounts, &labels)
					}
					dialogs::UserLookupAction::Timeline => {
						dialogs::prompt_for_account_choice(frame, &accounts, &labels).map(|account| (account, action))
					}
				}
			},
			|ctx, (account, action)| open_involved_account(ctx, account, action),
		),
	}
}

fn open_involved_account(ctx: &mut UiCommandContext<'_>, account: Account, action: dialogs::UserLookupAction) {
	let requests = ctx.requests();
	let state = &mut *ctx.state;
	let frame = ctx.frame;
	let live_region = ctx.live_region;
	let ui_tx = ctx.ui_tx;
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
	let state = &mut *ctx.state;
	let suggestions: Vec<String> =
		dialogs::timeline_people(state).iter().map(|person| format!("@{}", person.text)).collect();
	let default_value = get_selected_entry(state).and_then(|entry| match entry {
		TimelineEntry::Status(status) => Some(status.account.full_acct()),
		TimelineEntry::Notification(notification) => Some(notification.account.full_acct()),
		TimelineEntry::Account(account) => Some(account.full_acct()),
		TimelineEntry::Hashtag(_) => None,
	});
	let default_value = default_value.map(|acct| format!("@{acct}"));
	deferred::dialog_some(
		ctx,
		move |frame| dialogs::prompt_for_user_lookup(frame, &suggestions, default_value.as_deref()),
		|ctx, (input, action)| {
			let handle: String = input.chars().filter(|c| !c.is_whitespace()).collect();
			if let Some(requests) = ctx.requests() {
				look_up(&requests, AccountQuery::Handle(handle), action);
			} else {
				ctx.live_region.announce("Network not available");
			}
		},
	);
}

pub(super) fn view_mentions(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
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
	deferred::dialog_some(
		ctx,
		move |frame| dialogs::prompt_for_mentions(frame, &all_mentions),
		|ctx, (mention, action)| {
			let Some(requests) = ctx.requests() else {
				ctx.live_region.announce("Network not available");
				return;
			};
			let query = foreign_url(ctx.state, Some(&mention.url))
				.map_or_else(|| AccountQuery::Handle(mention.full_acct()), AccountQuery::Url);
			look_up(&requests, query, action);
		},
	);
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

/// Opens a dialog that fills with `account`'s followers or follows, or with the user's own mutes or blocks when
/// there's no `account`, a page at a time. It appears once the first page is in, and closing it stops the loading.
pub(super) fn show_follow_list(ctx: &mut UiCommandContext<'_>, account: Option<&Account>, list: FollowList) {
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
		list.label(account.is_some_and(|a| ctx.state.current_user_id.as_deref() == Some(a.id.as_str()))),
		&[],
		list.total(account),
		requests,
		ctx.ui_tx.clone(),
		move |account| {
			let timeline_type =
				TimelineType::User { id: account.id.clone(), name: account.display_name_or_username().to_string() };
			let _ = ui_tx_timeline.send(UiCommand::OpenTimeline(timeline_type));
		},
		move || {
			let _ = ui_tx_close.send(UiCommand::FollowListClosed(list));
		},
	);
	if let Some(previous) = ctx.state.follow_list_dialogs.insert(list, dlg.clone()) {
		previous.destroy();
	}
	spawn_for(&dlg.dialog(), load_follow_list(dlg, session, account.cloned(), list, ctx.live_region.clone()));
}

async fn load_follow_list(
	dlg: dialogs::FollowListDialog,
	session: Session,
	account: Option<Account>,
	list: FollowList,
	live_region: TimelineList,
) {
	let mut max_id: Option<String> = None;
	loop {
		let (page_account, page_max_id) = (account.clone(), max_id.clone());
		let page = session
			.call(move |client, token| list.fetch_page(client, token, page_account.as_ref(), page_max_id.as_deref()));
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

/// Loads all of the user's followers and follows behind a progress window, then sends the views to open.
pub(super) fn follower_analysis(ctx: &mut UiCommandContext<'_>) {
	let (Some(client), Some(token)) = (ctx.state.client.clone(), ctx.state.access_token.clone()) else {
		ctx.live_region.announce("Network not available");
		return;
	};
	let live_region = ctx.live_region.clone();
	let ui_tx = ctx.ui_tx.clone();
	run_with_progress(
		ctx.frame,
		"Follower Analysis",
		"Loading your followers...",
		move |progress| dialogs::load_follows(&client, &token, progress),
		move |loaded, ended| match loaded {
			_ if ended == Ended::Cancelled => {}
			Ok(Some((followers, following))) => {
				let analysis = dialogs::FollowAnalysis::new(&followers, &following, Local::now().date_naive());
				let _ = ui_tx.send(UiCommand::ShowFollowerAnalysis(Box::new(analysis)));
			}
			Ok(None) => {}
			Err(err) => live_region.announce(&spoken_failure(FollowList::Analysis.load_failure(), &err)),
		},
	);
}

/// Opens the follower analysis, starting on its first view.
pub(super) fn show_follower_analysis(ctx: &mut UiCommandContext<'_>, analysis: &dialogs::FollowAnalysis) {
	let (Some(session), Some(requests)) = (ctx.state.session(), ctx.requests()) else {
		ctx.live_region.announce("Network not available");
		return;
	};
	let list = FollowList::Analysis;
	let ui_tx_timeline = ctx.ui_tx.clone();
	let ui_tx_close = ctx.ui_tx.clone();
	let dlg = dialogs::FollowListDialog::new(
		ctx.frame,
		list.title(),
		list.label(true),
		&dialogs::FOLLOW_ANALYSIS_VIEWS,
		0,
		requests,
		ctx.ui_tx.clone(),
		move |account| {
			let timeline_type =
				TimelineType::User { id: account.id.clone(), name: account.display_name_or_username().to_string() };
			let _ = ui_tx_timeline.send(UiCommand::OpenTimeline(timeline_type));
		},
		move || {
			let _ = ui_tx_close.send(UiCommand::FollowListClosed(list));
		},
	);
	if let Some(previous) = ctx.state.follow_list_dialogs.insert(list, dlg.clone()) {
		previous.destroy();
	}
	let views = Rc::new(analysis.views.clone());
	let show_view = {
		let (dlg, session) = (dlg.clone(), session);
		move |index: usize| {
			let Some(accounts) = views.get(index) else { return };
			dlg.replace_accounts(accounts);
			dlg.mark_loaded();
			let ids: Vec<String> = accounts.iter().map(|a| a.id.clone()).collect();
			spawn_for(&dlg.dialog(), fill_relationships(dlg.clone(), session.clone(), dlg.missing_relationships(&ids)));
		}
	};
	show_view(0);
	dlg.on_view_changed(show_view);
	dlg.show();
}

/// Fetches the relationships for `ids`, a few at a time, so each person's details and actions fill in.
async fn fill_relationships(dlg: dialogs::FollowListDialog, session: Session, ids: Vec<String>) {
	for chunk in ids.chunks(40) {
		let chunk = chunk.to_vec();
		if let Ok(relationships) = session.call(move |client, token| client.get_relationships(token, &chunk)).await {
			dlg.update_relationships(&relationships);
		}
	}
}

/// The user's own followers or follows, without opening their profile first.
pub(super) fn show_own_follow_list(ctx: &mut UiCommandContext<'_>, list: FollowList) {
	let Some(id) = ctx.state.current_user_id.clone() else {
		ctx.live_region.announce("Your account hasn't loaded yet");
		return;
	};
	// Only the id is needed to page through a list on the user's own server.
	let account = Account { id, ..Account::default() };
	show_follow_list(ctx, Some(&account), list);
}

pub(super) fn follow_list_closed(ctx: &mut UiCommandContext<'_>, list: FollowList) {
	if let Some(dialog) = ctx.state.follow_list_dialogs.remove(&list) {
		dialog.destroy();
	}
}

pub(super) fn toggle_follow(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
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
				..crate::mastodon::Account::default()
			});
		}
	}
	if all_users.len() == 1 {
		send_toggle_follow(ctx, all_users.remove(0));
		return;
	}
	deferred::dialog_some(
		ctx,
		move |frame| {
			dialogs::prompt_for_account_list(frame, "Select User", "Select user to follow/unfollow:", &all_users)
				.map(|(account, _)| account)
		},
		send_toggle_follow,
	);
}

fn send_toggle_follow(ctx: &mut UiCommandContext<'_>, selected_user: Account) {
	let Some(requests) = ctx.requests() else {
		ctx.live_region.announce("Network not available");
		return;
	};
	let account_id = Some(selected_user.id).filter(|id| !id.is_empty());
	let target_name = selected_user.username;
	requests.request(
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
	let key = account.alias_key();
	let current = ctx.state.config.user_aliases.get(&key).cloned().unwrap_or_default();
	let message = format!("Alias for @{}, or leave it empty to use their display name:", account.full_acct());
	deferred::dialog_some(
		ctx,
		move |frame| {
			let dialog = TextEntryDialog::builder(frame, &message, "Set Alias").with_default_value(&current).build();
			if dialog.show_modal() != ID_OK {
				return None;
			}
			Some(dialog.get_value().unwrap_or_default().trim().to_string())
		},
		move |ctx, alias| {
			let state = &mut *ctx.state;
			if alias.is_empty() {
				state.config.user_aliases.remove(&key);
			} else {
				state.config.user_aliases.insert(key, alias);
			}
			if let Err(err) = state.save_config() {
				deferred::error(ctx, err);
			}
			refresh_active_timeline(ctx);
		},
	);
}

pub(super) fn send_direct_message(ctx: &mut UiCommandContext<'_>, account: Account) {
	let state = &mut *ctx.state;
	let (has_account, max_post_chars, poll_limits, compose_prefs) = (
		state.active_account().is_some(),
		state.max_post_chars,
		state.poll_limits.clone(),
		dialogs::ComposePrefs::from_config(&state.config),
	);
	if !has_account {
		ctx.live_region.announce("No account configured");
		return;
	}
	let max_media_description_chars = state.max_media_description_chars;
	let mention_source = dialogs::SuggestionSource::from_state(state);
	deferred::dialog_some(
		ctx,
		move |frame| {
			dialogs::prompt_for_direct_message(
				frame,
				&account,
				max_post_chars,
				max_media_description_chars,
				&poll_limits,
				compose_prefs,
				mention_source,
			)
		},
		|ctx, (post, config)| {
			let Some(requests) = ctx.requests() else {
				ctx.live_region.announce("Network not available");
				return;
			};
			let state = &mut *ctx.state;
			state.pending_thread_continuation = post.continue_thread;
			state.pending_post = Some(crate::PendingPost {
				config,
				operation: crate::PostOperation::NewPost,
				last_result: post.clone(),
			});
			statuses::post(&requests, post_result_to_data(post, None));
		},
	);
}
