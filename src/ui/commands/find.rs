//! Find-in-timeline commands.

use super::{UiCommand, UiCommandContext, handle_ui_command};
use crate::{
	config::{FindLoadingMode, SortOrder},
	timeline::Timeline,
	ui::{
		timeline_list::TimelineList,
		timeline_view::{entry_id_to_node_id, entry_index_to_list_index},
	},
};

fn select_entry(active: &mut Timeline, entry_index: usize, sort_order: SortOrder, timeline_list: &TimelineList) {
	active.selected_index = entry_index_to_list_index(entry_index, active.entries.len(), sort_order);
	active.selected_id = Some(active.entries[entry_index].id().to_string());
	timeline_list.set_selection(active.selected_id.as_deref().map(entry_id_to_node_id));
}

pub(super) fn find(ctx: &mut UiCommandContext<'_>, query: String) {
	let state = &mut *ctx.state;
	let Some(active) = state.timeline_manager.active_mut() else { return };
	active.find_query = Some(query);
	if let Some(index) = active.find_next(0, &state.config) {
		select_entry(active, index, active.effective_sort_order(&state.config), &ctx.timeline_list);
	} else {
		ctx.live_region.announce("Not found");
	}
}

pub(super) fn find_next(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	let Some(active) = state.timeline_manager.active_mut().filter(|a| a.find_query.is_some()) else {
		ctx.live_region.announce("No active search");
		return;
	};
	let sort_order = active.effective_sort_order(&state.config);
	let start_index = active.selected_index.map_or(0, |i| i + 1);
	if let Some(index) = active.find_next(start_index, &state.config) {
		select_entry(active, index, sort_order, &ctx.timeline_list);
		ctx.live_region.announce("Found next");
	} else if state.config.find_loading_mode == FindLoadingMode::LoadOnNext && sort_order == SortOrder::NewestToOldest {
		active.pending_find_next = true;
		ctx.live_region.announce("Loading more...");
		handle_ui_command(UiCommand::LoadMore, ctx);
	} else if let Some(index) = active.find_next(0, &state.config) {
		select_entry(active, index, sort_order, &ctx.timeline_list);
		ctx.live_region.announce("Wrapped to top");
	} else {
		ctx.live_region.announce("Not found");
	}
}

pub(super) fn find_prev(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	let Some(active) = state.timeline_manager.active_mut().filter(|a| a.find_query.is_some()) else {
		ctx.live_region.announce("No active search");
		return;
	};
	let sort_order = active.effective_sort_order(&state.config);
	let start_index = active.selected_index.unwrap_or(active.entries.len());
	if let Some(index) = active.find_prev(start_index, &state.config) {
		select_entry(active, index, sort_order, &ctx.timeline_list);
	} else if state.config.find_loading_mode == FindLoadingMode::LoadOnNext && sort_order == SortOrder::OldestToNewest {
		active.pending_find_prev = true;
		ctx.live_region.announce("Loading more...");
		handle_ui_command(UiCommand::LoadMore, ctx);
	} else if let Some(index) = active.find_prev(active.entries.len(), &state.config) {
		select_entry(active, index, sort_order, &ctx.timeline_list);
		ctx.live_region.announce("Wrapped to bottom");
	} else {
		ctx.live_region.announce("Not found");
	}
}
