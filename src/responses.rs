//! Processing of network responses and streaming events.

use std::{cell::Cell, sync::mpsc};

use wxdragon::prelude::*;

use crate::{
	AppState, UiCommand,
	config::SortOrder,
	live_region,
	mastodon::{Poll, Status},
	network::{NetworkCommand, NetworkResponse, TimelineData},
	streaming,
	timeline::{TimelineEntry, TimelineType},
	ui::{
		dialogs::{self, UserLookupAction},
		menu::update_menu_labels,
		timeline_view::{sync_timeline_selection_from_list, update_active_timeline_ui},
	},
};

/// Processes streaming events from WebSocket connections.
pub(crate) fn process_stream_events(
	state: &mut AppState,
	timeline_list: &ListBox,
	suppress_selection: &Cell<bool>,
	frame: &Frame,
) {
	let active_type = state.timeline_manager.active().map(|t| t.timeline_type.clone());
	let mut active_needs_update = false;
	for timeline in state.timeline_manager.iter_mut() {
		let handle = match &timeline.stream_handle {
			Some(h) => h,
			None => continue,
		};
		let events = handle.drain();
		let is_active = active_type.as_ref() == Some(&timeline.timeline_type);
		if is_active {
			sync_timeline_selection_from_list(timeline, timeline_list, state.config.sort_order);
		}
		for event in events {
			match event {
				streaming::StreamEvent::Update { timeline_type, status } => {
					if timeline.timeline_type == timeline_type {
						timeline.entries.insert(0, TimelineEntry::Status(*status));
						if is_active {
							active_needs_update = true;
						}
					}
				}
				streaming::StreamEvent::Delete { timeline_type, id } => {
					if timeline.timeline_type == timeline_type {
						timeline.entries.retain(|entry| entry.as_status().map(|s| s.id != id).unwrap_or(true));
						if is_active {
							active_needs_update = true;
						}
					}
				}
				streaming::StreamEvent::Notification { timeline_type, notification } => {
					if timeline.timeline_type == timeline_type {
						timeline.entries.insert(0, TimelineEntry::Notification(*notification));
						if is_active {
							active_needs_update = true;
						}
					}
				}
				streaming::StreamEvent::Connected(timeline_type) => {
					let _ = timeline_type;
				}
				streaming::StreamEvent::Disconnected(timeline_type) => {
					let _ = timeline_type;
				}
				streaming::StreamEvent::Error { timeline_type, message } => {
					let _ = (timeline_type, message);
				}
			}
		}
	}
	if active_needs_update && let Some(active) = state.timeline_manager.active_mut() {
		update_active_timeline_ui(
			timeline_list,
			active,
			suppress_selection,
			state.config.sort_order,
			state.config.timestamp_format,
			state.config.content_warning_display,
			&state.cw_expanded,
		);
		if let Some(mb) = frame.get_menu_bar() {
			update_menu_labels(&mb, state);
		}
	}
}

