//! Commands that compose, edit, or act on a single post.

use wx_utils::tasks::spawn_for;
use wxdragon::prelude::*;

use super::{
	UiCommand, UiCommandContext, deferred, handle_ui_command,
	selection::{foreign_url, get_selected_entry, get_selected_status},
};
use crate::{
	AppState,
	config::ContentWarningDisplay,
	html,
	mastodon::{MastodonClient, Status, StatusAction, friendly_time_local},
	network::{self, Target, resolve_status},
	responses::{spoken_failure, status_acted, statuses},
	timeline::{TimelineTextOptions, TimelineType},
	ui::{dialogs, timeline_view::list_index_to_entry_index},
};

pub(super) fn post_result_to_data(post: dialogs::PostResult, quoted_status_id: Option<String>) -> network::PostData {
	network::PostData {
		content: post.content,
		visibility: post.visibility.as_api_str().to_string(),
		sensitive: post.sensitive,
		spoiler_text: post.spoiler_text,
		content_type: post.content_type,
		language: post.language,
		media: post
			.media
			.into_iter()
			.map(|item| network::MediaUpload { path: item.path, description: item.description })
			.collect(),
		poll: post.poll.map(|poll| network::PollData {
			options: poll.options,
			expires_in: poll.expires_in,
			multiple: poll.multiple,
			hide_totals: poll.hide_totals,
		}),
		quoted_status_id,
		scheduled_at: post.scheduled_at,
	}
}

/// The changes an edit dialog's result makes to a post. Attachments it already had stay as they
/// are, and new ones are uploaded.
fn status_edit(post: dialogs::PostResult) -> network::StatusEdit {
	network::StatusEdit {
		content: post.content,
		sensitive: post.sensitive,
		spoiler_text: post.spoiler_text,
		language: post.language,
		media: post
			.media
			.into_iter()
			.map(|item| {
				if item.is_existing {
					network::EditMedia::Existing(item.path)
				} else {
					network::EditMedia::New(network::MediaUpload { path: item.path, description: item.description })
				}
			})
			.collect(),
		poll: post.poll.map(|poll| network::PollData {
			options: poll.options,
			expires_in: poll.expires_in,
			multiple: poll.multiple,
			hide_totals: poll.hide_totals,
		}),
	}
}

/// Where a reply or vote on `target` goes: the post itself, or, on another instance's timeline,
/// its URL, since its id there means nothing to the account's own instance.
fn reply_target(state: &AppState, target: &Status) -> Target {
	let is_foreign =
		matches!(state.timeline_manager.active().map(|t| &t.timeline_type), Some(TimelineType::InstanceLocal { .. }));
	match &target.url {
		Some(url) if is_foreign => Target::Url(url.clone()),
		_ => Target::Id(target.id.clone()),
	}
}

/// Sends the toggle `choose` picks for the selected post, or announces why it can't.
fn send_status_action(
	ctx: &UiCommandContext<'_>,
	choose: fn(&AppState, &Status) -> Result<StatusAction, &'static str>,
) {
	let state = &*ctx.state;
	let Some(status) = get_selected_status(state) else {
		ctx.live_region.announce("No post selected");
		return;
	};
	let Some(session) = state.session() else {
		ctx.live_region.announce("Network not available");
		return;
	};
	let target = status.reblog.as_deref().unwrap_or(status);
	let action = match choose(state, target) {
		Ok(action) => action,
		Err(reason) => {
			ctx.live_region.announce(reason);
			return;
		}
	};
	let foreign = foreign_url(state, target.url.as_ref());
	let status_id = target.id.clone();
	ctx.handles.request(
		&session,
		move |client, token| {
			let status_id = match foreign {
				Some(url) => resolve_status(client, token, &url)?.id,
				None => status_id,
			};
			client.status_action(token, &status_id, action).map(|acted| (status_id, acted))
		},
		move |ctx, result| status_acted(ctx, action, result),
	);
}

