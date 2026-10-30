use std::{
	cell::{Cell, RefCell},
	rc::Rc,
};

use wxdragon::prelude::*;

use crate::{
	UiCommand,
	config::{ActionId, AutoloadMode, ShortcutsConfig, SortOrder},
	ui::{
		keys,
		menu::{PostMenuState, action_for_id, build_context_menu, build_menu_bar},
		shortcuts::{dispatch_action, timeline_index_for_key},
	},
	ui_wake::UiCommandSender,
};

pub struct WindowParts {
	pub frame: Frame,
	pub timelines_selector: ListBox,
	pub timeline_list: crate::ui::timeline_list::TimelineList,
}

pub fn build_main_window() -> WindowParts {
	let frame = Frame::builder().with_title("Fedra").with_size(Size::new(800, 600)).build();
	wxdragon::app::set_top_window(&frame);
	let menu_bar = build_menu_bar();
	frame.set_menu_bar(menu_bar);
	let panel = Panel::builder(&frame).build();
	let sizer = BoxSizer::builder(Orientation::Horizontal).build();
	// NVDA names the list after the label created before it; VoiceOver doesn't, so it's only noise there.
	let timelines_label = cfg!(windows).then(|| StaticText::builder(&panel).with_label("Timelines").build());
	let timelines_selector = ListBox::builder(&panel).with_choices(vec!["Home".to_string()]).build();
	timelines_selector.set_selection(0_u32, true);
	let timeline_list = crate::ui::timeline_list::TimelineList::new(&panel);
	let timelines_sizer = BoxSizer::builder(Orientation::Vertical).build();
	if let Some(timelines_label) = timelines_label {
		timelines_sizer.add(&timelines_label, 0, SizerFlag::All, 8);
	}
	timelines_sizer.add(
		&timelines_selector,
		1,
		SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Bottom,
		8,
	);
	sizer.add_sizer(&timelines_sizer, 1, SizerFlag::Expand, 0);
	sizer.add(&timeline_list, 3, SizerFlag::Expand | SizerFlag::All, 8);
	panel.set_sizer(sizer, true);
	let frame_sizer = BoxSizer::builder(Orientation::Vertical).build();
	frame_sizer.add(&panel, 1, SizerFlag::Expand, 0);
	frame.set_sizer(frame_sizer, true);
	WindowParts { frame, timelines_selector, timeline_list }
}

