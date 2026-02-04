use std::{cell::Cell, collections::HashSet};

use wxdragon::prelude::ListBox;

use crate::{
	config::{ContentWarningDisplay, SortOrder, TimestampFormat},
	timeline::{Timeline, TimelineEntry, TimelineType},
};

pub fn update_timeline_ui(
	timeline_list: &ListBox,
	entries: &[TimelineEntry],
	sort_order: SortOrder,
	timestamp_format: TimestampFormat,
	cw_display: ContentWarningDisplay,
	cw_expanded: &HashSet<String>,
) {
	let iter: Box<dyn Iterator<Item = &TimelineEntry>> = match sort_order {
		SortOrder::NewestToOldest => Box::new(entries.iter()),
		SortOrder::OldestToNewest => Box::new(entries.iter().rev()),
	};

	let count = timeline_list.get_count() as usize;
	if count == entries.len() {
		for (i, entry) in iter.enumerate() {
			let is_expanded = cw_expanded.contains(entry.id());
			let text = entry.display_text(timestamp_format, cw_display, is_expanded);
			if let Some(current) = timeline_list.get_string(i as u32) {
				if current != text {
					timeline_list.set_string(i as u32, &text);
				}
			} else {
				timeline_list.set_string(i as u32, &text);
			}
		}
	} else {
		timeline_list.clear();
		for entry in iter {
			let is_expanded = cw_expanded.contains(entry.id());
			timeline_list.append(&entry.display_text(timestamp_format, cw_display, is_expanded));
		}
	}
}

pub fn with_suppressed_selection<T>(suppress_selection: &Cell<bool>, f: impl FnOnce() -> T) -> T {
	suppress_selection.set(true);
	let result = f();
	suppress_selection.set(false);
	result
}

pub const fn list_index_to_entry_index(list_index: usize, entries_len: usize, sort_order: SortOrder) -> Option<usize> {
	if list_index >= entries_len {
		return None;
	}
	match sort_order {
		SortOrder::NewestToOldest => Some(list_index),
		SortOrder::OldestToNewest => Some(entries_len - 1 - list_index),
	}
}

pub const fn entry_index_to_list_index(entry_index: usize, entries_len: usize, sort_order: SortOrder) -> Option<usize> {
	if entry_index >= entries_len {
		return None;
	}
	match sort_order {
		SortOrder::NewestToOldest => Some(entry_index),
		SortOrder::OldestToNewest => Some(entries_len - 1 - entry_index),
	}
}

pub fn sync_timeline_selection_from_list(timeline: &mut Timeline, timeline_list: &ListBox, sort_order: SortOrder) {
	let selection = timeline_list.get_selection().map(|sel| sel as usize);
	timeline.selected_index = selection;
	timeline.selected_id = selection
		.and_then(|list_index| list_index_to_entry_index(list_index, timeline.entries.len(), sort_order))
		.map(|entry_index| timeline.entries[entry_index].id().to_string());
}

pub fn apply_timeline_selection(timeline_list: &ListBox, timeline: &mut Timeline, sort_order: SortOrder) {
	if timeline.entries.is_empty() {
		timeline.selected_index = None;
		timeline.selected_id = None;
		return;
	}
	let entries_len = timeline.entries.len();
	let is_thread = matches!(timeline.timeline_type, TimelineType::Thread { .. });
	let selection = timeline
		.selected_id
		.as_deref()
		.and_then(|selected_id| {
			timeline
				.entries
				.iter()
				.position(|entry| entry.id() == selected_id)
				.and_then(|entry_index| entry_index_to_list_index(entry_index, entries_len, sort_order))
		})
		.or_else(|| timeline.selected_index.filter(|&sel| sel < entries_len))
		.unwrap_or_else(|| {
			if is_thread {
				// For threads, select the first (oldest) post so users can read sequentially
				match sort_order {
					SortOrder::NewestToOldest => entries_len - 1,
					SortOrder::OldestToNewest => 0,
				}
			} else {
				match sort_order {
					SortOrder::NewestToOldest => 0,
					SortOrder::OldestToNewest => entries_len - 1,
				}
			}
		});
	timeline.selected_index = Some(selection);
	timeline.selected_id = list_index_to_entry_index(selection, entries_len, sort_order)
		.map(|entry_index| timeline.entries[entry_index].id().to_string());

	let current_ui_sel = timeline_list.get_selection().map(|s| s as usize);
	if current_ui_sel != Some(selection) {
		timeline_list.set_selection(selection as u32, true);
	}
}

pub fn update_active_timeline_ui(
	timeline_list: &ListBox,
	timeline: &mut Timeline,
	suppress_selection: &Cell<bool>,
	sort_order: SortOrder,
	timestamp_format: TimestampFormat,
	cw_display: ContentWarningDisplay,
	cw_expanded: &HashSet<String>,
	preserve_thread_order: bool,
) {
	let effective_sort_order = if preserve_thread_order && matches!(timeline.timeline_type, TimelineType::Thread { .. })
	{
		SortOrder::OldestToNewest
	} else {
		sort_order
	};
	with_suppressed_selection(suppress_selection, || {
		update_timeline_ui(
			timeline_list,
			&timeline.entries,
			effective_sort_order,
			timestamp_format,
			cw_display,
			cw_expanded,
		);
		apply_timeline_selection(timeline_list, timeline, effective_sort_order);
	});
}
