use anyhow::Result;

use super::{
	NetworkResponseContext,
	helpers::{refresh_own_user_timelines, summarize_api_error},
	timeline_updates::{remove_status_from_timelines, update_poll_in_timelines, update_status_in_timelines},
};
use crate::{
	UiCommand,
	mastodon::{Poll, PostSubmission, Status, StatusAction, StatusSource, friendly_time_local},
	network::{self, PostData, StatusEdit, Target, resolve_status, submit},
	soundpack::SoundEvent,
	ui::commands::{Requests, run_edit_post_dialog},
};

/// Publishes or schedules `post`, a new post or a quote.
pub fn post(requests: &Requests, post: PostData) {
	requests.request(move |client, token| submit(client, token, &post, None), post_complete);
}

pub fn reply(requests: &Requests, target: Target, post: PostData) {
	requests.request(move |client, token| network::reply(client, token, target, &post), replied);
}

pub fn edit(requests: &Requests, status_id: String, changes: StatusEdit) {
	requests.request(
		move |client, token| network::edit(client, token, &status_id, changes),
		|ctx, result| match result {
			Ok(status) => edited(ctx, &status),
			Err(err) => ctx.announce_failure("Failed to edit", &err),
		},
	);
}

pub fn delete(requests: &Requests, status_id: String) {
	let id = status_id.clone();
	requests.request(
		move |client, token| client.delete_status(token, &id),
		move |ctx, result| match result {
			Ok(()) => deleted(ctx, &status_id),
			Err(err) => ctx.announce_failure("Failed to delete", &err),
		},
	);
}

pub fn vote(requests: &Requests, target: Target, choices: Vec<usize>) {
	requests.request(move |client, token| network::vote(client, token, target, &choices), poll_voted);
}

/// Fetches the text `status` was written as, then opens it for editing.
pub fn fetch_source(requests: &Requests, status: Status) {
	let id = status.id.clone();
	requests.request(
		move |client, token| client.fetch_status_source(token, &id),
		move |ctx, result| source_fetched(ctx, status, result),
	);
}

/// Finds a post from elsewhere on the account's instance, then offers to quote it.
pub fn resolve_for_quote(requests: &Requests, url: String) {
	requests.request(
		move |client, token| resolve_status(client, token, &url),
		|ctx, result| match result {
			Ok(focus) => {
				let _ = ctx.ui_tx.send(UiCommand::PromptForQuote(Box::new(focus)));
			}
			Err(err) => ctx.announce_error(&format!("Failed to resolve post for quote: {}", summarize_api_error(&err))),
		},
	);
}

/// Finds a post from elsewhere on the account's instance, then opens its thread.
pub fn resolve_for_thread(requests: &Requests, url: String) {
	requests.request(
		move |client, token| resolve_status(client, token, &url),
		|ctx, result| match result {
			Ok(focus) => {
				let _ = ctx.ui_tx.send(UiCommand::ViewResolvedThread(Box::new(focus)));
			}
			Err(err) => ctx.announce_error(&format!("Failed to resolve thread: {}", summarize_api_error(&err))),
		},
	);
}

/// One of the toggle actions a post supports, and how to fold the server's
/// answer back into every copy of that post already on screen.
struct StatusEffect {
	success: &'static str,
	failure: &'static str,
	apply: fn(&mut Status, &Status),
	/// Pinning reorders a user timeline, which only a refetch can reproduce.
	refetch_own_timelines: bool,
}

const FAVORITE: StatusEffect = StatusEffect {
	success: "Favorited",
	failure: "Failed to favorite",
	apply: copy_favorite,
	refetch_own_timelines: false,
};

const UNFAVORITE: StatusEffect = StatusEffect {
	success: "Unfavorited",
	failure: "Failed to unfavorite",
	apply: copy_favorite,
	refetch_own_timelines: false,
};