pub fn bind_input_handlers(
	parts: &WindowParts,
	ui_tx: UiCommandSender,
	is_shutting_down: Rc<Cell<bool>>,
	suppress_selection: Rc<Cell<bool>>,
	quick_action_keys_enabled: &Rc<Cell<bool>>,
	autoload_mode: Rc<Cell<AutoloadMode>>,
	sort_order_cell: Rc<Cell<SortOrder>>,
	post_menu_state: Rc<Cell<PostMenuState>>,
	shortcuts_cell: Rc<RefCell<ShortcutsConfig>>,
) {
	#[cfg(target_os = "macos")]
	super::mac_shortcuts::install(
		parts,
		ui_tx.clone(),
		is_shutting_down.clone(),
		quick_action_keys_enabled.clone(),
		shortcuts_cell.clone(),
	);
	let ui_tx_selector = ui_tx.clone();
	let shutdown_selector = is_shutting_down.clone();
	let suppress_selector = suppress_selection.clone();
	let timelines_selector = parts.timelines_selector;
	timelines_selector.on_selection_changed(move |event| {
		if shutdown_selector.get() {
			return;
		}
		if suppress_selector.get() {
			return;
		}
		if let Some(index) = event.get_selection()
			&& let Ok(index) = usize::try_from(index)
		{
			let _ = ui_tx_selector.send(UiCommand::TimelineSelectionChanged(index));
		}
	});
	let ui_tx_delete = ui_tx.clone();
	let shutdown_delete = is_shutting_down.clone();
	let quick_action_keys_selector = quick_action_keys_enabled.clone();
	let timelines_selector_delete = parts.timelines_selector;
	let selector_frame = parts.frame;
	let shortcuts_selector = shortcuts_cell.clone();
	timelines_selector_delete.on_key_down(move |event| {
		if shutdown_delete.get() {
			return;
		}
		if let WindowEventData::Keyboard(ref key_event) = event {
			let ctrl = key_event.control_down();
			let shift = key_event.shift_down();
			let alt = key_event.alt_down();
			if let Some(k) = key_event.get_key_code() {
				let quick_mode = quick_action_keys_selector.get();
				if let Some(index) = timeline_index_for_key(k, quick_mode, ctrl, alt, shift) {
					let _ = ui_tx_delete.send(UiCommand::SwitchTimelineByIndex(index));
					event.skip(false);
					return;
				}
				if let Some(action) = shortcuts_selector.borrow().find_action(quick_mode, k, ctrl, alt, shift)
					&& matches!(
						action,
						ActionId::SwitchPrevAccount
							| ActionId::SwitchNextAccount
							| ActionId::MoveTimelineLeft
							| ActionId::MoveTimelineRight
							| ActionId::CloseTimeline
							| ActionId::TogglePermanentTimeline
							| ActionId::ToggleTimelineNotifications
							| ActionId::ClearTimeline
							| ActionId::ClearAllTimelines
							| ActionId::SwitchPrevTimeline
							| ActionId::SwitchNextTimeline
					) {
					dispatch_action(action, &selector_frame, &ui_tx_delete, &quick_action_keys_selector);
					event.skip(false);
					return;
				}
			}
		}
		event.skip(true);
	});
	let ui_tx_list_key = ui_tx.clone();
	let shutdown_list_key = is_shutting_down.clone();
	let quick_action_keys_list = quick_action_keys_enabled.clone();
	let autoload_mode_list = autoload_mode.clone();
	let sort_order_list = sort_order_cell.clone();
	let timeline_list_key = parts.timeline_list.clone();
	let find_frame = parts.frame;
	let shortcuts_list_key = shortcuts_cell.clone();
	parts.timeline_list.on_key_down(move |event| {
		let WindowEventData::Keyboard(key_event) = event else {
			return false;
		};
		if shutdown_list_key.get() {
			return false;
		}
		let ctrl = key_event.control_down();
		let shift = key_event.shift_down();
		let alt = key_event.alt_down();
		let Some(k) = key_event.get_key_code() else {
			return false;
		};
		let quick_mode = quick_action_keys_list.get();
		if let Some(index) = timeline_index_for_key(k, quick_mode, ctrl, alt, shift) {
			let _ = ui_tx_list_key.send(UiCommand::SwitchTimelineByIndex(index));
			return true;
		}
		if !ctrl && !shift && !alt {
			let mode = autoload_mode_list.get();
			if mode == AutoloadMode::AtBoundary || mode == AutoloadMode::AtEnd {
				let sort_order = sort_order_list.get();
				let selection = timeline_list_key.get_selection();
				let count = timeline_list_key.get_count();
				if let Some(index) = selection {
					if k == keys::UP {
						if sort_order == SortOrder::OldestToNewest && index == 0 {
							let _ = ui_tx_list_key.send(UiCommand::LoadMore);
							return true;
						}
					} else if k == keys::DOWN
						&& mode == AutoloadMode::AtBoundary
						&& sort_order == SortOrder::NewestToOldest
						&& index + 1 == count
					{
						let _ = ui_tx_list_key.send(UiCommand::LoadMore);
						return true;
					}
				}
			}
		}
		if let Some(action) = shortcuts_list_key.borrow().find_action(quick_mode, k, ctrl, alt, shift) {
			dispatch_action(action, &find_frame, &ui_tx_list_key, &quick_action_keys_list);
			return true;
		}
		// Letters are commands in quick action mode, so the list doesn't find posts by them. It does
		// otherwise.
		quick_mode && !ctrl && !alt && (32..=126).contains(&k)
	});
	let shutdown_ctx = is_shutting_down.clone();
	let post_menu_state_ctx = post_menu_state;
	let timeline_list_ctx = parts.timeline_list.clone();
	let shortcuts_ctx = shortcuts_cell;
	parts.timeline_list.on_context_menu(move || {
		if shutdown_ctx.get() {
			return;
		}
		let mut menu = build_context_menu(post_menu_state_ctx.get(), &shortcuts_ctx.borrow());
		timeline_list_ctx.popup_menu(&mut menu);
	});
	let ui_tx_list = ui_tx.clone();
	let shutdown_list = is_shutting_down.clone();
	let suppress_list = suppress_selection;
	let timeline_list_sel = parts.timeline_list.clone();
	let autoload_mode_selection = autoload_mode;
	let sort_order_selection = sort_order_cell;
	parts.timeline_list.on_selection_changed(move || {
		if shutdown_list.get() {
			return;
		}
		if suppress_list.get() {
			return;
		}
		let Some(sel) = timeline_list_sel.get_selection() else { return };
		let Ok(selection) = usize::try_from(sel) else { return };
		let _ = ui_tx_list.send(UiCommand::TimelineEntrySelectionChanged(selection));
		if autoload_mode_selection.get() == AutoloadMode::AtEnd {
			let count = usize::try_from(timeline_list_sel.get_count()).unwrap_or(0);
			let sort_order = sort_order_selection.get();
			let is_oldest = sort_order == SortOrder::OldestToNewest && selection == 0;
			let is_newest = sort_order == SortOrder::NewestToOldest && selection + 1 == count;
			if is_oldest {
				let _ = ui_tx_list.send(UiCommand::LoadMoreBackground);
			} else if is_newest {
				let _ = ui_tx_list.send(UiCommand::LoadMore);
			}
		}
	});
	let ui_tx_menu = ui_tx;
	let shutdown_menu = is_shutting_down;
	let frame_menu = parts.frame;
	let quick_action_keys_menu = quick_action_keys_enabled.clone();
	frame_menu.on_menu_selected(move |event| {
		if shutdown_menu.get() {
			return;
		}
		if let Some(action) = action_for_id(event.get_id()) {
			dispatch_action(action, &frame_menu, &ui_tx_menu, &quick_action_keys_menu);
		}
	});
}
