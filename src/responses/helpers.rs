use std::collections::HashMap;

use crate::{AppState, mastodon::Status, network::NetworkCommand, timeline::TimelineEntry};

pub(super) fn summarize_api_error(err: &anyhow::Error) -> String {
	for cause in err.chain().skip(1) {
		let mut message = cause.to_string();
		if message.trim().is_empty() {
			continue;
		}
		if message.starts_with("HTTP status")
			&& let Some((head, _)) = message.split_once(" for url")
		{
			message = head.to_string();
		}
		return message;
	}
	err.to_string()
}

pub(super) fn spoken_failure(prefix: &str, err: &anyhow::Error) -> String {
	format!("{prefix}: {}", summarize_api_error(err))
}

/// Re-fetches all open user timelines belonging to the current account so that
/// pinned-post ordering reflects the latest pin state.
pub(super) fn refresh_own_user_timelines(state: &AppState) {
	let Some(current_user_id) = &state.current_user_id else { return };
	let Some(handle) = &state.network_handle else { return };
	let types = state.timeline_manager.open_timeline_types();
	for tt in types {
		if let crate::timeline::TimelineType::User { ref id, .. } = tt
			&& id == current_user_id
		{
			handle.send(NetworkCommand::FetchTimeline {
				timeline_type: tt,
				limit: Some(u32::from(state.config.fetch_limit)),
				max_id: None,
			});
		}
	}
}

/// Folds freshly fetched copies of posts into every timeline still showing an older copy.
/// Returns whether the active timeline changed.
pub(super) fn merge_status_snapshots(state: &mut AppState, snapshots: &[Status]) -> bool {
	let mut fresh: HashMap<&str, &Status> = HashMap::with_capacity(snapshots.len());
	for snapshot in snapshots {
		fresh.insert(&snapshot.id, snapshot);
		if let Some(reblog) = &snapshot.reblog {
			fresh.insert(&reblog.id, reblog);
		}
	}
	if fresh.is_empty() {
		return false;
	}
	let active = state.timeline_manager.active_index();
	let mut active_changed = false;
	for (index, timeline) in state.timeline_manager.iter_mut().enumerate() {
		for status in timeline.entries.iter_mut().filter_map(TimelineEntry::as_status_mut) {
			let mut changed = replace_if_stale(status, &fresh);
			if let Some(reblog) = &mut status.reblog {
				changed |= replace_if_stale(reblog, &fresh);
			}
			active_changed |= changed && index == active;
		}
	}
	active_changed
}

fn replace_if_stale(status: &mut Status, fresh: &HashMap<&str, &Status>) -> bool {
	match fresh.get(status.id.as_str()) {
		Some(&snapshot) if snapshot != status => {
			*status = snapshot.clone();
			true
		}
		_ => false,
	}
}