const BOOKMARK: StatusEffect = StatusEffect {
	success: "Bookmarked",
	failure: "Failed to bookmark",
	apply: copy_bookmark,
	refetch_own_timelines: false,
};

const UNBOOKMARK: StatusEffect = StatusEffect {
	success: "Unbookmarked",
	failure: "Failed to unbookmark",
	apply: copy_bookmark,
	refetch_own_timelines: false,
};

const PIN: StatusEffect = StatusEffect {
	success: "Post pinned",
	failure: "Failed to pin post",
	apply: copy_pin,
	refetch_own_timelines: true,
};

const UNPIN: StatusEffect = StatusEffect {
	success: "Post unpinned",
	failure: "Failed to unpin post",
	apply: copy_pin,
	refetch_own_timelines: true,
};

const BOOST: StatusEffect = StatusEffect {
	success: "Boosted",
	failure: "Failed to boost",
	apply: copy_boost_from_wrapper,
	refetch_own_timelines: false,
};

const UNBOOST: StatusEffect = StatusEffect {
	success: "Unboosted",
	failure: "Failed to unboost",
	apply: copy_boost,
	refetch_own_timelines: false,
};

const MUTE_CONVERSATION: StatusEffect = StatusEffect {
	success: "Conversation muted",
	failure: "Failed to mute conversation",
	apply: copy_mute,
	refetch_own_timelines: false,
};

const UNMUTE_CONVERSATION: StatusEffect = StatusEffect {
	success: "Conversation unmuted",
	failure: "Failed to unmute conversation",
	apply: copy_mute,
	refetch_own_timelines: false,
};

const fn copy_favorite(target: &mut Status, source: &Status) {
	target.favourited = source.favourited;
	target.favourites_count = source.favourites_count;
}

const fn copy_bookmark(target: &mut Status, source: &Status) {
	target.bookmarked = source.bookmarked;
}

const fn copy_mute(target: &mut Status, source: &Status) {
	target.muted = source.muted;
}

const fn copy_pin(target: &mut Status, source: &Status) {
	target.pinned = source.pinned;
}

const fn copy_boost(target: &mut Status, source: &Status) {
	target.reblogged = source.reblogged;
	target.reblogs_count = source.reblogs_count;
}

/// Boosting answers with the reblog wrapper, so the counts live one level down.
fn copy_boost_from_wrapper(target: &mut Status, source: &Status) {
	if let Some(inner) = &source.reblog {
		copy_boost(target, inner);
	}
}

const fn effect(action: StatusAction) -> &'static StatusEffect {
	match action {
		StatusAction::Favorite => &FAVORITE,
		StatusAction::Unfavorite => &UNFAVORITE,
		StatusAction::Bookmark => &BOOKMARK,
		StatusAction::Unbookmark => &UNBOOKMARK,
		StatusAction::Pin => &PIN,
		StatusAction::Unpin => &UNPIN,
		StatusAction::Boost => &BOOST,
		StatusAction::Unboost => &UNBOOST,
		StatusAction::MuteConversation => &MUTE_CONVERSATION,
		StatusAction::UnmuteConversation => &UNMUTE_CONVERSATION,
	}
}

/// Shows the outcome of a toggle on a post. The id is the acted-on post's, and the status is the
/// server's answer, which for a boost is the new wrapper.
pub fn action(ctx: &mut NetworkResponseContext<'_>, action: StatusAction, result: Result<(String, Status)>) {
	let effect = effect(action);
	match result {
		Ok((status_id, status)) => {
			update_status_in_timelines(ctx.state, &status_id, |s| (effect.apply)(s, &status));
			if effect.refetch_own_timelines {
				refresh_own_user_timelines(ctx.state);
			}
			ctx.refresh_menu_labels();
			// Only turning something on has a sound; the undoing actions stay silent.
			match action {
				StatusAction::Favorite => ctx.play(SoundEvent::Favorite),
				StatusAction::Bookmark => ctx.play(SoundEvent::Bookmark),
				StatusAction::Boost => ctx.play(SoundEvent::Boost),
				StatusAction::Pin => ctx.play(SoundEvent::Pin),
				_ => {}
			}
			ctx.announce(effect.success);
		}
		Err(err) => ctx.announce_failure(effect.failure, &err),
	}
}

