use std::{cell::Cell, collections::HashSet};

use wxdragon::{WxWidget, prelude::ListBox};

use crate::{
	config::{Config, SortOrder},
	timeline::{Timeline, TimelineEntry, TimelineTextOptions, TimelineType},
};

#[derive(Debug, Clone, Copy)]
pub struct TimelineViewOptions {
	pub sort_order: SortOrder,
	pub preserve_thread_order: bool,
	pub text_options: TimelineTextOptions,
}

impl TimelineViewOptions {
	pub fn from_config(config: &Config) -> Self {
		Self {
			sort_order: config.sort_order,
			preserve_thread_order: config.preserve_thread_order,
			text_options: TimelineTextOptions::from_config(config),
		}
	}
}

pub fn update_timeline_ui(
	timeline_list: ListBox,
	entries: &[TimelineEntry],
	sort_order: SortOrder,
	text_options: TimelineTextOptions,
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
			let text = entry.display_text(text_options, is_expanded);
			let Ok(index) = u32::try_from(i) else { continue };
			if let Some(current) = timeline_list.get_string(index) {
				if current != text {
					timeline_list.set_string(index, &text);
				}
			} else {
				timeline_list.set_string(index, &text);
			}
		}
	} else {
		timeline_list.clear();
		for entry in iter {
			let is_expanded = cw_expanded.contains(entry.id());
			timeline_list.append(&entry.display_text(text_options, is_expanded));
		}
	}
}

pub fn with_suppressed_selection<T>(suppress_selection: &Cell<bool>, f: impl FnOnce() -> T) -> T {
	suppress_selection.set(true);
	let result = f();
	suppress_selection.set(false);
	result
}

pub fn with_frozen_listbox<T>(listbox: ListBox, f: impl FnOnce() -> T) -> T {
	listbox.freeze();
	struct ThawOnDrop(ListBox);
	impl Drop for ThawOnDrop {
		fn drop(&mut self) {
			self.0.thaw();
		}
	}
	let thaw_guard = ThawOnDrop(listbox);
	let result = f();
	drop(thaw_guard);
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

pub fn sync_timeline_selection_from_list(timeline: &mut Timeline, timeline_list: ListBox, sort_order: SortOrder) {
	let selection = timeline_list.get_selection().map(|sel| sel as usize);
	timeline.selected_index = selection;
	timeline.selected_id = selection
		.and_then(|list_index| list_index_to_entry_index(list_index, timeline.entries.len(), sort_order))
		.map(|entry_index| timeline.entries[entry_index].id().to_string());
}

pub fn apply_timeline_selection(timeline_list: ListBox, timeline: &mut Timeline, sort_order: SortOrder) {
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
	if current_ui_sel != Some(selection)
		&& let Ok(sel) = u32::try_from(selection)
	{
		timeline_list.set_selection(sel, true);
	}
}

pub fn update_active_timeline_ui(
	timeline_list: ListBox,
	timeline: &mut Timeline,
	suppress_selection: &Cell<bool>,
	options: TimelineViewOptions,
	cw_expanded: &HashSet<String>,
) {
	let effective_sort_order =
		if options.preserve_thread_order && matches!(timeline.timeline_type, TimelineType::Thread { .. }) {
			SortOrder::OldestToNewest
		} else {
			options.sort_order
		};
	with_frozen_listbox(timeline_list, || {
		with_suppressed_selection(suppress_selection, || {
			update_timeline_ui(
				timeline_list,
				&timeline.entries,
				effective_sort_order,
				options.text_options,
				cw_expanded,
			);
			apply_timeline_selection(timeline_list, timeline, effective_sort_order);
		});
	});
}
