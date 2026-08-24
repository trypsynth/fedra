//! Find-in-timeline commands.

use super::{UiCommand, UiCommandContext, handle_ui_command};
use crate::{config, config::SortOrder};

pub(super) fn find(ctx: &mut UiCommandContext<'_>, query: String) {
	let state = &mut *ctx.state;
	let timeline_list = &ctx.timeline_list;
	let live_region = ctx.live_region;
	if let Some(active) = state.timeline_manager.active_mut() {
		active.find_query = Some(query);
		if let Some(index) = active.find_next(0, &state.config) {
			let list_index = crate::ui::timeline_view::entry_index_to_list_index(
				index,
				active.entries.len(),
				active.effective_sort_order(&state.config),
			);
			if let Some(idx) = list_index {
				active.selected_index = Some(idx);
				let effective_sort_order = active.effective_sort_order(&state.config);
				active.selected_id = crate::ui::timeline_view::list_index_to_entry_index(
					idx,
					active.entries.len(),
					effective_sort_order,
				)
				.map(|entry_index| active.entries[entry_index].id().to_string());
				timeline_list
					.set_selection(active.selected_id.as_deref().map(crate::ui::timeline_view::entry_id_to_node_id));
			}
		} else {
			live_region.announce("Not found");
		}
	}
}

pub(super) fn find_next(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	let timeline_list = &ctx.timeline_list;
	let live_region = ctx.live_region;
	if let Some(active) = state.timeline_manager.active_mut()
		&& active.find_query.is_some()
	{
		let start_index = active.selected_index.map_or(0, |i| i + 1);
		let found_index = if let Some(index) = active.find_next(start_index, &state.config) {
			Some(index)
		} else {
			// wrap around
			active.find_next(0, &state.config)
		};

		if let Some(index) = found_index {
			let list_index = crate::ui::timeline_view::entry_index_to_list_index(
				index,
				active.entries.len(),
				active.effective_sort_order(&state.config),
			);
			if let Some(idx) = list_index {
				active.selected_index = Some(idx);
				let effective_sort_order = active.effective_sort_order(&state.config);
				active.selected_id = crate::ui::timeline_view::list_index_to_entry_index(
					idx,
					active.entries.len(),
					effective_sort_order,
				)
				.map(|entry_index| active.entries[entry_index].id().to_string());

				timeline_list
					.set_selection(active.selected_id.as_deref().map(crate::ui::timeline_view::entry_id_to_node_id));
				live_region.announce("Found next");
			}
		} else {
			match state.config.find_loading_mode {
				config::FindLoadingMode::None => {
					if let Some(index) = active.find_next(0, &state.config) {
						let list_index = crate::ui::timeline_view::entry_index_to_list_index(
							index,
							active.entries.len(),
							active.effective_sort_order(&state.config),
						);
						if let Some(idx) = list_index {
							active.selected_index = Some(idx);
							let effective_sort_order = active.effective_sort_order(&state.config);
							active.selected_id = crate::ui::timeline_view::list_index_to_entry_index(
								idx,
								active.entries.len(),
								effective_sort_order,
							)
							.map(|entry_index| active.entries[entry_index].id().to_string());

							timeline_list.set_selection(
								active.selected_id.as_deref().map(crate::ui::timeline_view::entry_id_to_node_id),
							);
							live_region.announce("Wrapped to top");
						}
					} else {
						active.selected_index = Some(0);
						active.selected_id = active.entries.first().map(|e| e.id().to_string());

						timeline_list.set_selection(
							active.selected_id.as_deref().map(crate::ui::timeline_view::entry_id_to_node_id),
						);
						live_region.announce("Wrapped to top");
					}
				}
				config::FindLoadingMode::LoadOnNext => {
					active.pending_find_next = true;
					live_region.announce("Loading more...");
					handle_ui_command(UiCommand::LoadMore, ctx);
				}
			}
		}
	} else {
		live_region.announce("No active search");
	}
}

pub(super) fn find_prev(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	let timeline_list = &ctx.timeline_list;
	let live_region = ctx.live_region;
	if let Some(active) = state.timeline_manager.active_mut()
		&& active.find_query.is_some()
	{
		let start_index = active.selected_index.unwrap_or(active.entries.len());

		if let Some(index) = active.find_prev(start_index, &state.config) {
			let list_index = crate::ui::timeline_view::entry_index_to_list_index(
				index,
				active.entries.len(),
				active.effective_sort_order(&state.config),
			);
			if let Some(idx) = list_index {
				active.selected_index = Some(idx);
				let effective_sort_order = active.effective_sort_order(&state.config);
				active.selected_id = crate::ui::timeline_view::list_index_to_entry_index(
					idx,
					active.entries.len(),
					effective_sort_order,
				)
				.map(|entry_index| active.entries[entry_index].id().to_string());
				timeline_list
					.set_selection(active.selected_id.as_deref().map(crate::ui::timeline_view::entry_id_to_node_id));
			}
		} else {
			let effective_sort_order = active.effective_sort_order(&state.config);

			match state.config.find_loading_mode {
				config::FindLoadingMode::None => {
					if let Some(index) = active.find_prev(active.entries.len(), &state.config) {
						let list_index = crate::ui::timeline_view::entry_index_to_list_index(
							index,
							active.entries.len(),
							active.effective_sort_order(&state.config),
						);
						if let Some(idx) = list_index {
							active.selected_index = Some(idx);
							active.selected_id = crate::ui::timeline_view::list_index_to_entry_index(
								idx,
								active.entries.len(),
								effective_sort_order,
							)
							.map(|entry_index| active.entries[entry_index].id().to_string());

							timeline_list.set_selection(
								active.selected_id.as_deref().map(crate::ui::timeline_view::entry_id_to_node_id),
							);
							live_region.announce("Wrapped to bottom");
						}
					} else {
						active.selected_index = Some(active.entries.len() - 1);
						active.selected_id = active.entries.last().map(|e| e.id().to_string());

						timeline_list.set_selection(
							active.selected_id.as_deref().map(crate::ui::timeline_view::entry_id_to_node_id),
						);
						live_region.announce("Wrapped to bottom");
					}
				}
				config::FindLoadingMode::LoadOnNext => {
					if effective_sort_order == SortOrder::OldestToNewest {
						active.pending_find_prev = true;
						live_region.announce("Loading more...");
						handle_ui_command(UiCommand::LoadMore, ctx);
					} else {
						if let Some(index) = active.find_prev(active.entries.len(), &state.config) {
							let list_index = crate::ui::timeline_view::entry_index_to_list_index(
								index,
								active.entries.len(),
								active.effective_sort_order(&state.config),
							);
							if let Some(idx) = list_index {
								active.selected_index = Some(idx);
								active.selected_id = crate::ui::timeline_view::list_index_to_entry_index(
									idx,
									active.entries.len(),
									effective_sort_order,
								)
								.map(|entry_index| active.entries[entry_index].id().to_string());

								timeline_list.set_selection(
									active.selected_id.as_deref().map(crate::ui::timeline_view::entry_id_to_node_id),
								);
								live_region.announce("Wrapped to bottom");
							}
						} else {
							live_region.announce("No matches found");
						}
					}
				}
			}
		}
	} else {
		live_region.announce("No active search");
	}
}
