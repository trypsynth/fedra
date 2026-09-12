use std::cell::Cell;

use wxdragon::prelude::*;

use super::helpers::merge_status_snapshot;
use crate::{
	AppState,
	mastodon::Status,
	streaming,
	timeline::{TimelineEntry, TimelineType},
	ui::{
		menu::update_menu_labels,
		timeline_view::{sync_timeline_selection_from_list, update_active_timeline_ui},
	},
};

/// Processes streaming events from WebSocket connections.
pub fn process_stream_events(
	state: &mut AppState,
	timeline_list: &crate::ui::timeline_list::TimelineList,
	suppress_selection: &Cell<bool>,
	frame: &Frame,
) {
	let active_type = state.timeline_manager.active().map(|t| t.timeline_type.clone());
	let mut active_needs_update = false;
	let mut processed_notification_ids = std::collections::HashSet::new();
	let mut status_snapshots: Vec<Status> = Vec::new();
	let mut mention_forwards: Vec<Box<crate::mastodon::Notification>> = Vec::new();
	let mut own_post_forwards: Vec<Box<Status>> = Vec::new();
	let mut own_delete_forwards: Vec<String> = Vec::new();

	for timeline in state.timeline_manager.iter_mut() {
		let Some(handle) = &timeline.stream_handle else { continue };
		let events = handle.drain();
		let is_active = active_type.as_ref() == Some(&timeline.timeline_type);
		let filter_context = timeline.timeline_type.filter_context();
		let filter_key = timeline.timeline_type.filter_key();
		let timeline_filter = state.config.filters.resolve(filter_key);
		let current_user_id_string = state
			.config
			.active_account_id
			.as_deref()
			.and_then(|id| state.config.accounts.iter().find(|a| a.id == id).and_then(|a| a.user_id.clone()));
		let current_user_id = current_user_id_string.as_deref();
		if is_active {
			let effective_sort_order = timeline.effective_sort_order(&state.config);
			sync_timeline_selection_from_list(timeline, timeline_list, effective_sort_order);
		}
		for event in events {
			match event {
				streaming::StreamEvent::Update { timeline_type, status } => {
					status_snapshots.push((*status).clone());
					// The user stream (Home) is the only stream that carries our own posts;
					// user timelines have no stream of their own, so forward them by hand.
					if timeline_type == TimelineType::Home && current_user_id == Some(status.account.id.as_str()) {
						own_post_forwards.push(status.clone());
					}
					if timeline.timeline_type == timeline_type
						&& !status.should_hide(&filter_context)
						&& status.matches_filter(&timeline_filter, current_user_id)
					{
						if !timeline.entries.iter().any(|entry| entry.id() == status.id) {
							timeline.entries.insert(0, TimelineEntry::Status(Box::new(*status)));
							if is_active {
								active_needs_update = true;
							}
						}
					}
				}
				streaming::StreamEvent::StatusUpdate { status, .. } => {
					status_snapshots.push((*status).clone());
				}
				streaming::StreamEvent::Delete { timeline_type, id } => {
					if timeline_type == TimelineType::Home {
						own_delete_forwards.push(id.clone());
					}
					if timeline.timeline_type == timeline_type {
						timeline.entries.retain(|entry| entry.as_status().is_none_or(|s| s.id != id));
						if is_active {
							active_needs_update = true;
						}
					}
				}
				streaming::StreamEvent::Notification { timeline_type, notification } => {
					if let Some(status) = notification.status.as_deref() {
						status_snapshots.push(status.clone());
					}
					if timeline.timeline_type == timeline_type {
						if !processed_notification_ids.contains(&notification.id) {
							let pref = state.config.notification_preference;
							match pref {
								crate::config::NotificationPreference::Classic => {
									if let Some(app_shell) = &state.app_shell {
										crate::notifications::show_notification(app_shell, &notification);
									}
								}
								crate::config::NotificationPreference::SoundOnly => {
									if let Some(mc) = &state.media_ctrl {
										mc.stop();
										mc.play();
									}
								}
								crate::config::NotificationPreference::Disabled => {}
							}
							processed_notification_ids.insert(notification.id.clone());
						}
						if notification.status.as_ref().is_none_or(|s| !s.should_hide(&filter_context))
							&& notification.matches_filter(&timeline_filter, current_user_id)
						{
							if notification.kind == "mention" {
								mention_forwards.push(notification.clone());
							}
							if !timeline.entries.iter().any(|entry| entry.id() == notification.id) {
								timeline.entries.insert(0, TimelineEntry::Notification(Box::new(*notification)));
								if is_active {
									active_needs_update = true;
								}
							}
						}
					}
				}
				streaming::StreamEvent::Conversation { timeline_type, conversation } => {
					if timeline.timeline_type == timeline_type
						&& let Some(mut status) = conversation.last_status
						&& !status.should_hide(&filter_context)
						&& status.matches_filter(&timeline_filter, current_user_id)
					{
						status.conversation_id = Some(conversation.id);
						status_snapshots.push(status.clone());
						if let Some(conv_id) = &status.conversation_id {
							timeline.entries.retain(|entry| {
								if let TimelineEntry::Status(s) = entry {
									s.conversation_id.as_deref() != Some(conv_id)
								} else {
									true
								}
							});
						}
						timeline.entries.insert(0, TimelineEntry::Status(Box::new(status)));
						if is_active {
							active_needs_update = true;
						}
					}
				}
				streaming::StreamEvent::Connected(timeline_type)
				| streaming::StreamEvent::Disconnected(timeline_type) => {
					let _ = timeline_type;
				}
			}
		}
	}
	if !mention_forwards.is_empty() {
		if let Some(mentions_tl) = state.timeline_manager.get_mut(&TimelineType::Mentions) {
			let existing_ids: std::collections::HashSet<String> =
				mentions_tl.entries.iter().map(|e| e.id().to_string()).collect();
			for notif in mention_forwards {
				if !existing_ids.contains(&notif.id) {
					mentions_tl.entries.insert(0, TimelineEntry::Notification(notif));
				}
			}
			if active_type.as_ref() == Some(&TimelineType::Mentions) {
				active_needs_update = true;
			}
		}
	}

	if !own_post_forwards.is_empty() || !own_delete_forwards.is_empty() {
		let current_user_id = state.current_user_id.clone();
		if let Some(current_user_id) = current_user_id {
			for timeline in state.timeline_manager.iter_mut() {
				let TimelineType::User { ref id, .. } = timeline.timeline_type else { continue };
				if *id != current_user_id {
					continue;
				}
				let filter_context = timeline.timeline_type.filter_context();
				let timeline_filter = state.config.filters.resolve(timeline.timeline_type.filter_key());
				let mut changed = false;
				if !own_delete_forwards.is_empty() {
					let before = timeline.entries.len();
					timeline
						.entries
						.retain(|entry| entry.as_status().is_none_or(|s| !own_delete_forwards.contains(&s.id)));
					changed |= timeline.entries.len() != before;
				}
				// New posts go below any pinned posts, which the timeline fetch keeps at the front.
				// Inserting each one at that same index leaves the newest first, as on Home.
				let insert_at = timeline.entries.iter().take_while(|e| e.as_status().is_some_and(|s| s.pinned)).count();
				for status in &own_post_forwards {
					if timeline.entries.iter().any(|e| e.as_status().is_some_and(|s| s.id == status.id)) {
						continue;
					}
					if status.should_hide(&filter_context)
						|| !status.matches_filter(&timeline_filter, Some(current_user_id.as_str()))
					{
						continue;
					}
					timeline.entries.insert(insert_at, TimelineEntry::Status(status.clone()));
					changed = true;
				}
				if changed && active_type.as_ref() == Some(&timeline.timeline_type) {
					active_needs_update = true;
				}
			}
		}
	}

	let mut merged_any = false;
	for snapshot in &status_snapshots {
		if merge_status_snapshot(state, snapshot) {
			merged_any = true;
		}
	}
	if merged_any {
		active_needs_update = true;
	}
	let view_options = state.timeline_manager.active().map(|a| state.timeline_view_options_for(&a.timeline_type));
	let active_index = state.timeline_manager.active_index();
	if active_needs_update
		&& let Some(view_options) = view_options
		&& let Some(active) = state.timeline_manager.active_mut()
	{
		update_active_timeline_ui(
			timeline_list,
			active,
			suppress_selection,
			&view_options,
			&state.cw_expanded,
			active_index,
		);
		if let Some(mb) = frame.get_menu_bar() {
			update_menu_labels(&mb, state);
		}
	}
}