pub fn run_edit_post_dialog(ctx: &mut UiCommandContext<'_>, target: &Status, source_text: Option<String>) {
	let state = &mut *ctx.state;
	let max_post_chars = state.max_post_chars;
	let max_media_description_chars = state.max_media_description_chars;
	let compose_prefs = dialogs::ComposePrefs::from_config(&state.config);
	let poll_limits = state.poll_limits.clone();
	let mention_source = dialogs::SuggestionSource::from_state(state);
	let shown_target = target.clone();
	let target_id = target.id.clone();
	deferred::dialog_some(
		ctx,
		move |frame| {
			dialogs::prompt_for_edit(
				frame,
				&shown_target,
				source_text.as_deref(),
				max_post_chars,
				max_media_description_chars,
				&poll_limits,
				compose_prefs,
				mention_source,
			)
		},
		move |ctx, (edit, config)| send_edit(ctx, target_id, edit, config),
	);
}

fn send_edit(
	ctx: &mut UiCommandContext<'_>,
	target_id: String,
	edit: dialogs::PostResult,
	config: dialogs::ComposeDialogConfig,
) {
	let Some(requests) = ctx.requests() else {
		ctx.live_region.announce("Network not available");
		return;
	};
	ctx.state.pending_post = Some(crate::PendingPost {
		config,
		operation: crate::PostOperation::Edit { status_id: target_id.clone() },
		last_result: edit.clone(),
	});
	statuses::edit(&requests, target_id, status_edit(edit));
}