/// Processes network responses from the background network thread.
#[allow(clippy::too_many_arguments)]
pub(crate) fn process_network_responses(
	frame: &Frame,
	state: &mut AppState,
	timelines_selector: &ListBox,
	timeline_list: &ListBox,
	suppress_selection: &Cell<bool>,
	live_region: &StaticText,
	quick_action_keys_enabled: &Cell<bool>,
	autoload_enabled: &Cell<bool>,
	sort_order_cell: &Cell<SortOrder>,
	tray_hidden: &Cell<bool>,
	ui_tx: &mpsc::Sender<UiCommand>,
) {
	let handle = match &state.network_handle {
		Some(h) => h,
		None => return,
	};
	let active_type = state.timeline_manager.active().map(|t| t.timeline_type.clone());
	for response in handle.drain() {
		match response {
			NetworkResponse::TimelineLoaded { timeline_type, result: Ok(data), max_id } => {
				let is_active = active_type.as_ref() == Some(&timeline_type);
				if let Some(timeline) = state.timeline_manager.get_mut(&timeline_type) {
					if is_active {
						sync_timeline_selection_from_list(timeline, timeline_list, state.config.sort_order);
					}
					let new_entries: Vec<TimelineEntry> = match data {
						TimelineData::Statuses(statuses) => statuses.into_iter().map(TimelineEntry::Status).collect(),
						TimelineData::Notifications(notifications) => {
							notifications.into_iter().map(TimelineEntry::Notification).collect()
						}
					};

					if max_id.is_some() {
						timeline.entries.extend(new_entries.clone());

						if is_active {
							if state.config.sort_order == SortOrder::NewestToOldest {
								for entry in &new_entries {
									let is_expanded = state.cw_expanded.contains(entry.id());
									timeline_list.append(&entry.display_text(
										state.config.timestamp_format,
										state.config.content_warning_display,
										is_expanded,
									));
								}
							} else {
								update_active_timeline_ui(
									timeline_list,
									timeline,
									suppress_selection,
									state.config.sort_order,
									state.config.timestamp_format,
									state.config.content_warning_display,
									&state.cw_expanded,
								);
							}
						}
					} else {
						timeline.entries = new_entries;
						if is_active {
							update_active_timeline_ui(
								timeline_list,
								timeline,
								suppress_selection,
								state.config.sort_order,
								state.config.timestamp_format,
								state.config.content_warning_display,
								&state.cw_expanded,
							);
						}
					}
					timeline.loading_more = false;
				}
			}
			NetworkResponse::TimelineLoaded { timeline_type, result: Err(ref err), .. } => {
				if let Some(timeline) = state.timeline_manager.get_mut(&timeline_type) {
					timeline.loading_more = false;
				}
				live_region::announce(live_region, &format!("Failed to load timeline: {}", err));
			}
			NetworkResponse::AccountLookupResult { handle: _, result: Ok(account) } => {
				let action = state.pending_user_lookup_action.take().unwrap_or(UserLookupAction::Timeline);
				match action {
					UserLookupAction::Profile => {
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
								account.clone(),
								net_tx,
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
							live_region::announce(live_region, "Network not available");
						}
					}
					UserLookupAction::Timeline => {
						let timeline_type = TimelineType::User {
							id: account.id.clone(),
							name: account.display_name_or_username().to_string(),
						};
						crate::commands::handle_ui_command(
							UiCommand::OpenTimeline(timeline_type),
							state,
							frame,
							timelines_selector,
							timeline_list,
							suppress_selection,
							live_region,
							quick_action_keys_enabled,
							autoload_enabled,
							sort_order_cell,
							tray_hidden,
							ui_tx,
						);
					}
				}
			}
			NetworkResponse::AccountLookupResult { handle, result: Err(err) } => {
				state.pending_user_lookup_action = None;
				live_region::announce(live_region, &format!("Failed to find user {}: {}", handle, err));
			}
			NetworkResponse::PostComplete(Ok(())) => {
				live_region::announce(live_region, "Posted");
			}
			NetworkResponse::PostComplete(Err(ref err)) => {
				live_region::announce(live_region, &format!("Failed to post: {}", err));
			}
			NetworkResponse::Favorited { status_id, result: Ok(status) } => {
				update_status_in_timelines(state, &status_id, |s| {
					s.favourited = status.favourited;
					s.favourites_count = status.favourites_count;
				});
				if let Some(mb) = frame.get_menu_bar() {
					update_menu_labels(&mb, state);
				}
				live_region::announce(live_region, "Favorited");
			}
			NetworkResponse::Favorited { result: Err(ref err), .. } => {
				live_region::announce(live_region, &format!("Failed to favorite: {}", err));
			}
			NetworkResponse::Unfavorited { status_id, result: Ok(status) } => {
				update_status_in_timelines(state, &status_id, |s| {
					s.favourited = status.favourited;
					s.favourites_count = status.favourites_count;
				});
				if let Some(mb) = frame.get_menu_bar() {
					update_menu_labels(&mb, state);
				}
				live_region::announce(live_region, "Unfavorited");
			}
			NetworkResponse::Unfavorited { result: Err(ref err), .. } => {
				live_region::announce(live_region, &format!("Failed to unfavorite: {}", err));
			}
			NetworkResponse::Boosted { status_id, result: Ok(status) } => {
				// The returned status is the reblog wrapper, get the inner status
				if let Some(inner) = &status.reblog {
					update_status_in_timelines(state, &status_id, |s| {
						s.reblogged = inner.reblogged;
						s.reblogs_count = inner.reblogs_count;
					});
				}
				if let Some(mb) = frame.get_menu_bar() {
					update_menu_labels(&mb, state);
				}
				live_region::announce(live_region, "Boosted");
			}
			NetworkResponse::Boosted { result: Err(ref err), .. } => {
				live_region::announce(live_region, &format!("Failed to boost: {}", err));
			}
			NetworkResponse::Unboosted { status_id, result: Ok(status) } => {
				update_status_in_timelines(state, &status_id, |s| {
					s.reblogged = status.reblogged;
					s.reblogs_count = status.reblogs_count;
				});
				if let Some(mb) = frame.get_menu_bar() {
					update_menu_labels(&mb, state);
				}
				live_region::announce(live_region, "Unboosted");
			}
			NetworkResponse::Unboosted { result: Err(ref err), .. } => {
				live_region::announce(live_region, &format!("Failed to unboost: {}", err));
			}
			NetworkResponse::Replied(Ok(())) => {
				live_region::announce(live_region, "Reply sent");
			}
			NetworkResponse::Replied(Err(ref err)) => {
				live_region::announce(live_region, &format!("Failed to reply: {}", err));
			}
			NetworkResponse::StatusDeleted { status_id, result: Ok(()) } => {
				remove_status_from_timelines(state, &status_id);
				if let Some(active) = state.timeline_manager.active_mut() {
					update_active_timeline_ui(
						timeline_list,
						active,
						suppress_selection,
						state.config.sort_order,
						state.config.timestamp_format,
						state.config.content_warning_display,
						&state.cw_expanded,
					);
				}
				live_region::announce(live_region, "Deleted");
			}
			NetworkResponse::StatusDeleted { result: Err(ref err), .. } => {
				live_region::announce(live_region, &format!("Failed to delete: {}", err));
			}
			NetworkResponse::StatusEdited { _status_id: _, result: Ok(status) } => {
				let status_clone = status.clone();
				update_status_in_timelines(state, &status.id, move |s| *s = status_clone.clone());
				if let Some(active) = state.timeline_manager.active_mut() {
					update_active_timeline_ui(
						timeline_list,
						active,
						suppress_selection,
						state.config.sort_order,
						state.config.timestamp_format,
						state.config.content_warning_display,
						&state.cw_expanded,
					);
				}
				live_region::announce(live_region, "Edited");
			}
			NetworkResponse::StatusEdited { result: Err(ref err), .. } => {
				live_region::announce(live_region, &format!("Failed to edit: {}", err));
			}
			NetworkResponse::TagFollowed { name, result: Ok(_) } => {
				update_tag_in_timelines(state, &name, true);
				if let Some(dlg) = &state.hashtag_dialog {
					dlg.update_tag(&name, true);
				}
				live_region::announce(live_region, &format!("Followed #{}", name));
			}
			NetworkResponse::TagFollowed { name, result: Err(err) } => {
				live_region::announce(live_region, &format!("Failed to follow #{}: {}", name, err));
			}
			NetworkResponse::TagUnfollowed { name, result: Ok(_) } => {
				update_tag_in_timelines(state, &name, false);
				if let Some(dlg) = &state.hashtag_dialog {
					dlg.update_tag(&name, false);
				}
				live_region::announce(live_region, &format!("Unfollowed #{}", name));
			}
			NetworkResponse::TagUnfollowed { name, result: Err(err) } => {
				live_region::announce(live_region, &format!("Failed to unfollow #{}: {}", name, err));
			}
			NetworkResponse::TagsInfoFetched { result: Ok(tags) } => {
				if let Some(handle) = &state.network_handle {
					let net_tx = handle.command_tx.clone();
					let ui_tx_dlg = ui_tx.clone();
					let dlg = dialogs::HashtagDialog::new(frame, tags, net_tx, move || {
						let _ = ui_tx_dlg.send(UiCommand::HashtagDialogClosed);
					});
					dlg.show();
					state.hashtag_dialog = Some(dlg);
				}
			}
			NetworkResponse::TagsInfoFetched { result: Err(err) } => {
				live_region::announce(live_region, &format!("Failed to load hashtags: {}", err));
			}
			NetworkResponse::RelationshipUpdated { _account_id: _, target_name, action, result } => match result {
				Ok(rel) => {
					if let Some(dlg) = &state.profile_dialog {
						dlg.update_relationship(rel);
					}
					let msg = match action {
						crate::network::RelationshipAction::Follow => format!("Followed {}", target_name),
						crate::network::RelationshipAction::Unfollow => format!("Unfollowed {}", target_name),
						crate::network::RelationshipAction::Block => format!("Blocked {}", target_name),
						crate::network::RelationshipAction::Unblock => format!("Unblocked {}", target_name),
						crate::network::RelationshipAction::Mute => format!("Muted {}", target_name),
						crate::network::RelationshipAction::Unmute => format!("Unmuted {}", target_name),
						crate::network::RelationshipAction::ShowBoosts => {
							format!("Showing boosts from {}", target_name)
						}
						crate::network::RelationshipAction::HideBoosts => format!("Hiding boosts from {}", target_name),
					};
					live_region::announce(live_region, &msg);
				}
				Err(err) => {
					live_region::announce(live_region, &format!("Failed to update relationship: {}", err));
				}
			},
			NetworkResponse::RelationshipLoaded { _account_id: _, result } => {
				if let Ok(rel) = result
					&& let Some(dlg) = &state.profile_dialog
				{
					dlg.update_relationship(rel);
				}
			}
			NetworkResponse::AccountFetched { result } => {
				if let Ok(account) = result
					&& let Some(dlg) = &state.profile_dialog
				{
					dlg.update_account(account);
				}
			}
			NetworkResponse::PollVoted { result } => match result {
				Ok(poll) => {
					update_poll_in_timelines(state, &poll);
					if let Some(active) = state.timeline_manager.active_mut() {
						update_active_timeline_ui(
							timeline_list,
							active,
							suppress_selection,
							state.config.sort_order,
							state.config.timestamp_format,
							state.config.content_warning_display,
							&state.cw_expanded,
						);
					}
					live_region::announce(live_region, "Vote submitted");
				}
				Err(err) => {
					live_region::announce(live_region, &format!("Failed to vote: {}", err));
				}
			},
			NetworkResponse::CredentialsFetched { result: Ok(account) } => {
				if let Some(update) = dialogs::prompt_for_profile_edit(frame, &account)
					&& let Some(handle) = &state.network_handle
				{
					handle.send(NetworkCommand::UpdateProfile { update });
				}
			}
			NetworkResponse::CredentialsFetched { result: Err(err) } => {
				live_region::announce(live_region, &format!("Failed to fetch profile: {}", err));
			}
			NetworkResponse::ProfileUpdated { result: Ok(_) } => {
				live_region::announce(live_region, "Profile updated");
				let _ = ui_tx.send(UiCommand::Refresh);
			}
			NetworkResponse::ProfileUpdated { result: Err(err) } => {
				live_region::announce(live_region, &format!("Failed to update profile: {}", err));
			}
		}
	}
	let _ = frame;
}

