//! Helpers for reading the currently selected timeline entry.

use url::Url;

use crate::{
	AppState,
	config::SortOrder,
	mastodon::Status,
	timeline::{TimelineEntry, TimelineType},
};

pub(super) fn paging_max_id(entries: &[TimelineEntry]) -> Option<String> {
	let mut min_id: Option<u128> = None;
	let mut min_id_str: Option<String> = None;
	for entry in entries {
		let id_str = entry.id();
		if let Ok(id) = id_str.parse::<u128>()
			&& min_id.is_none_or(|current| id < current)
		{
			min_id = Some(id);
			min_id_str = Some(id_str.to_string());
		}
	}
	min_id_str.or_else(|| entries.last().map(|entry| entry.id().to_string()))
}

/// Gets the currently selected timeline entry.
pub(super) fn get_selected_entry(state: &AppState) -> Option<&TimelineEntry> {
	let timeline = state.timeline_manager.active()?;
	let index = timeline.selected_index?;

	let effective_sort_order =
		if state.config.preserve_thread_order && matches!(timeline.timeline_type, TimelineType::Thread { .. }) {
			SortOrder::OldestToNewest
		} else {
			state.config.sort_order
		};

	let final_index = match effective_sort_order {
		SortOrder::NewestToOldest => index,
		SortOrder::OldestToNewest => timeline.entries.len().checked_sub(1)?.checked_sub(index)?,
	};

	timeline.entries.get(final_index)
}

/// Gets the currently selected status (unwrapping from notification if needed).
pub fn get_selected_status(state: &AppState) -> Option<&Status> {
	get_selected_entry(state)?.as_status()
}

/// Returns the URL if the active timeline is for a foreign instance.
pub(super) fn foreign_url(state: &AppState, url: Option<&String>) -> Option<String> {
	if matches!(state.timeline_manager.active().map(|t| &t.timeline_type), Some(TimelineType::InstanceLocal { .. })) {
		url.cloned()
	} else {
		None
	}
}

/// Derive a Mastodon `acct` (`user` or `user@instance`) from the display text and URL of a
/// mention link parsed out of post HTML.  The display text from Mastodon HTML is already in the
/// right format (`@user` or `@user@instance`), so we prefer it; the URL is the fallback.
pub(super) fn acct_from_mention_link(display_text: &str, url: &str) -> String {
	let text = display_text.trim().trim_start_matches('@');
	if text.contains('@') {
		return text.to_string();
	}
	if let Ok(parsed) = Url::parse(url)
		&& let Some(host) = parsed.host_str()
	{
		let path = parsed.path();
		let username = path.trim_start_matches('/').trim_start_matches('@');
		let username = username.split('/').next().unwrap_or(text);
		if !username.is_empty() {
			return format!("{username}@{host}");
		}
	}
	text.to_string()
}
