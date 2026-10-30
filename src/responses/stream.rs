use std::cell::Cell;

use wxdragon::prelude::*;

use super::helpers::merge_status_snapshots;
use crate::{
	AppState,
	mastodon::Status,
	streaming,
	timeline::{TimelineEntry, TimelineType},
	ui::menu::update_menu_labels,
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
	let mut deleted_ids: Vec<String> = Vec::new();
	let mut new_post_alerts: Vec<(TimelineType, Status)> = Vec::new();
	let timeline_sounds = state.active_account().map(|a| a.timeline_sounds.clone()).unwrap_or_default();
	let notifying_timelines = state.active_account().map(|a| a.notifying_timelines.clone()).unwrap_or_default();
	for timeline in state.timeline_manager.iter_mut() {
		let Some(handle) = &timeline.stream_handle else { continue };
		let events = handle.drain();
		let is_active = active_type.as_ref() == Some(&timeline.timeline_type);
		let filter_context = timeline.timeline_type.filter_context();
		let template_key = timeline.timeline_type.template_key();
		let timeline_filter = state.config.filters.resolve(template_key);
		let current_user_id_string = state
			.config
			.active_account_id
			.as_deref()
			.and_then(|id| state.config.accounts.iter().find(|a| a.id == id).and_then(|a| a.user_id.clone()));
		let current_user_id = current_user_id_string.as_deref();
		let notifying = notifying_timelines.contains(&timeline.timeline_type);
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
						&& !timeline.entries.iter().any(|entry| entry.id() == status.id)
					{
						if notifying && current_user_id != Some(status.account.id.as_str()) {
							new_post_alerts.push((timeline.timeline_type.clone(), (*status).clone()));
						}
						timeline.entries.insert(0, TimelineEntry::Status(Box::new(*status)));
						if is_active {
							active_needs_update = true;
						}
					}
				}
				streaming::StreamEvent::StatusUpdate { status, .. } => {
					status_snapshots.push((*status).clone());
				}
				// Removed from every timeline below, not just this one, since copies may be open anywhere.
				streaming::StreamEvent::Delete { id } => deleted_ids.push(id),
				streaming::StreamEvent::Notification { timeline_type, notification } => {
					if let Some(status) = notification.status.as_deref() {
						status_snapshots.push(status.clone());
					}
					if timeline.timeline_type == timeline_type {
						let type_enabled = state.config.notification_kind_enabled(&notification.kind);
						let type_alerts = state.config.notification_kind_alerts(&notification.kind);
						if !processed_notification_ids.contains(&notification.id) {
							if type_alerts {
								let pref = state.config.notification_preference;
								match pref {
									crate::config::NotificationPreference::Classic => {
										crate::notifications::show_notification(
											state.app_shell.as_deref(),
											&notification,
										);
									}
									crate::config::NotificationPreference::SoundOnly => {
										let custom = timeline_sounds
											.iter()
											.find(|sound| sound.timeline == timeline.timeline_type)
											.map(|sound| sound.path.as_path());
										crate::notifications::play_sound(state.notification_sound.as_ref(), custom);
									}
									crate::config::NotificationPreference::Disabled => {}
								}
							}
							processed_notification_ids.insert(notification.id.clone());
						}
						if type_enabled
							&& notification.status.as_ref().is_none_or(|s| !s.should_hide(&filter_context))
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
						if notifying && current_user_id != Some(status.account.id.as_str()) {
							new_post_alerts.push((timeline.timeline_type.clone(), status.clone()));
						}
						timeline.entries.insert(0, TimelineEntry::Status(Box::new(status)));
						if is_active {
							active_needs_update = true;
						}
					}
				}
				streaming::StreamEvent::Connected(timeline_type) => {
					if timeline.timeline_type == timeline_type {
						timeline.stream_connected = true;
					}
				}
				streaming::StreamEvent::Disconnected(timeline_type) => {
					if timeline.timeline_type == timeline_type {
						timeline.stream_connected = false;
					}
				}
			}
		}
	}
	crate::notifications::notify_new_posts(state, &new_post_alerts);
	if !mention_forwards.is_empty()
		&& let Some(mentions_tl) = state.timeline_manager.get_mut(&TimelineType::Mentions)
	{
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
	if !deleted_ids.is_empty() {
		let active = state.timeline_manager.active_index();
		for (index, timeline) in state.timeline_manager.iter_mut().enumerate() {
			let before = timeline.entries.len();
			timeline.entries.retain(|entry| {
				entry.as_status().is_none_or(|s| {
					!deleted_ids.contains(&s.id) && s.reblog.as_ref().is_none_or(|r| !deleted_ids.contains(&r.id))
				})
			});
			active_needs_update |= index == active && timeline.entries.len() != before;
		}
	}
	if !own_post_forwards.is_empty() {
		let current_user_id = state.current_user_id.clone();
		if let Some(current_user_id) = current_user_id {
			for timeline in state.timeline_manager.iter_mut() {
				let TimelineType::User { ref id, .. } = timeline.timeline_type else { continue };
				if *id != current_user_id {
					continue;
				}
				let filter_context = timeline.timeline_type.filter_context();
				let timeline_filter = state.config.filters.resolve(timeline.timeline_type.template_key());
				let mut changed = false;
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
	if merge_status_snapshots(state, &status_snapshots) {
		active_needs_update = true;
	}
	if !active_needs_update {
		return;
	}
	state.redraw_active_timeline(timeline_list, suppress_selection);
	if let Some(mb) = frame.get_menu_bar() {
		update_menu_labels(&mb, state);
	}
}