/// Updates a poll in all timelines where it appears.
pub(crate) fn update_poll_in_timelines(state: &mut AppState, poll: &Poll) {
	for timeline in state.timeline_manager.iter_mut() {
		for entry in &mut timeline.entries {
			if let Some(status) = entry.as_status_mut() {
				if let Some(p) = &mut status.poll
					&& p.id == poll.id
				{
					*p = poll.clone();
				}
				if let Some(reblog) = &mut status.reblog
					&& let Some(p) = &mut reblog.poll
					&& p.id == poll.id
				{
					*p = poll.clone();
				}
			}
		}
	}
}

/// Removes a status from all timelines.
pub(crate) fn remove_status_from_timelines(state: &mut AppState, status_id: &str) {
	for timeline in state.timeline_manager.iter_mut() {
		timeline.entries.retain(|entry| {
			if let Some(status) = entry.as_status() {
				if status.id == status_id {
					return false;
				}
				if let Some(reblog) = &status.reblog
					&& reblog.id == status_id
				{
					return false;
				}
			}
			true
		});
	}
}

/// Updates a status in all timelines where it appears.
pub(crate) fn update_status_in_timelines<F>(state: &mut AppState, status_id: &str, updater: F)
where
	F: Fn(&mut Status),
{
	for timeline in state.timeline_manager.iter_mut() {
		for entry in &mut timeline.entries {
			if let Some(status) = entry.as_status_mut() {
				// Check the status itself
				if status.id == status_id {
					updater(status);
				}
				// Check if it's a reblog of the target
				if let Some(ref mut reblog) = status.reblog
					&& reblog.id == status_id
				{
					updater(reblog);
				}
			}
		}
	}
}

/// Updates the following state of a tag in all timelines.
pub(crate) fn update_tag_in_timelines(state: &mut AppState, tag_name: &str, following: bool) {
	for timeline in state.timeline_manager.iter_mut() {
		for entry in &mut timeline.entries {
			if let Some(status) = entry.as_status_mut() {
				let check_status = |s: &mut Status| {
					for tag in &mut s.tags {
						if tag.name.eq_ignore_ascii_case(tag_name) {
							tag.following = following;
						}
					}
				};
				check_status(status);
				if let Some(ref mut reblog) = status.reblog {
					check_status(reblog);
				}
			}
		}
	}
}
