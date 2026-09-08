use super::{
	NetworkResponseContext,
	helpers::{merge_status_snapshot, summarize_api_error},
};
use crate::{
	UiCommand,
	mastodon::{SearchResults, SearchType, Status},
	network::{NetworkCommand, TimelineData},
	timeline::{TimelineEntry, TimelineType},
	ui::{
		dialogs,
		menu::update_menu_labels,
		timeline_view::{sync_timeline_selection_from_list, update_active_timeline_ui},
	},
};

pub(super) fn loaded(
	ctx: &mut NetworkResponseContext<'_>,
	timeline_type: TimelineType,
	data: TimelineData,
	max_id: Option<String>,
	active_type: Option<&TimelineType>,
) {
	let mut should_find_next = false;
	let mut should_find_prev = false;
	let status_snapshots = {
		let state = &mut *ctx.state;
		let timeline_list = &ctx.timeline_list;
		let live_region = ctx.live_region;
		let suppress_selection = ctx.suppress_selection;
		let is_active = active_type == Some(&timeline_type);
		let mut status_snapshots: Vec<Status> = Vec::new();
		let view_options = state.timeline_view_options_for(&timeline_type);
		let _text_options = &view_options.text_options;
		// Extract any pending position restore for this timeline's initial load.
		let timeline_index_opt = state.timeline_manager.index_of(&timeline_type);
		let restore_id = if max_id.is_none() {
			state
				.pending_restore_post_id
				.as_ref()
				.and_then(|(rt, id)| if *rt == timeline_type { Some(id.clone()) } else { None })
		} else {
			None
		};
		if let Some(timeline) = state.timeline_manager.get_mut(&timeline_type) {
			if is_active {
				let effective_sort_order = timeline.effective_sort_order(&state.config);
				sync_timeline_selection_from_list(timeline, timeline_list, effective_sort_order);
			}
			let filter_context = timeline_type.filter_context();
			let template_key = timeline_type.template_key();
			let timeline_filter = state.config.filters.resolve(template_key);
			let current_user_id_string = state
				.config
				.active_account_id
				.as_deref()
				.and_then(|id| state.config.accounts.iter().find(|a| a.id == id).and_then(|a| a.user_id.clone()));
			let current_user_id = current_user_id_string.as_deref();

			let (new_entries, next_max_id): (Vec<TimelineEntry>, Option<String>) = match data {
				TimelineData::Statuses(statuses, next) => (
					statuses
						.into_iter()
						.filter(|s| {
							!s.should_hide(&filter_context) && s.matches_filter(&timeline_filter, current_user_id)
						})
						.map(|s| TimelineEntry::Status(Box::new(s)))
						.collect(),
					next,
				),
				TimelineData::Notifications(notifications, next) => (
					notifications
						.into_iter()
						.filter(|n| {
							n.status.as_ref().is_none_or(|s| !s.should_hide(&filter_context))
								&& n.matches_filter(&timeline_filter, current_user_id)
						})
						.map(|n| TimelineEntry::Notification(Box::new(n)))
						.collect(),
					next,
				),
				TimelineData::Conversations(conversations, next) => (
					conversations
						.into_iter()
						.filter_map(|c| {
							c.last_status
								.filter(|s| {
									!s.should_hide(&filter_context)
										&& s.matches_filter(&timeline_filter, current_user_id)
								})
								.map(|mut s| {
									s.conversation_id = Some(c.id);
									TimelineEntry::Status(Box::new(s))
								})
						})
						.collect(),
					next,
				),
			};
			for entry in &new_entries {
				if let Some(status) = entry.as_status() {
					status_snapshots.push(status.clone());
				}
			}

			if max_id.is_some() {
				let existing_ids: std::collections::HashSet<&str> =
					timeline.entries.iter().map(crate::timeline::TimelineEntry::id).collect();
				let filtered: Vec<TimelineEntry> =
					new_entries.into_iter().filter(|entry| !existing_ids.contains(entry.id())).collect();
				if filtered.is_empty() {
					live_region.announce("No more posts");
				} else {
					timeline.entries.extend(filtered.clone());
				}
				timeline.next_max_id = next_max_id;

				if is_active {
					if let Some(idx) = timeline_index_opt {
						update_active_timeline_ui(
							timeline_list,
							timeline,
							suppress_selection,
							&view_options,
							&state.cw_expanded,
							idx,
						);
					}
				}
			} else {
				// A fresh fetch of the newest posts (initial open, or a manual/background
				// refresh). If the timeline already has entries loaded, merge instead of
				// replacing outright, so posts the user has already scrolled past (and their
				// selection/scroll position) survive a refresh instead of being wiped by a
				// full page of just-fetched newest posts.
				let was_initial_load = timeline.entries.is_empty();
				if was_initial_load {
					timeline.entries = new_entries;
				} else {
					let existing_ids: std::collections::HashSet<&str> =
						timeline.entries.iter().map(crate::timeline::TimelineEntry::id).collect();
					let mut fresh: Vec<TimelineEntry> =
						new_entries.into_iter().filter(|entry| !existing_ids.contains(entry.id())).collect();
					if !fresh.is_empty() {
						fresh.extend(std::mem::take(&mut timeline.entries));
						timeline.entries = fresh;
					}
				}
				// Restore selected post if it exists in the freshly loaded entries.
				if let Some(ref id) = restore_id {
					if timeline.entries.iter().any(|e| e.id() == id.as_str()) {
						timeline.selected_id = Some(id.clone());
					}
				}
				if is_active {
					if let Some(idx) = timeline_index_opt {
						update_active_timeline_ui(
							timeline_list,
							timeline,
							suppress_selection,
							&view_options,
							&state.cw_expanded,
							idx,
						);
					}
				}
				// Only adopt the new pagination cursor on the initial load; a merge keeps
				// the existing (older) cursor, which still correctly points past the
				// combined list's oldest entry for "load more".
				if was_initial_load {
					timeline.next_max_id = next_max_id;
				}
			}
			timeline.loading_more = false;
			timeline.loading_more_in_background = false;
			if is_active && timeline.pending_find_next {
				timeline.pending_find_next = false;
				should_find_next = true;
			}
			if is_active && timeline.pending_find_prev {
				timeline.pending_find_prev = false;
				should_find_prev = true;
			}
		}
		// Clear the pending restore if it was for this timeline (whether found or not).
		if restore_id.is_some() {
			state.pending_restore_post_id = None;
		}
		status_snapshots
	};
	merge_snapshots(ctx, &status_snapshots);
	if let Some(mb) = ctx.frame.get_menu_bar() {
		update_menu_labels(&mb, ctx.state);
	}
	if should_find_next {
		ctx.dispatch(UiCommand::FindNext);
	}
	if should_find_prev {
		ctx.dispatch(UiCommand::FindPrev);
	}
}

