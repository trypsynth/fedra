//! Keeping the home and notifications reading position in sync with the server's markers.

use std::{sync::mpsc, thread, time::Duration};

use crate::{AppState, mastodon::Markers, timeline::TimelineType};

pub const MAX_RESTORE_PAGES: u8 = 10;

const fn marker_name(timeline_type: &TimelineType) -> Option<&'static str> {
	match timeline_type {
		TimelineType::Home => Some("home"),
		TimelineType::Notifications => Some("notifications"),
		_ => None,
	}
}

/// Whether post or notification ID `a` is older than `b`. Mastodon IDs sort by length, then text.
pub fn is_older(a: &str, b: &str) -> bool {
	(a.len(), a) < (b.len(), b)
}

/// Restores the active timeline to its server marker when that's further along than the
/// position saved locally.
pub fn apply_markers(state: &mut AppState, markers: Markers) {
	let Some(active) = state.timeline_manager.active().map(|t| t.timeline_type.clone()) else { return };
	let marker = match active {
		TimelineType::Home => markers.home,
		TimelineType::Notifications => markers.notifications,
		_ => None,
	};
	let Some(marker) = marker else { return };
	if let Some(name) = marker_name(&active) {
		state.synced_markers.insert(name, marker.last_read_id.clone());
	}
	let local = state.pending_restore_post_id.as_ref().filter(|(t, _)| *t == active).map(|(_, id)| id.as_str());
	if local.is_none_or(|local| is_older(local, &marker.last_read_id)) {
		state.pending_restore_post_id = Some((active, marker.last_read_id));
	}
}

fn changed_positions(state: &AppState) -> Vec<(&'static str, String)> {
	if !state.config.sync_read_position {
		return Vec::new();
	}
	state
		.timeline_manager
		.timelines()
		.iter()
		.filter_map(|t| Some((marker_name(&t.timeline_type)?, t.selected_id.clone()?)))
		.filter(|(name, id)| state.synced_markers.get(name) != Some(id))
		.collect()
}

pub fn sync(state: &mut AppState) {
	let changed = changed_positions(state);
	let Some(requests) = state.requests() else { return };
	for (name, id) in changed {
		let last_read_id = id.clone();
		requests.request(
			move |client, token| {
				// A position that doesn't save is saved again with the next change.
				let _ = client.set_marker(token, name, &last_read_id);
			},
			|_, ()| {},
		);
		state.synced_markers.insert(name, id);
	}
}

/// Saves changed positions directly, waiting up to two seconds, since the request queue may not
/// get to them before the process exits.
pub fn sync_before_exit(state: &AppState) {
	let changed = changed_positions(state);
	let (Some(client), Some(token)) = (state.client.clone(), state.access_token.clone()) else { return };
	if changed.is_empty() {
		return;
	}
	let (done_tx, done_rx) = mpsc::channel();
	thread::spawn(move || {
		for (name, id) in changed {
			let _ = client.set_marker(&token, name, &id);
		}
		let _ = done_tx.send(());
	});
	if done_rx.recv_timeout(Duration::from_secs(2)).is_err() {
		tracing::warn!("exited before the reading positions were saved to the server");
	}
}
