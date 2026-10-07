//! Commands that act on users, profiles, and hashtags.

use wxdragon::prelude::*;

use super::{
	UiCommand, UiCommandContext, handle_ui_command,
	post::post_result_to_data,
	selection::{acct_from_mention_link, foreign_url, get_selected_entry, get_selected_status},
	timeline::{open_timeline, refresh_active_timeline},
};
use crate::{
	html,
	mastodon::Account,
	network::NetworkCommand,
	timeline::{TimelineEntry, TimelineType},
	ui::dialogs,
};

pub(super) fn view_profile(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	let frame = ctx.frame;
	let timelines_selector = ctx.timelines_selector;
	let timeline_list = &ctx.timeline_list;
	let suppress_selection = ctx.suppress_selection;
	let live_region = ctx.live_region;
	let ui_tx = ctx.ui_tx;
	let Some(entry) = get_selected_entry(state) else {
		live_region.announce("No item selected");
		return;
	};
	let (account, action) = match entry {
		TimelineEntry::Status(status) => {
			if let Some(reblog) = &status.reblog {
				let booster = &status.account;
				let author = &reblog.account;
				if booster.id == author.id {
					(author.clone(), dialogs::UserLookupAction::Profile)
				} else {
					let accounts = [booster, author];
					let labels = [
						format!("{} (booster)", booster.display_name_or_username()),
						format!("{} (author)", author.display_name_or_username()),
					];
					let label_refs: Vec<&str> = labels.iter().map(String::as_str).collect();
					match dialogs::prompt_for_account_selection(frame, &accounts, &label_refs) {
						Some((acc, act)) => (acc, act),
						None => return,
					}
				}
			} else if let Some(quote) = &status.quote {
				if let Some(quoted_status) = &quote.quoted_status {
					let quoter = &status.account;
					let author = &quoted_status.account;
					if quoter.id == author.id {
						(author.clone(), dialogs::UserLookupAction::Profile)
					} else {
						let accounts = [quoter, author];
						let labels = [
							format!("{} (quoter)", quoter.display_name_or_username()),
							format!("{} (author)", author.display_name_or_username()),
						];
						let label_refs: Vec<&str> = labels.iter().map(String::as_str).collect();
						match dialogs::prompt_for_account_selection(frame, &accounts, &label_refs) {
							Some((acc, act)) => (acc, act),
							None => return,
						}
					}
				} else {
					(status.account.clone(), dialogs::UserLookupAction::Profile)
				}
			} else {
				(status.account.clone(), dialogs::UserLookupAction::Profile)
			}
		}
		TimelineEntry::Notification(notification) => (notification.account.clone(), dialogs::UserLookupAction::Profile),
		TimelineEntry::Account(account) => ((**account).clone(), dialogs::UserLookupAction::Profile),
		TimelineEntry::Hashtag(_) => {
			live_region.announce("Cannot view profile for a hashtag");
			return;
		}
	};
	if let Some(url) = foreign_url(state, Some(&account.url)) {
		state.pending_user_lookup_action = Some(action);
		if let Some(net) = &state.network_handle {
			net.send(NetworkCommand::ResolveAccount { url });
		}
		return;
	}
	match action {
		dialogs::UserLookupAction::Profile => {
			if let Some(net) = &state.network_handle {
				net.send(NetworkCommand::FetchRelationship { account_id: account.id.clone() });
				net.send(NetworkCommand::FetchAccount { account_id: account.id.clone() });
				let net_tx = net.command_tx.clone();
				let ui_tx_timeline = ui_tx.clone();
				let timeline_type =
					TimelineType::User { id: account.id.clone(), name: account.display_name_or_username().to_string() };
				let ui_tx_close = ui_tx.clone();
				let dlg = dialogs::ProfileDialog::new(
					frame,
					account,
					state.current_user_id.as_deref(),
					net_tx,
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
			} else {
				live_region.announce("Network not available");
			}
		}
		dialogs::UserLookupAction::Timeline => {
			let timeline_type =
				TimelineType::User { id: account.id.clone(), name: account.display_name_or_username().to_string() };
			open_timeline(
				state,
				timelines_selector,
				timeline_list,
				&timeline_type,
				suppress_selection,
				live_region,
				frame,
			);
		}
	}
}

pub(super) fn open_user_timeline(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	let frame = ctx.frame;
	let timelines_selector = ctx.timelines_selector;
	let timeline_list = &ctx.timeline_list;
	let suppress_selection = ctx.suppress_selection;
	let live_region = ctx.live_region;
	let ui_tx = ctx.ui_tx;
	let Some(entry) = get_selected_entry(state) else {
		live_region.announce("No item selected");
		return;
	};
	let (account, action) = match entry {
		TimelineEntry::Status(status) => {
			if let Some(reblog) = &status.reblog {
				let booster = &status.account;
				let author = &reblog.account;
				if booster.id == author.id {
					(author.clone(), dialogs::UserLookupAction::Timeline)
				} else {
					let accounts = [booster, author];
					let labels = [
						format!("{} (booster)", booster.display_name_or_username()),
						format!("{} (author)", author.display_name_or_username()),
					];
					let label_refs: Vec<&str> = labels.iter().map(String::as_str).collect();
					match dialogs::prompt_for_account_choice(frame, &accounts, &label_refs) {
						Some(acc) => (acc, dialogs::UserLookupAction::Timeline),
						None => return,
					}
				}
			} else if let Some(quote) = &status.quote {
				if let Some(quoted_status) = &quote.quoted_status {
					let quoter = &status.account;
					let author = &quoted_status.account;
					if quoter.id == author.id {
						(author.clone(), dialogs::UserLookupAction::Timeline)
					} else {
						let accounts = [quoter, author];
						let labels = [
							format!("{} (quoter)", quoter.display_name_or_username()),
							format!("{} (author)", author.display_name_or_username()),
						];
						let label_refs: Vec<&str> = labels.iter().map(String::as_str).collect();
						match dialogs::prompt_for_account_choice(frame, &accounts, &label_refs) {
							Some(acc) => (acc, dialogs::UserLookupAction::Timeline),
							None => return,
						}
					}
				} else {
					(status.account.clone(), dialogs::UserLookupAction::Timeline)
				}
			} else {
				(status.account.clone(), dialogs::UserLookupAction::Timeline)
			}
		}
		TimelineEntry::Notification(notification) => {
			(notification.account.clone(), dialogs::UserLookupAction::Timeline)
		}
		TimelineEntry::Account(account) => ((**account).clone(), dialogs::UserLookupAction::Timeline),
		TimelineEntry::Hashtag(_) => {
			live_region.announce("Cannot view user timeline for a hashtag");
			return;
		}
	};
	if let Some(url) = foreign_url(state, Some(&account.url)) {
		state.pending_user_lookup_action = Some(action);
		if let Some(net) = &state.network_handle {
			net.send(NetworkCommand::ResolveAccount { url });
		}
		return;
	}
	match action {
		dialogs::UserLookupAction::Profile => {
			if let Some(net) = &state.network_handle {
				net.send(NetworkCommand::FetchRelationship { account_id: account.id.clone() });
				net.send(NetworkCommand::FetchAccount { account_id: account.id.clone() });
				let net_tx = net.command_tx.clone();
				let ui_tx_timeline = ui_tx.clone();
				let timeline_type =
					TimelineType::User { id: account.id.clone(), name: account.display_name_or_username().to_string() };
				let ui_tx_close = ui_tx.clone();
				let dlg = dialogs::ProfileDialog::new(
					frame,
					account,
					state.current_user_id.as_deref(),
					net_tx,
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
			} else {
				live_region.announce("Network not available");
			}
		}
		dialogs::UserLookupAction::Timeline => {
			let timeline_type =
				TimelineType::User { id: account.id.clone(), name: account.display_name_or_username().to_string() };
			open_timeline(
				state,
				timelines_selector,
				timeline_list,
				&timeline_type,
				suppress_selection,
				live_region,
				frame,
			);
		}
	}
}

pub(super) fn open_user_timeline_by_input(ctx: &mut UiCommandContext<'_>) {
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
		if let Some(network) = &state.network_handle {
			state.pending_user_lookup_action = Some(action);
			network.send(NetworkCommand::LookupAccount { handle });
		} else {
			live_region.announce("Network not available");
		}
	}
}

pub(super) fn view_mentions(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	let frame = ctx.frame;
	let live_region = ctx.live_region;
	let ui_tx = ctx.ui_tx;
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
		if let Some(url) = foreign_url(state, Some(&mention.url)) {
			state.pending_user_lookup_action = Some(action);
			if let Some(net) = &state.network_handle {
				net.send(NetworkCommand::ResolveAccount { url });
			}
			return;
		}
		let account = if let (Some(client), Some(token)) = (&state.client, &state.access_token) {
			let by_id = if mention.id.is_empty() { None } else { client.get_account(token, &mention.id).ok() };
			by_id.or_else(|| client.lookup_account(token, &mention.full_acct()).ok())
		} else {
			None
		};
		let account = match account {
			Some(acc) => acc,
			None if mention.id.is_empty() => {
				live_region.announce(&format!("Could not resolve account @{}", mention.full_acct()));
				return;
			}
			None => crate::mastodon::Account {
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
			},
		};
		match action {
			dialogs::UserLookupAction::Profile => {
				if let Some(net) = &state.network_handle {
					net.send(NetworkCommand::FetchRelationship { account_id: account.id.clone() });
					let net_tx = net.command_tx.clone();
					let ui_tx_timeline = ui_tx.clone();
					let timeline_type = TimelineType::User {
						id: account.id.clone(),
						name: account.display_name_or_username().to_string(),
					};
					let ui_tx_close = ui_tx.clone();
					let dlg = dialogs::ProfileDialog::new(
						frame,
						account,
						state.current_user_id.as_deref(),
						net_tx,
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
				} else {
					live_region.announce("Network not available");
				}
			}
			dialogs::UserLookupAction::Timeline => {
				let timeline_type =
					TimelineType::User { id: account.id.clone(), name: account.display_name_or_username().to_string() };
				handle_ui_command(UiCommand::OpenTimeline(timeline_type), ctx);
			}
		}
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
	if let Some(handle) = &state.network_handle {
		handle.send(NetworkCommand::FetchTagsInfo { names });
	} else {
		live_region.announce("Network not available");
	}
}

pub(super) fn view_boosts(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	let live_region = ctx.live_region;
	let Some(status) = get_selected_status(state) else {
		live_region.announce("No post selected");
		return;
	};
	let target = status.reblog.as_ref().map_or(status, std::convert::AsRef::as_ref);
	if target.reblogs_count == 0 {
		live_region.announce("No boosts for this post");
		return;
	}
	if let Some(handle) = &state.network_handle {
		handle.send(NetworkCommand::FetchRebloggedBy { status_id: target.id.clone() });
	} else {
		live_region.announce("Network not available");
	}
}

pub(super) fn view_favorites(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	let live_region = ctx.live_region;
	let Some(status) = get_selected_status(state) else {
		live_region.announce("No post selected");
		return;
	};
	let target = status.reblog.as_ref().map_or(status, std::convert::AsRef::as_ref);
	if target.favourites_count == 0 {
		live_region.announce("No favorites for this post");
		return;
	}
	if let Some(handle) = &state.network_handle {
		handle.send(NetworkCommand::FetchFavoritedBy { status_id: target.id.clone() });
	} else {
		live_region.announce("Network not available");
	}
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
	if let Some(net) = &state.network_handle {
		net.send(NetworkCommand::ToggleFollow {
			account_id: if selected_user.id.is_empty() { None } else { Some(selected_user.id) },
			acct: selected_user.acct.clone(),
			target_name: selected_user.username,
		});
	} else {
		live_region.announce("Network not available");
	}
}

pub(super) fn add_user_to_list(ctx: &mut UiCommandContext<'_>, account_id: String) {
	let state = &mut *ctx.state;
	let live_region = ctx.live_region;
	state.pending_add_to_list_user = Some(account_id);
	if let Some(handle) = &state.network_handle {
		handle.send(NetworkCommand::FetchLists);
	} else {
		live_region.announce("Network not available");
	}
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
		&poll_limits,
		enter_to_send,
		dialogs::MentionSource::from_state(state),
	) else {
		return;
	};
	if let Some(handle) = &state.network_handle {
		state.pending_thread_continuation = post.continue_thread;
		state.pending_post =
			Some(crate::PendingPost { config, operation: crate::PostOperation::NewPost, last_result: post.clone() });
		handle.send(NetworkCommand::PostStatus { post: post_result_to_data(post, None) });
	} else {
		live_region.announce("Network not available");
	}
}