pub(super) fn load_failed(
	ctx: &mut NetworkResponseContext<'_>,
	timeline_type: &TimelineType,
	err: &anyhow::Error,
	loading_more: bool,
) {
	if let Some(timeline) = ctx.state.timeline_manager.get_mut(timeline_type) {
		timeline.loading_more = false;
	}
	if loading_more {
		ctx.announce("Failed to load more posts");
	} else {
		ctx.announce_failure("Failed to load timeline", err);
	}
}

pub(super) fn search_loaded(
	ctx: &mut NetworkResponseContext<'_>,
	query: String,
	search_type: SearchType,
	results: SearchResults,
	offset: Option<u32>,
	active_type: Option<&TimelineType>,
) {
	let status_snapshots = {
		let state = &mut *ctx.state;
		let timeline_list = &ctx.timeline_list;
		let live_region = ctx.live_region;
		let suppress_selection = ctx.suppress_selection;
		if let Some(dlg) = &state.manage_list_members_dialog
			&& matches!(search_type, crate::mastodon::SearchType::Accounts)
		{
			let labels: Vec<String> =
				results.accounts.iter().map(|a| format!("{}: @{}", a.display_name_or_username(), a.acct)).collect();
			let label_refs: Vec<&str> = labels.iter().map(String::as_str).collect();
			let accounts_ref: Vec<&crate::mastodon::Account> = results.accounts.iter().collect();

			if let Some(account) = dialogs::prompt_for_account_choice(dlg.get_dialog(), &accounts_ref, &label_refs)
				&& let Some(handle) = &state.network_handle
			{
				handle.send(NetworkCommand::AddListAccount {
					list_id: dlg.get_list_id().to_string(),
					account_id: account.id,
				});
			}
			return;
		}

		let timeline_type = TimelineType::Search { query: query.clone(), search_type };
		let is_active = active_type == Some(&timeline_type);
		let mut status_snapshots: Vec<Status> = Vec::new();
		let view_options = state.timeline_view_options_for(&timeline_type);
		let _text_options = &view_options.text_options;
		let timeline_index_opt = state.timeline_manager.index_of(&timeline_type);
		if let Some(timeline) = state.timeline_manager.get_mut(&timeline_type) {
			if is_active {
				let effective_sort_order = timeline.effective_sort_order(&state.config);
				sync_timeline_selection_from_list(timeline, timeline_list, effective_sort_order);
			}
			let mut new_entries: Vec<TimelineEntry> = Vec::new();
			for account in results.accounts {
				new_entries.push(TimelineEntry::Account(account));
			}
			for hashtag in results.hashtags {
				new_entries.push(TimelineEntry::Hashtag(hashtag));
			}
			for status in results.statuses {
				new_entries.push(TimelineEntry::Status(Box::new(status)));
			}
			for entry in &new_entries {
				if let Some(status) = entry.as_status() {
					status_snapshots.push(status.clone());
				}
			}
			let is_load_more = offset.is_some() && offset.unwrap_or(0) > 0;
			if is_load_more {
				if new_entries.is_empty() {
					live_region.announce("No more results");
				} else {
					timeline.entries.extend(new_entries.clone());
					if is_active {
						if let Some(idx) = timeline_index_opt {
							update_active_timeline_ui(
								timeline_list,
								timeline,
								suppress_selection,
								&view_options,
								&state.cw_expanded,
								idx,
							);
						}
					}
				}
			} else {
				timeline.entries = new_entries;
				if is_active {
					if let Some(idx) = timeline_index_opt {
						update_active_timeline_ui(
							timeline_list,
							timeline,
							suppress_selection,
							&view_options,
							&state.cw_expanded,
							idx,
						);
					}
				}
			}
			timeline.loading_more = false;
		}
		status_snapshots
	};
	merge_snapshots(ctx, &status_snapshots);
	if let Some(mb) = ctx.frame.get_menu_bar() {
		update_menu_labels(&mb, ctx.state);
	}
}

pub(super) fn search_failed(
	ctx: &mut NetworkResponseContext<'_>,
	query: &str,
	search_type: SearchType,
	err: &anyhow::Error,
) {
	let timeline_type = TimelineType::Search { query: query.to_string(), search_type };
	if let Some(timeline) = ctx.state.timeline_manager.get_mut(&timeline_type) {
		timeline.loading_more = false;
	}
	ctx.announce(&format!("Search for '{query}' failed: {}", summarize_api_error(err)));
}

/// Folds freshly fetched copies of a post into every timeline that already
/// shows it, then redraws if anything changed.
fn merge_snapshots(ctx: &mut NetworkResponseContext<'_>, snapshots: &[Status]) {
	if snapshots.is_empty() {
		return;
	}
	let mut merged_any = false;
	for snapshot in snapshots {
		if merge_status_snapshot(ctx.state, snapshot) {
			merged_any = true;
		}
	}
	if merged_any {
		ctx.refresh_active_timeline();
	}
}