pub(super) fn new_post(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	let live_region = ctx.live_region;
	let (has_account, max_post_chars, poll_limits, compose_prefs) = (
		state.active_account().is_some(),
		state.max_post_chars,
		state.poll_limits.clone(),
		dialogs::ComposePrefs::from_config(&state.config),
	);
	if !has_account {
		live_region.announce("No account configured");
		return;
	}
	let default_visibility = if state.timeline_manager.active().is_some_and(|t| t.timeline_type == TimelineType::Direct)
	{
		Some(dialogs::PostVisibility::Direct)
	} else {
		state.active_account().and_then(|a| a.default_post_visibility.as_deref()).and_then(|v| match v {
			"public" => Some(dialogs::PostVisibility::Public),
			"unlisted" => Some(dialogs::PostVisibility::Unlisted),
			"private" => Some(dialogs::PostVisibility::Private),
			"direct" => Some(dialogs::PostVisibility::Direct),
			_ => None,
		})
	};
	let max_media_description_chars = state.max_media_description_chars;
	let mention_source = dialogs::SuggestionSource::from_state(state);
	deferred::dialog_some(
		ctx,
		move |frame| {
			dialogs::prompt_for_post(
				frame,
				max_post_chars,
				max_media_description_chars,
				&poll_limits,
				compose_prefs,
				default_visibility,
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

pub(super) fn continue_thread(ctx: &mut UiCommandContext<'_>, mut status: Box<Status>) {
	let state = &mut *ctx.state;
	if status.visibility == "public" {
		status.visibility = "unlisted".to_string();
	}
	let (max_post_chars, compose_prefs) = (state.max_post_chars, dialogs::ComposePrefs::from_config(&state.config));
	let max_media_description_chars = state.max_media_description_chars;
	let poll_limits = state.poll_limits.clone();
	let self_acct = state.active_account().and_then(|account| account.acct.clone());
	let mention_source = dialogs::SuggestionSource::from_state(state);
	let status_id = status.id.clone();
	deferred::dialog_some(
		ctx,
		move |frame| {
			dialogs::prompt_for_reply(
				frame,
				&status,
				max_post_chars,
				max_media_description_chars,
				&poll_limits,
				true,
				self_acct.as_deref(),
				compose_prefs,
				true,
				mention_source,
			)
		},
		move |ctx, (reply, config)| {
			let Some(requests) = ctx.requests() else {
				ctx.live_region.announce("Network not available");
				return;
			};
			let state = &mut *ctx.state;
			state.pending_thread_continuation = reply.continue_thread;
			state.pending_post = Some(crate::PendingPost {
				config,
				operation: crate::PostOperation::Reply { in_reply_to_id: status_id.clone() },
				last_result: reply.clone(),
			});
			statuses::reply(&requests, Target::Id(status_id), post_result_to_data(reply, None));
		},
	);
}

pub(super) fn reply(ctx: &mut UiCommandContext<'_>, reply_all: bool) {
	let state = &mut *ctx.state;
	let live_region = ctx.live_region;
	let (status, max_post_chars, compose_prefs) =
		(get_selected_status(state).cloned(), state.max_post_chars, dialogs::ComposePrefs::from_config(&state.config));
	let Some(status) = status else {
		live_region.announce("No post selected");
		return;
	};
	let target = status.reblog.as_deref().unwrap_or(&status).clone();
	let max_media_description_chars = state.max_media_description_chars;
	let poll_limits = state.poll_limits.clone();
	let self_acct = state.active_account().and_then(|account| account.acct.clone());
	let mention_source = dialogs::SuggestionSource::from_state(state);
	let shown_target = target.clone();
	deferred::dialog_some(
		ctx,
		move |frame| {
			dialogs::prompt_for_reply(
				frame,
				&shown_target,
				max_post_chars,
				max_media_description_chars,
				&poll_limits,
				reply_all,
				self_acct.as_deref(),
				compose_prefs,
				false,
				mention_source,
			)
		},
		move |ctx, (reply, config)| {
			let Some(requests) = ctx.requests() else {
				ctx.live_region.announce("Network not available");
				return;
			};
			let state = &mut *ctx.state;
			state.pending_thread_continuation = reply.continue_thread;
			state.pending_post = Some(crate::PendingPost {
				config,
				operation: crate::PostOperation::Reply { in_reply_to_id: target.id.clone() },
				last_result: reply.clone(),
			});
			statuses::reply(&requests, reply_target(state, &target), post_result_to_data(reply, None));
		},
	);
}

pub(super) fn quote(ctx: &mut UiCommandContext<'_>) {
	let requests = ctx.requests();
	let state = &mut *ctx.state;
	let live_region = ctx.live_region;
	let status = get_selected_status(state).cloned();
	let Some(status) = status else {
		live_region.announce("No post selected");
		return;
	};
	let target = status.reblog.as_ref().map_or(&status, std::convert::AsRef::as_ref);
	if target.visibility == "direct" {
		live_region.announce("Cannot quote direct messages");
		return;
	}
	if let Some(url) = foreign_url(state, target.url.as_ref()) {
		if let Some(requests) = requests {
			statuses::resolve_for_quote(&requests, url);
		} else {
			live_region.announce("Network not available");
		}
		return;
	}
	handle_ui_command(UiCommand::PromptForQuote(Box::new(target.clone())), ctx);
}

pub(super) fn prompt_for_quote(ctx: &mut UiCommandContext<'_>, target: Box<Status>) {
	let state = &mut *ctx.state;
	if let Some(approval) = &target.quote_approval
		&& approval.current_user.as_deref() == Some("denied")
	{
		ctx.live_region.announce("You are not allowed to quote this post");
		return;
	}
	let target_id = target.id.clone();
	let max_post_chars = state.max_post_chars;
	let max_media_description_chars = state.max_media_description_chars;
	let poll_limits = state.poll_limits.clone();
	let compose_prefs = dialogs::ComposePrefs::from_config(&state.config);
	let mention_source = dialogs::SuggestionSource::from_state(state);
	deferred::dialog_some(
		ctx,
		move |frame| {
			dialogs::prompt_for_quote(
				frame,
				&target,
				max_post_chars,
				max_media_description_chars,
				&poll_limits,
				compose_prefs,
				mention_source,
			)
		},
		move |ctx, (post, config)| {
			let Some(requests) = ctx.requests() else {
				ctx.live_region.announce("Network not available");
				return;
			};
			let state = &mut *ctx.state;
			state.pending_thread_continuation = post.continue_thread;
			state.pending_post = Some(crate::PendingPost {
				config,
				operation: crate::PostOperation::Quote { quoted_status_id: target_id.clone() },
				last_result: post.clone(),
			});
			statuses::post(&requests, post_result_to_data(post, Some(target_id)));
		},
	);
}

pub(super) fn delete_post(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	let live_region = ctx.live_region;
	let Some(status) = get_selected_status(state) else {
		live_region.announce("No post selected");
		return;
	};
	let target = status.reblog.as_ref().map_or(status, std::convert::AsRef::as_ref);
	if let Some(current_user) = &state.current_user_id {
		if &target.account.id != current_user {
			live_region.announce("You can only delete your own posts");
			return;
		}
	} else {
		live_region.announce("Could not tell whether this post is yours");
		return;
	}
	let status_id = target.id.clone();
	deferred::dialog(
		ctx,
		|frame| wx_utils::confirm(frame, "Are you sure you want to delete this post?", "Delete Post"),
		move |ctx, confirmed| {
			if !confirmed {
				return;
			}
			if let Some(requests) = ctx.requests() {
				statuses::delete(&requests, status_id);
			} else {
				ctx.live_region.announce("Network not available");
			}
		},
	);
}

pub(super) fn edit_post(ctx: &mut UiCommandContext<'_>) {
	let requests = ctx.requests();
	let state = &mut *ctx.state;
	let live_region = ctx.live_region;
	let status = get_selected_status(state).cloned();
	let Some(status) = status else {
		live_region.announce("No post selected");
		return;
	};
	let target = status.reblog.as_ref().map_or(&status, std::convert::AsRef::as_ref);
	if let Some(current_user) = &state.current_user_id {
		if &target.account.id != current_user {
			live_region.announce("You can only edit your own posts");
			return;
		}
	} else {
		live_region.announce("Could not tell whether this post is yours");
		return;
	}
	if let Some(requests) = requests {
		statuses::fetch_source(&requests, target.clone());
	} else {
		live_region.announce("Network not available");
	}
}

pub(super) fn copy_post(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	let live_region = ctx.live_region;
	let Some(entry) = get_selected_entry(state) else {
		live_region.announce("No post selected");
		return;
	};
	let mut options = state.timeline_manager.active().map_or_else(
		|| TimelineTextOptions::from_config_default(&state.config),
		|a| TimelineTextOptions::from_config(&state.config, &a.timeline_type),
	);
	let strip_stats = |template: &str| -> String {
		let mut t = template.to_string();
		let to_remove = [
			" - {{ relative_time }}",
			" - {{ absolute_time }}",
			"{{ relative_time }}",
			"{{ absolute_time }}",
			", {{ visibility }}",
			"{{ visibility }}",
			"{% if reply_count %}, {{ reply_count }}{% endif %}",
			"{% if boost_count %}, {{ boost_count }}{% endif %}",
			"{% if favorite_count %}, {{ favorite_count }}{% endif %}",
			"{% if client %}, via {{ client }}{% endif %}",
		];
		for s in to_remove {
			t = t.replace(s, "");
		}
		t = t.replace(" - ,", " -");
		t = t.replace(", ,", ",");
		let mut cleaned = t.trim().to_string();
		if cleaned.ends_with(" -") {
			cleaned.truncate(cleaned.len() - 2);
		}
		if cleaned.ends_with(',') {
			cleaned.pop();
		}
		cleaned.trim().to_string()
	};
	options.post_template = strip_stats(&options.post_template);
	options.boost_template = strip_stats(&options.boost_template);
	options.quote_template = strip_stats(&options.quote_template);
	options.show_link_previews = false;
	let is_expanded = state.cw_expanded.contains(entry.id());
	let mut text = entry.display_text(&options, is_expanded).trim().to_string();
	while text.ends_with(" -") || text.ends_with(',') {
		if text.ends_with(" -") {
			text.truncate(text.len() - 2);
		} else if text.ends_with(',') {
			text.pop();
		}
		text = text.trim().to_string();
	}
	if text.is_empty() {
		live_region.announce("Post has no text");
		return;
	}
	let clipboard = Clipboard::get();
	let _ = clipboard.set_text(&text);
	// Persist the data after the app exits (otherwise Windows clears it)
	let _ = clipboard.flush();
	live_region.announce("Post copied");
}

pub(super) fn copy_post_link(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	let live_region = ctx.live_region;
	let Some(entry) = get_selected_entry(state) else {
		live_region.announce("No post selected");
		return;
	};
	if let Some(status) = entry.as_status() {
		let target = status.reblog.as_ref().map_or(status, std::convert::AsRef::as_ref);
		if let Some(url) = &target.url {
			let clipboard = Clipboard::get();
			let _ = clipboard.set_text(url);
			let _ = clipboard.flush();
			live_region.announce("Post link copied");
		} else {
			live_region.announce("Post has no link");
		}
	} else {
		live_region.announce("That is not a post");
	}
}

pub(super) fn favorite(ctx: &mut UiCommandContext<'_>) {
	send_status_action(ctx, |_, s| Ok(if s.favourited { StatusAction::Unfavorite } else { StatusAction::Favorite }));
}

pub(super) fn bookmark(ctx: &mut UiCommandContext<'_>) {
	send_status_action(ctx, |_, s| Ok(if s.bookmarked { StatusAction::Unbookmark } else { StatusAction::Bookmark }));
}

pub(super) fn mute_conversation(ctx: &mut UiCommandContext<'_>) {
	send_status_action(ctx, |_, s| {
		Ok(if s.muted { StatusAction::UnmuteConversation } else { StatusAction::MuteConversation })
	});
}

/// Lists the user's scheduled posts to reschedule or delete, coming back to the list after each change until it's
/// closed.
pub(super) fn scheduled_posts(ctx: &mut UiCommandContext<'_>) {
	let Some(session) = ctx.state.session() else {
		ctx.live_region.announce("Network not available");
		return;
	};
	let frame = *ctx.frame;
	let live_region = ctx.live_region.clone();
	spawn_for(&frame, async move {
		let mut selected = 0;
		let mut changed = false;
		loop {
			let mut posts = match session.call(MastodonClient::get_scheduled_statuses).await {
				Ok(posts) => posts,
				Err(err) => {
					live_region.announce(&spoken_failure("Failed to load scheduled posts", &err));
					return;
				}
			};
			if posts.is_empty() {
				live_region.announce(if changed { "No more scheduled posts" } else { "No scheduled posts" });
				return;
			}
			posts.sort_by(|a, b| a.scheduled_at.cmp(&b.scheduled_at));
			let Some((action, index)) = dialogs::show_scheduled_posts_dialog(&frame, &posts, selected) else { return };
			selected = index;
			let done = match action {
				dialogs::ScheduledPostAction::Reschedule { id, scheduled_at } => session
					.queued(move |client, token| client.reschedule_status(token, &id, &scheduled_at))
					.await
					.map(|post| format!("Rescheduled for {}", friendly_time_local(&post.scheduled_at))),
				dialogs::ScheduledPostAction::Delete { id } => session
					.queued(move |client, token| client.delete_scheduled_status(token, &id))
					.await
					.map(|()| "Scheduled post deleted".to_string()),
			};
			match done {
				Ok(message) => {
					changed = true;
					live_region.announce(&message);
				}
				Err(err) => live_region.announce(&spoken_failure("Failed to change the scheduled post", &err)),
			}
		}
	});
}

pub(super) fn view_edit_history(ctx: &mut UiCommandContext<'_>) {
	let state = &*ctx.state;
	let Some(status) = get_selected_status(state) else {
		ctx.live_region.announce("No post selected");
		return;
	};
	let target = status.reblog.as_deref().unwrap_or(status);
	if target.edited_at.is_none() {
		ctx.live_region.announce("This post hasn't been edited");
		return;
	}
	let Some(session) = state.session() else {
		ctx.live_region.announce("Network not available");
		return;
	};
	let foreign = foreign_url(state, target.url.as_ref());
	let status_id = target.id.clone();
	let history = session.call(move |client, token| {
		let status_id = match foreign {
			Some(url) => resolve_status(client, token, &url)?.id,
			None => status_id,
		};
		client.get_status_history(token, &status_id)
	});
	let frame = *ctx.frame;
	let live_region = ctx.live_region.clone();
	spawn_for(&frame, async move {
		match history.await {
			Ok(versions) if versions.len() > 1 => dialogs::show_edit_history_dialog(&frame, &versions),
			Ok(_) => live_region.announce("No earlier versions of this post"),
			Err(err) => live_region.announce(&spoken_failure("Failed to load edit history", &err)),
		}
	});
}

pub(super) fn boost(ctx: &mut UiCommandContext<'_>) {
	send_status_action(ctx, |_, s| {
		if s.visibility == "direct" {
			return Err("Cannot boost direct messages");
		}
		Ok(if s.reblogged { StatusAction::Unboost } else { StatusAction::Boost })
	});
}

pub(super) fn pin(ctx: &mut UiCommandContext<'_>) {
	send_status_action(ctx, |state, s| {
		if Some(&s.account.id) != state.current_user_id.as_ref() {
			return Err("You can only pin your own posts");
		}
		Ok(if s.pinned { StatusAction::Unpin } else { StatusAction::Pin })
	});
}

pub(super) fn toggle_content_warning(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	let timeline_list = &ctx.timeline_list;
	let suppress_selection = ctx.suppress_selection;
	let live_region = ctx.live_region;
	if state.config.content_warning_display != ContentWarningDisplay::WarningOnly {
		return;
	}
	let Some(active) = state.timeline_manager.active() else { return };
	let Some(list_index) = active.selected_index else {
		live_region.announce("No post selected");
		return;
	};
	let Some(entry_index) =
		list_index_to_entry_index(list_index, active.entries.len(), active.effective_sort_order(&state.config))
	else {
		return;
	};
	let Some(entry) = active.entries.get(entry_index) else { return };
	let Some(status) = entry.as_status() else {
		live_region.announce("No post selected");
		return;
	};
	let target = status.reblog.as_ref().map_or(status, std::convert::AsRef::as_ref);
	if target.spoiler_text.trim().is_empty() {
		live_region.announce("No content warning");
		return;
	}
	let entry_id = entry.id().to_string();
	if !state.cw_expanded.remove(&entry_id) {
		state.cw_expanded.insert(entry_id);
	}
	state.redraw_active_timeline(timeline_list, suppress_selection);
}

/// What Open does with a choice, once the post's choices are known.
enum OpenTarget {
	Command(UiCommand),
	Link(String),
}

/// Does what the selected post has: votes in its poll, plays its media, or opens its link, or asks which when it has
/// more than one, so the user needn't remember which key does what for which post.
pub(super) fn open_post(ctx: &mut UiCommandContext<'_>) {
	let state = &*ctx.state;
	let live_region = ctx.live_region;
	let Some(status) = get_selected_status(state) else {
		live_region.announce("No post selected");
		return;
	};
	let target = status.reblog.as_deref().unwrap_or(status);
	let mut choices: Vec<(String, OpenTarget)> = Vec::new();
	if let Some(poll) = &target.poll {
		let label = if poll.expired || poll.voted.unwrap_or(false) { "Poll results" } else { "Vote on poll" };
		choices.push((label.to_string(), OpenTarget::Command(UiCommand::Vote)));
	}
	let quoted = target.quote.as_ref().and_then(|q| q.quoted_status.as_deref());
	let media = if target.media_attachments.is_empty() {
		quoted.map_or(0, |q| q.media_attachments.len())
	} else {
		target.media_attachments.len()
	};
	match media {
		0 => {}
		1 => choices.push(("Play media".to_string(), OpenTarget::Command(UiCommand::PlayMedia))),
		count => choices.push((format!("Play media ({count} attachments)"), OpenTarget::Command(UiCommand::PlayMedia))),
	}
	for link in post_links(state, target) {
		choices.push((format!("Open {}", link.url), OpenTarget::Link(link.url)));
	}
	let only_link_but_asked =
		state.config.always_show_link_dialog && matches!(choices.as_slice(), [(_, OpenTarget::Link(_))]);
	match choices.len() {
		0 => live_region.announce("Nothing to open in this post"),
		1 if !only_link_but_asked => {
			if let Some((_, target)) = choices.pop() {
				open_target(ctx, target);
			}
		}
		_ => {
			let options: Vec<dialogs::OpenChoice> = choices
				.iter()
				.map(|(label, target)| dialogs::OpenChoice {
					label: label.clone(),
					url: match target {
						OpenTarget::Link(url) => Some(url.clone()),
						OpenTarget::Command(_) => None,
					},
				})
				.collect();
			deferred::dialog_some(
				ctx,
				move |frame| dialogs::show_open_dialog(frame, "Open", "What to open:", &options),
				move |ctx, index| {
					if index < choices.len() {
						let target = choices.swap_remove(index);
						open_target(ctx, target.1);
					}
				},
			);
		}
	}
}

fn open_target(ctx: &mut UiCommandContext<'_>, target: OpenTarget) {
	match target {
		OpenTarget::Command(command) => handle_ui_command(command, ctx),
		OpenTarget::Link(url) => {
			ctx.live_region.announce("Opening link");
			let _ = launch_default_browser(&url, BrowserLaunchFlags::Default);
		}
	}
}

/// The links in `target` and the post it quotes, once each, with tracking removed if the user asked for that.
fn post_links(state: &AppState, target: &Status) -> Vec<html::Link> {
	let mut links = html::extract_links(&target.content);
	if let Some(quote) = &target.quote
		&& let Some(quoted_status) = &quote.quoted_status
	{
		let mut quote_links = html::extract_links(&quoted_status.content);
		links.append(&mut quote_links);
		if let Some(quote_url) = &quoted_status.url {
			links.retain(|link| link.url != *quote_url);
		}
	}
	let mut seen = std::collections::HashSet::new();
	links.retain(|link| seen.insert(link.url.clone()));
	if state.config.strip_tracking {
		for link in &mut links {
			link.url = html::clean_url(&link.url);
		}
	}
	links
}

pub(super) fn open_links(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	let live_region = ctx.live_region;
	let Some(status) = get_selected_status(state) else { return };
	let target = status.reblog.as_ref().map_or(status, std::convert::AsRef::as_ref);
	let links = post_links(state, target);
	if links.is_empty() {
		live_region.announce("No links in this post");
		return;
	}
	let open = |live_region: &crate::ui::timeline_list::TimelineList, url: &str| {
		live_region.announce("Opening link");
		let _ = launch_default_browser(url, BrowserLaunchFlags::Default);
	};
	if links.len() == 1 && !state.config.always_show_link_dialog {
		open(live_region, &links[0].url);
		return;
	}
	deferred::dialog_some(
		ctx,
		move |frame| dialogs::show_link_selection_dialog(frame, &links),
		move |ctx, url| open(ctx.live_region, &url),
	);
}

pub(super) fn play_media(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	let live_region = ctx.live_region;
	let Some(status) = get_selected_status(state) else {
		live_region.announce("No post selected");
		return;
	};
	let target = status.reblog.as_ref().map_or(status, std::convert::AsRef::as_ref);
	// A quote post's own media_attachments only cover media attached directly
	// to it; media on the post it quotes lives on the quoted status instead.
	let target = if target.media_attachments.is_empty() {
		target.quote.as_ref().and_then(|q| q.quoted_status.as_deref()).unwrap_or(target)
	} else {
		target
	};
	if target.media_attachments.is_empty() {
		live_region.announce("No media attached to this post");
		return;
	}
	let access_token = state.access_token.clone();
	let media: Vec<(String, String)> =
		target.media_attachments.iter().map(|m| (m.url.clone(), m.kind.clone())).collect();
	if media.len() == 1 {
		let (url, kind) = &media[0];
		crate::ui::dialogs::show_media_player(ctx.frame, url.clone(), kind, access_token);
		return;
	}
	let options: Vec<String> = target
		.media_attachments
		.iter()
		.enumerate()
		.map(|(i, m)| {
			let name = format!("{} {}", m.kind, i + 1);
			if let Some(desc) = &m.description
				&& !desc.is_empty()
			{
				return format!("{name} - {desc}");
			}
			name
		})
		.collect();
	deferred::dialog_some(
		ctx,
		move |frame| {
			let options_refs: Vec<&str> = options.iter().map(AsRef::as_ref).collect();
			let dialog =
				SingleChoiceDialog::builder(frame, "Select media to play", "Play Media", &options_refs).build();
			if dialog.show_modal() != ID_OK {
				return None;
			}
			usize::try_from(dialog.get_selection()).ok()
		},
		move |ctx, index| {
			if let Some((url, kind)) = media.get(index) {
				crate::ui::dialogs::show_media_player(ctx.frame, url.clone(), kind, access_token);
			}
		},
	);
}

pub(super) fn view_in_browser(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	let live_region = ctx.live_region;
	let Some(status) = get_selected_status(state) else {
		live_region.announce("No post selected");
		return;
	};
	let target = status.reblog.as_ref().map_or(status, std::convert::AsRef::as_ref);
	let mut options = Vec::new();
	let mut urls = Vec::new();
	if let Some(url) = &target.url {
		options.push("Original Post");
		urls.push(url.clone());
	}
	if let Some(quote) = &target.quote
		&& let Some(quoted_status) = &quote.quoted_status
		&& let Some(quote_url) = &quoted_status.url
	{
		options.push("Quoted Post");
		urls.push(quote_url.clone());
	}
	if options.is_empty() {
		live_region.announce("Post URL not available");
		return;
	}
	let open = |live_region: &crate::ui::timeline_list::TimelineList, url: &str| {
		live_region.announce("Opening post in browser");
		let _ = launch_default_browser(url, BrowserLaunchFlags::Default);
	};
	if options.len() == 1 {
		open(live_region, &urls[0]);
		return;
	}
	deferred::dialog_some(
		ctx,
		move |frame| {
			let dialog =
				SingleChoiceDialog::builder(frame, "Which post do you want to open?", "View in Browser", &options)
					.build();
			if dialog.show_modal() != ID_OK {
				return None;
			}
			usize::try_from(dialog.get_selection()).ok().and_then(|index| urls.get(index).cloned())
		},
		move |ctx, url| open(ctx.live_region, &url),
	);
}

pub(super) fn vote(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	let live_region = ctx.live_region;
	let Some(status) = get_selected_status(state) else {
		live_region.announce("No post selected");
		return;
	};
	let target = status.reblog.as_ref().map_or(status, std::convert::AsRef::as_ref);
	let Some(poll) = target.poll.clone() else {
		live_region.announce("No poll in this post");
		return;
	};
	let post_text = target.display_text().to_string();
	let poll_id = poll.id.clone();
	let target = target.clone();
	deferred::dialog_some(
		ctx,
		move |frame| dialogs::prompt_for_vote(frame, &poll, &post_text),
		move |ctx, choices| {
			let Some(requests) = ctx.requests() else {
				ctx.live_region.announce("Network not available");
				return;
			};
			let poll_target = match reply_target(ctx.state, &target) {
				Target::Url(url) => Target::Url(url),
				Target::Id(_) => Target::Id(poll_id),
			};
			statuses::vote(&requests, poll_target, choices);
		},
	);
}

pub(super) fn view_post(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	let live_region = ctx.live_region;
	let Some(status) = get_selected_status(state) else {
		live_region.announce("No post selected");
		return;
	};
	let target = status.reblog.as_ref().map_or(status, std::convert::AsRef::as_ref).clone();
	deferred::dialog_some(
		ctx,
		move |frame| crate::ui::dialogs::show_post_view_dialog(frame, &target),
		|ctx, next_cmd| handle_ui_command(next_cmd, ctx),
	);
}

pub(super) fn recover_draft(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	let Some(pending) = state.pending_post.take() else { return };
	let mut config = pending.config;
	config.initial_content = pending.last_result.content;
	config.initial_cw = pending.last_result.spoiler_text;
	config.initial_sensitive = pending.last_result.sensitive;
	config.initial_language = pending.last_result.language;
	config.default_visibility = pending.last_result.visibility;
	config.initial_thread_mode = pending.last_result.continue_thread;
	config.max_media_description_chars = state.max_media_description_chars;
	let max_post_chars = state.max_post_chars;
	let poll_limits = state.poll_limits.clone();
	let compose_prefs = dialogs::ComposePrefs::from_config(&state.config);
	let mention_source = dialogs::SuggestionSource::from_state(state);
	let media = pending.last_result.media;
	let poll = pending.last_result.poll;
	let operation = pending.operation;
	deferred::dialog_some(
		ctx,
		move |frame| {
			dialogs::prompt_for_compose(
				frame,
				max_post_chars,
				&poll_limits,
				compose_prefs,
				config,
				media,
				poll,
				mention_source,
			)
		},
		move |ctx, (new_post, new_config)| send_recovered_draft(ctx, operation, new_post, new_config),
	);
}

fn send_recovered_draft(
	ctx: &mut UiCommandContext<'_>,
	operation: crate::PostOperation,
	new_post: dialogs::PostResult,
	new_config: dialogs::ComposeDialogConfig,
) {
	let Some(requests) = ctx.requests() else {
		ctx.timeline_list.announce("Network not available");
		return;
	};
	let state = &mut *ctx.state;
	let quoted_id = match &operation {
		crate::PostOperation::Quote { quoted_status_id } => Some(quoted_status_id.clone()),
		_ => None,
	};
	match &operation {
		crate::PostOperation::Reply { in_reply_to_id } => statuses::reply(
			&requests,
			Target::Id(in_reply_to_id.clone()),
			post_result_to_data(new_post.clone(), quoted_id),
		),
		crate::PostOperation::Edit { status_id } => {
			statuses::edit(&requests, status_id.clone(), status_edit(new_post.clone()));
		}
		crate::PostOperation::NewPost | crate::PostOperation::Quote { .. } => {
			statuses::post(&requests, post_result_to_data(new_post.clone(), quoted_id));
		}
	}
	state.pending_thread_continuation = new_post.continue_thread;
	state.pending_post = Some(crate::PendingPost { config: new_config, operation, last_result: new_post });
}
