use anyhow::Result;

use super::{
	NetworkResponseContext,
	helpers::{refresh_own_user_timelines, summarize_api_error},
	timeline_updates::{remove_status_from_timelines, update_poll_in_timelines, update_status_in_timelines},
};
use crate::{
	UiCommand,
	mastodon::{Poll, PostSubmission, Status, StatusSource, friendly_time_local},
};

/// One of the toggle actions a post supports, and how to fold the server's
/// answer back into every copy of that post already on screen.
pub(super) struct StatusAction {
	success: &'static str,
	failure: &'static str,
	apply: fn(&mut Status, &Status),
	/// Pinning reorders a user timeline, which only a refetch can reproduce.
	refetch_own_timelines: bool,
}

pub(super) const FAVORITE: StatusAction = StatusAction {
	success: "Favorited",
	failure: "Failed to favorite",
	apply: copy_favorite,
	refetch_own_timelines: false,
};

pub(super) const UNFAVORITE: StatusAction = StatusAction {
	success: "Unfavorited",
	failure: "Failed to unfavorite",
	apply: copy_favorite,
	refetch_own_timelines: false,
};

pub(super) const BOOKMARK: StatusAction = StatusAction {
	success: "Bookmarked",
	failure: "Failed to bookmark",
	apply: copy_bookmark,
	refetch_own_timelines: false,
};

pub(super) const UNBOOKMARK: StatusAction = StatusAction {
	success: "Unbookmarked",
	failure: "Failed to unbookmark",
	apply: copy_bookmark,
	refetch_own_timelines: false,
};

pub(super) const PIN: StatusAction = StatusAction {
	success: "Post pinned",
	failure: "Failed to pin post",
	apply: copy_pin,
	refetch_own_timelines: true,
};

pub(super) const UNPIN: StatusAction = StatusAction {
	success: "Post unpinned",
	failure: "Failed to unpin post",
	apply: copy_pin,
	refetch_own_timelines: true,
};

pub(super) const BOOST: StatusAction = StatusAction {
	success: "Boosted",
	failure: "Failed to boost",
	apply: copy_boost_from_wrapper,
	refetch_own_timelines: false,
};

pub(super) const UNBOOST: StatusAction = StatusAction {
	success: "Unboosted",
	failure: "Failed to unboost",
	apply: copy_boost,
	refetch_own_timelines: false,
};

fn copy_favorite(target: &mut Status, source: &Status) {
	target.favourited = source.favourited;
	target.favourites_count = source.favourites_count;
}

fn copy_bookmark(target: &mut Status, source: &Status) {
	target.bookmarked = source.bookmarked;
}

fn copy_pin(target: &mut Status, source: &Status) {
	target.pinned = source.pinned;
}

fn copy_boost(target: &mut Status, source: &Status) {
	target.reblogged = source.reblogged;
	target.reblogs_count = source.reblogs_count;
}

/// Boosting answers with the reblog wrapper, so the counts live one level down.
fn copy_boost_from_wrapper(target: &mut Status, source: &Status) {
	if let Some(inner) = &source.reblog {
		copy_boost(target, inner);
	}
}

pub(super) fn action(
	ctx: &mut NetworkResponseContext<'_>,
	status_id: &str,
	result: Result<Status>,
	action: &StatusAction,
) {
	match result {
		Ok(status) => {
			update_status_in_timelines(ctx.state, status_id, |s| (action.apply)(s, &status));
			if action.refetch_own_timelines {
				refresh_own_user_timelines(ctx.state);
			}
			ctx.refresh_menu_labels();
			ctx.announce(action.success);
		}
		Err(err) => ctx.announce_failure(action.failure, &err),
	}
}

pub(super) fn source_fetched(ctx: &mut NetworkResponseContext<'_>, mut status: Status, result: Result<StatusSource>) {
	let source_text = match result {
		Ok(source) => {
			if !source.spoiler_text.is_empty() {
				status.spoiler_text = source.spoiler_text;
			}
			Some(source.text)
		}
		Err(err) => {
			ctx.announce(&format!(
				"Could not fetch source text, editing with stripped HTML: {}",
				summarize_api_error(&err)
			));
			None
		}
	};
	crate::ui::commands::run_edit_post_dialog(ctx.frame, ctx.state, &status, source_text.as_deref());
}

pub(super) fn post_complete(ctx: &mut NetworkResponseContext<'_>, result: Result<PostSubmission>) {
	match result {
		Ok(PostSubmission::Published(status)) => {
			ctx.state.pending_post = None;
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

pub(super) fn replied(ctx: &mut NetworkResponseContext<'_>, result: Result<PostSubmission>) {
	match result {
		Ok(PostSubmission::Published(status)) => {
			ctx.announce("Reply sent");
			continue_thread_if_pending(ctx, status);
		}
		Ok(PostSubmission::Scheduled(scheduled)) => {
			ctx.state.pending_thread_continuation = false;
			ctx.announce(&format!("Reply scheduled for {}", friendly_time_local(&scheduled.scheduled_at)));
		}
		Err(err) => {
			ctx.state.pending_thread_continuation = false;
			ctx.announce_failure("Failed to reply", &err);
		}
	}
}

fn continue_thread_if_pending(ctx: &mut NetworkResponseContext<'_>, status: Box<Status>) {
	if ctx.state.pending_thread_continuation {
		ctx.state.pending_thread_continuation = false;
		ctx.dispatch(UiCommand::ContinueThread(status));
	}
}

pub(super) fn deleted(ctx: &mut NetworkResponseContext<'_>, status_id: &str) {
	remove_status_from_timelines(ctx.state, status_id);
	ctx.refresh_active_timeline();
	ctx.announce("Deleted");
}

pub(super) fn edited(ctx: &mut NetworkResponseContext<'_>, status: &Status) {
	let snapshot = status.clone();
	update_status_in_timelines(ctx.state, &status.id, |s| *s = snapshot.clone());
	ctx.refresh_active_timeline();
	ctx.announce("Edited");
}

pub(super) fn poll_voted(ctx: &mut NetworkResponseContext<'_>, result: Result<Poll>) {
	match result {
		Ok(poll) => {
			update_poll_in_timelines(ctx.state, &poll);
			ctx.refresh_active_timeline();
			ctx.announce("Vote submitted");
		}
		Err(err) => ctx.announce_failure("Failed to vote", &err),
	}
}