fn source_fetched(ctx: &mut NetworkResponseContext<'_>, mut status: Status, result: Result<StatusSource>) {
	let source_text = match result {
		Ok(source) => {
			if !source.spoiler_text.is_empty() {
				status.spoiler_text = source.spoiler_text;
			}
			Some(source.text)
		}
		Err(err) => {
			ctx.announce(&format!(
				"Could not load the original text, so formatting may be lost: {}",
				summarize_api_error(&err)
			));
			None
		}
	};
	run_edit_post_dialog(ctx, &status, source_text);
}

fn post_complete(ctx: &mut NetworkResponseContext<'_>, result: Result<PostSubmission>) {
	match result {
		Ok(PostSubmission::Published(status)) => {
			ctx.state.pending_post = None;
			ctx.play(SoundEvent::PostSent);
			ctx.announce("Posted");
			continue_thread_if_pending(ctx, status);
		}
		Ok(PostSubmission::Scheduled(scheduled)) => {
			ctx.state.pending_post = None;
			ctx.state.pending_thread_continuation = false;
			ctx.announce(&format!("Post scheduled for {}", friendly_time_local(&scheduled.scheduled_at)));
		}
		Err(err) => {
			ctx.state.pending_thread_continuation = false;
			ctx.announce_failure("Failed to post", &err);
			ctx.dispatch(UiCommand::RecoverDraft);
		}
	}
}

fn replied(ctx: &mut NetworkResponseContext<'_>, result: Result<PostSubmission>) {
	match result {
		Ok(PostSubmission::Published(status)) => {
			ctx.state.pending_post = None;
			ctx.play(SoundEvent::PostSent);
			ctx.announce("Reply sent");
			continue_thread_if_pending(ctx, status);
		}
		Ok(PostSubmission::Scheduled(scheduled)) => {
			ctx.state.pending_post = None;
			ctx.state.pending_thread_continuation = false;
			ctx.announce(&format!("Reply scheduled for {}", friendly_time_local(&scheduled.scheduled_at)));
		}
		Err(err) => {
			ctx.state.pending_thread_continuation = false;
			ctx.announce_failure("Failed to reply", &err);
			ctx.dispatch(UiCommand::RecoverDraft);
		}
	}
}

fn continue_thread_if_pending(ctx: &mut NetworkResponseContext<'_>, status: Box<Status>) {
	if ctx.state.pending_thread_continuation {
		ctx.state.pending_thread_continuation = false;
		ctx.dispatch(UiCommand::ContinueThread(status));
	}
}

fn deleted(ctx: &mut NetworkResponseContext<'_>, status_id: &str) {
	remove_status_from_timelines(ctx.state, status_id);
	ctx.refresh_active_timeline();
	ctx.play(SoundEvent::PostDeleted);
	ctx.announce("Deleted");
}

fn edited(ctx: &mut NetworkResponseContext<'_>, status: &Status) {
	let snapshot = status.clone();
	update_status_in_timelines(ctx.state, &status.id, |s| *s = snapshot.clone());
	ctx.refresh_active_timeline();
	ctx.play(SoundEvent::PostEdited);
	ctx.announce("Edited");
}

fn poll_voted(ctx: &mut NetworkResponseContext<'_>, result: Result<Poll>) {
	match result {
		Ok(poll) => {
			update_poll_in_timelines(ctx.state, &poll);
			ctx.refresh_active_timeline();
			ctx.play(SoundEvent::PollVoted);
			ctx.announce("Vote submitted");
		}
		Err(err) => ctx.announce_failure("Failed to vote", &err),
	}
}
