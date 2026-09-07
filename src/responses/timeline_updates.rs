use crate::{
	AppState,
	mastodon::{Poll, Status},
};

pub(super) fn update_poll_in_timelines(state: &mut AppState, poll: &Poll) {
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
pub(super) fn remove_status_from_timelines(state: &mut AppState, status_id: &str) {
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
pub(super) fn update_status_in_timelines<F>(state: &mut AppState, status_id: &str, updater: F)
where
	F: Fn(&mut Status),
{
	for timeline in state.timeline_manager.iter_mut() {
		for entry in &mut timeline.entries {
			if let Some(status) = entry.as_status_mut() {
				if status.id == status_id {
					updater(status);
				}
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
pub(super) fn update_tag_in_timelines(state: &mut AppState, tag_name: &str, following: bool) {
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
