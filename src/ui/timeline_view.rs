use std::{
	cell::Cell,
	collections::{HashSet, hash_map::DefaultHasher},
	hash::{Hash, Hasher},
};

use accesskit::NodeId;

use crate::{
	config::{Config, SortOrder},
	timeline::{Timeline, TimelineEntry, TimelineTextOptions, TimelineType},
	ui::timeline_list::TimelineList,
};

pub fn entry_id_to_node_id(id: &str) -> NodeId {
	let mut hasher = DefaultHasher::new();
	id.hash(&mut hasher);
	let hash = hasher.finish();
	let hash = if hash == 0 { 1 } else { hash };
	NodeId(hash)
}

#[derive(Debug, Clone)]
pub struct TimelineViewOptions {
	pub sort_order: SortOrder,
	pub preserve_thread_order: bool,
	pub text_options: TimelineTextOptions,
}

impl TimelineViewOptions {
	pub fn from_config(config: &Config, timeline_type: &TimelineType) -> Self {
		Self {
			sort_order: config.sort_order,
			preserve_thread_order: config.preserve_thread_order,
			text_options: TimelineTextOptions::from_config(config, timeline_type),
		}
	}
}

pub fn update_timeline_ui(
	timeline_list: &TimelineList,
	entries: &[TimelineEntry],
	sort_order: SortOrder,
	text_options: &TimelineTextOptions,
	cw_expanded: &HashSet<String>,
	_timeline_index: usize,
	selected_id: Option<&str>,
) {
	let iter: Box<dyn Iterator<Item = &TimelineEntry>> = match sort_order {
		SortOrder::NewestToOldest => Box::new(entries.iter()),
		SortOrder::OldestToNewest => Box::new(entries.iter().rev()),
	};

	let mut list_entries = Vec::with_capacity(entries.len());
	for entry in iter {
		let is_expanded = cw_expanded.contains(entry.id());
		let text = entry.display_text(text_options, is_expanded);
		list_entries.push((entry_id_to_node_id(entry.id()), text));
	}

	let selected_node_id = selected_id.map(entry_id_to_node_id);
	timeline_list.update_entries(&list_entries, selected_node_id);
}

pub fn with_suppressed_selection<T>(suppress_selection: &Cell<bool>, f: impl FnOnce() -> T) -> T {
	suppress_selection.set(true);
	let result = f();
	suppress_selection.set(false);
	result
}

pub fn with_frozen_listbox<T>(_listbox: &TimelineList, f: impl FnOnce() -> T) -> T {
	// Custom TimelineList doesn't need freeze/thaw right now, but kept for compatibility
	f()
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

pub fn sync_timeline_selection_from_list(
	_timeline: &mut Timeline,
	_timeline_list: &TimelineList,
	_sort_order: SortOrder,
) {
	// Selection is driven by Timeline state and keyboard events on TimelineList,
	// so this direction is less relevant for the virtual tree unless reacting to UI Automation events.
}

pub fn apply_timeline_selection(timeline_list: &TimelineList, timeline: &mut Timeline, sort_order: SortOrder) {
	if timeline.entries.is_empty() {
		timeline.selected_index = None;
		timeline.selected_id = None;
		timeline_list.set_selection(None);
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
}

pub fn update_active_timeline_ui(
	timeline_list: &TimelineList,
	timeline: &mut Timeline,
	suppress_selection: &Cell<bool>,
	options: &TimelineViewOptions,
	cw_expanded: &HashSet<String>,
	timeline_index: usize,
) {
	let effective_sort_order =
		if options.preserve_thread_order && matches!(timeline.timeline_type, TimelineType::Thread { .. }) {
			SortOrder::OldestToNewest
		} else {
			options.sort_order
		};
	with_frozen_listbox(timeline_list, || {
		with_suppressed_selection(suppress_selection, || {
			// Apply selection before update so we can pass the correctly calculated selection ID
			apply_timeline_selection(timeline_list, timeline, effective_sort_order);

			update_timeline_ui(
				timeline_list,
				&timeline.entries,
				effective_sort_order,
				&options.text_options,
				cw_expanded,
				timeline_index,
				timeline.selected_id.as_deref(),
			);
		});
	});
}
