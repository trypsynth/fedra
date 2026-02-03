use std::{cell::Cell, rc::Rc, sync::mpsc::Sender};

use wxdragon::prelude::*;

use crate::{
	ID_BOOKMARK, ID_BOOST, ID_CLOSE_TIMELINE, ID_COPY_POST, ID_DELETE_POST, ID_EDIT_POST, ID_EDIT_PROFILE, ID_FAVORITE,
	ID_FEDERATED_TIMELINE, ID_LOAD_MORE, ID_LOCAL_TIMELINE, ID_MANAGE_ACCOUNTS, ID_NEW_POST, ID_OPEN_LINKS,
	ID_OPEN_USER_TIMELINE_BY_INPUT, ID_OPTIONS, ID_REFRESH, ID_REPLY, ID_REPLY_AUTHOR, ID_SEARCH, ID_VIEW_BOOSTS,
	ID_VIEW_FAVORITES, ID_VIEW_HASHTAGS, ID_VIEW_HELP, ID_VIEW_IN_BROWSER, ID_VIEW_MENTIONS, ID_VIEW_PROFILE,
	ID_VIEW_THREAD, ID_VIEW_USER_TIMELINE,
	KEY_DELETE, UiCommand,
	config::{AutoloadMode, SortOrder},
	live_region,
	ui::menu::build_menu_bar,
};

pub struct WindowParts {
	pub frame: Frame,
	pub timelines_selector: ListBox,
	pub timeline_list: ListBox,
	pub live_region_label: StaticText,
}

pub fn build_main_window() -> WindowParts {
	let frame = Frame::builder().with_title("Fedra").with_size(Size::new(800, 600)).build();
	wxdragon::app::set_top_window(&frame);
	let menu_bar = build_menu_bar();
	frame.set_menu_bar(menu_bar);
	let panel = Panel::builder(&frame).build();
	// live region
	let live_region_label = StaticText::builder(&panel).with_size(Size::new(1, 1)).build();
	live_region_label.show(false);
	live_region::set_live_region(&live_region_label);

	let sizer = BoxSizer::builder(Orientation::Horizontal).build();
	let timelines_label = StaticText::builder(&panel).with_label("Timelines").build();
	let timelines_selector = ListBox::builder(&panel).with_choices(vec!["Home".to_string()]).build();
	timelines_selector.set_selection(0_u32, true);
	let timeline_list = ListBox::builder(&panel).build();
	let timelines_sizer = BoxSizer::builder(Orientation::Vertical).build();
	timelines_sizer.add(&timelines_label, 0, SizerFlag::All, 8);
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
	frame_sizer.add(&panel, 1, SizerFlag::Expand | SizerFlag::All, 0);
	frame.set_sizer(frame_sizer, true);

	WindowParts { frame, timelines_selector, timeline_list, live_region_label }
}

pub fn bind_input_handlers(
	parts: &WindowParts,
	ui_tx: Sender<UiCommand>,
	is_shutting_down: Rc<Cell<bool>>,
	suppress_selection: Rc<Cell<bool>>,
	quick_action_keys_enabled: Rc<Cell<bool>>,
	autoload_mode: Rc<Cell<AutoloadMode>>,
	sort_order_cell: Rc<Cell<SortOrder>>,
	timer: Rc<Timer<Frame>>,
) {
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
			&& index >= 0
		{
			let _ = ui_tx_selector.send(UiCommand::TimelineSelectionChanged(index as usize));
		}
	});

	let ui_tx_delete = ui_tx.clone();
	let shutdown_delete = is_shutting_down.clone();
	let timelines_selector_delete = parts.timelines_selector;
	timelines_selector_delete.on_key_down(move |event| {
		if shutdown_delete.get() {
			return;
		}
		if let WindowEventData::Keyboard(ref key_event) = event {
			if key_event.control_down() {
				match key_event.get_key_code() {
					Some(91) => {
						let _ = ui_tx_delete.send(UiCommand::SwitchPrevAccount);
						event.skip(false);
						return;
					}
					Some(93) => {
						let _ = ui_tx_delete.send(UiCommand::SwitchNextAccount);
						event.skip(false);
						return;
					}
					_ => {}
				}
			}
			if key_event.get_key_code() == Some(KEY_DELETE) {
				let _ = ui_tx_delete.send(UiCommand::CloseTimeline);
				event.skip(false);
			} else {
				event.skip(true);
			}
		} else {
			event.skip(true);
		}
	});

	let ui_tx_list = ui_tx.clone();
	let shutdown_list = is_shutting_down.clone();
	let suppress_list = suppress_selection.clone();
	let timeline_list_state = parts.timeline_list;
	let ui_tx_list_key = ui_tx.clone();
	let shutdown_list_key = is_shutting_down.clone();
	let quick_action_keys_list = quick_action_keys_enabled.clone();
	let autoload_mode_list = autoload_mode.clone();
	let sort_order_list = sort_order_cell.clone();
	timeline_list_state.bind_internal(EventType::KEY_DOWN, move |event| {
		if shutdown_list_key.get() {
			return;
		}
		if let Some(key) = event.get_key_code() {
			if key == 13 {
				// Enter
				if event.shift_down() {
					let _ = ui_tx_list_key.send(UiCommand::OpenLinks);
					event.skip(false);
					return;
				}
				if !event.control_down() && !event.alt_down() {
					let _ = ui_tx_list_key.send(UiCommand::ViewThread);
					event.skip(false);
					return;
				}
			}
			// Navigation keys (always active)
			if !event.control_down() && !event.shift_down() && !event.alt_down() {
				match key {
					8 => {
						// Backspace
						let _ = ui_tx_list_key.send(UiCommand::GoBack);
						event.skip(false);
						return;
					}
					314 => {
						// Left Arrow
						let _ = ui_tx_list_key.send(UiCommand::SwitchPrevTimeline);
						event.skip(false);
						return;
					}
					316 => {
						// Right Arrow
						let _ = ui_tx_list_key.send(UiCommand::SwitchNextTimeline);
						event.skip(false);
						return;
					}
					_ => {}
				}

				if autoload_mode_list.get() == AutoloadMode::AtBoundary {
					let sort_order = sort_order_list.get();
					let selection = timeline_list_state.get_selection().map(|s| s as usize);
					let count = timeline_list_state.get_count() as usize;

					if let Some(index) = selection {
						if key == 315 {
							// Up
							if sort_order == SortOrder::OldestToNewest && index == 0 {
								let _ = ui_tx_list_key.send(UiCommand::LoadMore);
							}
						} else if key == 317 {
							// Down
							if sort_order == SortOrder::NewestToOldest && index + 1 == count {
								let _ = ui_tx_list_key.send(UiCommand::LoadMore);
							}
						}
					}
				}
			}

			if event.control_down() && event.shift_down() && key == 81 {
				// Ctrl+Shift+Q
				let _ = ui_tx_list_key.send(UiCommand::SetQuickActionKeysEnabled(true));
				event.skip(false);
				return;
			}

			if event.control_down() {
				if event.shift_down() && key == 75 {
					// Ctrl+Shift+K
					let _ = ui_tx_list_key.send(UiCommand::Bookmark);
					event.skip(false);
					return;
				}
				match key {
					87 => {
						// w
						let _ = ui_tx_list_key.send(UiCommand::CloseTimeline);
						event.skip(false);
						return;
					}
					8 => {
						// Backspace
						let _ = ui_tx_list_key.send(UiCommand::CloseAndNavigateBack);
						event.skip(false);
						return;
					}
					k if (49..=57).contains(&k) => {
						// 1-9
						let _ = ui_tx_list_key.send(UiCommand::SwitchTimelineByIndex((k - 49) as usize));
						event.skip(false);
						return;
					}
					88 => {
						// x
						let _ = ui_tx_list_key.send(UiCommand::ToggleContentWarning);
						event.skip(false);
						return;
					}
					69 => {
						// e
						let _ = ui_tx_list_key.send(UiCommand::EditPost);
						event.skip(false);
						return;
					}
					91 => {
						// [
						let _ = ui_tx_list_key.send(UiCommand::SwitchPrevAccount);
						event.skip(false);
						return;
					}
					93 => {
						// ]
						let _ = ui_tx_list_key.send(UiCommand::SwitchNextAccount);
						event.skip(false);
						return;
					}
					85 => {
						// u
						let _ = ui_tx_list_key.send(UiCommand::OpenUserTimelineByInput);
						event.skip(false);
						return;
					}
					46 => {
						// .
						let _ = ui_tx_list_key.send(UiCommand::LoadMore);
						event.skip(false);
						return;
					}
					KEY_DELETE => {
						// Delete
						let _ = ui_tx_list_key.send(UiCommand::DeletePost);
						event.skip(false);
						return;
					}
					_ => {}
				}
			}

			if quick_action_keys_list.get() && !event.control_down() && !event.shift_down() && !event.alt_down() {
				match key {
					81 => {
						// q
						let _ = ui_tx_list_key.send(UiCommand::SetQuickActionKeysEnabled(false));
						event.skip(false);
						return;
					}
					70 => {
						// f
						let _ = ui_tx_list_key.send(UiCommand::Favorite);
						event.skip(false);
						return;
					}
					75 => {
						// k
						let _ = ui_tx_list_key.send(UiCommand::Bookmark);
						event.skip(false);
						return;
					}
					66 => {
						// b
						let _ = ui_tx_list_key.send(UiCommand::Boost);
						event.skip(false);
						return;
					}
					67 => {
						// c
						let _ = ui_tx_list_key.send(UiCommand::NewPost);
						event.skip(false);
						return;
					}
					68 => {
						// d
						let _ = ui_tx_list_key.send(UiCommand::DeletePost);
						event.skip(false);
						return;
					}
					69 => {
						// e
						let _ = ui_tx_list_key.send(UiCommand::EditPost);
						event.skip(false);
						return;
					}
					82 => {
						// r
						let _ = ui_tx_list_key.send(UiCommand::Reply { reply_all: true });
						event.skip(false);
						return;
					}
					84 => {
						// t
						let _ = ui_tx_list_key.send(UiCommand::OpenUserTimeline);
						event.skip(false);
						return;
					}
					77 => {
						// m
						let _ = ui_tx_list_key.send(UiCommand::ViewMentions);
						event.skip(false);
						return;
					}
					80 => {
						// p
						let _ = ui_tx_list_key.send(UiCommand::ViewProfile);
						event.skip(false);
						return;
					}
					72 => {
						// h
						let _ = ui_tx_list_key.send(UiCommand::ViewHashtags);
						event.skip(false);
						return;
					}
					79 => {
						// o
						let _ = ui_tx_list_key.send(UiCommand::ViewInBrowser);
						event.skip(false);
						return;
					}
					88 => {
						// x
						let _ = ui_tx_list_key.send(UiCommand::ToggleContentWarning);
						event.skip(false);
						return;
					}
					46 => {
						// .
						let _ = ui_tx_list_key.send(UiCommand::LoadMore);
						event.skip(false);
						return;
					}
					47 | 191 => {
						// / (forward slash - key code may vary by platform)
						let _ = ui_tx_list_key.send(UiCommand::Search);
						event.skip(false);
						return;
					}
					k if (49..=57).contains(&k) => {
						// 1-9
						let _ = ui_tx_list_key.send(UiCommand::SwitchTimelineByIndex((k - 49) as usize));
						event.skip(false);
						return;
					}
					_ => {}
				}
			}
		}
		event.skip(true);
	});

	let autoload_mode_selection = autoload_mode.clone();
	let sort_order_selection = sort_order_cell.clone();
	timeline_list_state.on_selection_changed(move |event| {
		if shutdown_list.get() {
			return;
		}
		if suppress_list.get() {
			return;
		}
		if let Some(selection) = event.get_selection()
			&& selection >= 0
		{
			let _ = ui_tx_list.send(UiCommand::TimelineEntrySelectionChanged(selection as usize));
			if autoload_mode_selection.get() == AutoloadMode::AtEnd {
				let count = timeline_list_state.get_count() as usize;
				let index = selection as usize;
				let sort_order = sort_order_selection.get();
				let at_load_position = match sort_order {
					SortOrder::NewestToOldest => index + 1 == count,
					SortOrder::OldestToNewest => index == 0,
				};
				if at_load_position {
					let _ = ui_tx_list.send(UiCommand::LoadMore);
				}
			}
		}
	});
	let ui_tx_menu = ui_tx.clone();
	let shutdown_menu = is_shutting_down.clone();
	let frame_menu = parts.frame;
	frame_menu.on_key_down(move |event| {
		if shutdown_menu.get() {
			return;
		}
		if let WindowEventData::Keyboard(ref key_event) = event {
			if key_event.control_down() {
				match key_event.get_key_code() {
					Some(91) => {
						let _ = ui_tx_menu.send(UiCommand::SwitchPrevAccount);
					}
					Some(93) => {
						let _ = ui_tx_menu.send(UiCommand::SwitchNextAccount);
					}
					_ => event.skip(true),
				}
			} else {
				event.skip(true);
			}
		} else {
			event.skip(true);
		}
	});

	let ui_tx_menu = ui_tx.clone();
	let shutdown_menu = is_shutting_down.clone();
	let frame_menu = parts.frame;
	frame_menu.on_menu_selected(move |event| match event.get_id() {
		ID_VIEW_PROFILE => {
			if shutdown_menu.get() {
				return;
			}
			let _ = ui_tx_menu.send(UiCommand::ViewProfile);
		}
		ID_OPTIONS => {
			if shutdown_menu.get() {
				return;
			}
			let _ = ui_tx_menu.send(UiCommand::ShowOptions);
		}
		ID_MANAGE_ACCOUNTS => {
			if shutdown_menu.get() {
				return;
			}
			let _ = ui_tx_menu.send(UiCommand::ManageAccounts);
		}
		ID_EDIT_PROFILE => {
			if shutdown_menu.get() {
				return;
			}
			let _ = ui_tx_menu.send(UiCommand::EditProfile);
		}
		ID_NEW_POST => {
			if shutdown_menu.get() {
				return;
			}
			let _ = ui_tx_menu.send(UiCommand::NewPost);
		}
		ID_REPLY => {
			if shutdown_menu.get() {
				return;
			}
			let _ = ui_tx_menu.send(UiCommand::Reply { reply_all: true });
		}
		ID_REPLY_AUTHOR => {
			if shutdown_menu.get() {
				return;
			}
			let _ = ui_tx_menu.send(UiCommand::Reply { reply_all: false });
		}
		ID_FAVORITE => {
			if shutdown_menu.get() {
				return;
			}
			let _ = ui_tx_menu.send(UiCommand::Favorite);
		}
		ID_BOOKMARK => {
			if shutdown_menu.get() {
				return;
			}
			let _ = ui_tx_menu.send(UiCommand::Bookmark);
		}
		ID_BOOST => {
			if shutdown_menu.get() {
				return;
			}
			let _ = ui_tx_menu.send(UiCommand::Boost);
		}
		ID_DELETE_POST => {
			if shutdown_menu.get() {
				return;
			}
			let _ = ui_tx_menu.send(UiCommand::DeletePost);
		}
		ID_EDIT_POST => {
			if shutdown_menu.get() {
				return;
			}
			let _ = ui_tx_menu.send(UiCommand::EditPost);
		}
		ID_REFRESH => {
			if shutdown_menu.get() {
				return;
			}
			let _ = ui_tx_menu.send(UiCommand::Refresh);
		}
		ID_VIEW_USER_TIMELINE => {
			if shutdown_menu.get() {
				return;
			}
			let _ = ui_tx_menu.send(UiCommand::OpenUserTimeline);
		}
		ID_OPEN_USER_TIMELINE_BY_INPUT => {
			if shutdown_menu.get() {
				return;
			}
			let _ = ui_tx_menu.send(UiCommand::OpenUserTimelineByInput);
		}
		ID_LOCAL_TIMELINE => {
			if shutdown_menu.get() {
				return;
			}
			let _ = ui_tx_menu.send(UiCommand::OpenTimeline(crate::timeline::TimelineType::Local));
		}
		ID_FEDERATED_TIMELINE => {
			if shutdown_menu.get() {
				return;
			}
			let _ = ui_tx_menu.send(UiCommand::OpenTimeline(crate::timeline::TimelineType::Federated));
		}
		crate::ID_BOOKMARKS_TIMELINE => {
			if shutdown_menu.get() {
				return;
			}
			let _ = ui_tx_menu.send(UiCommand::OpenTimeline(crate::timeline::TimelineType::Bookmarks));
		}
		crate::ID_FAVORITES_TIMELINE => {
			if shutdown_menu.get() {
				return;
			}
			let _ = ui_tx_menu.send(UiCommand::OpenTimeline(crate::timeline::TimelineType::Favorites));
		}
		ID_CLOSE_TIMELINE => {
			if shutdown_menu.get() {
				return;
			}
			let _ = ui_tx_menu.send(UiCommand::CloseTimeline);
		}
		ID_VIEW_MENTIONS => {
			if shutdown_menu.get() {
				return;
			}
			let _ = ui_tx_menu.send(UiCommand::ViewMentions);
		}
		ID_VIEW_HASHTAGS => {
			if shutdown_menu.get() {
				return;
			}
			let _ = ui_tx_menu.send(UiCommand::ViewHashtags);
		}
		ID_OPEN_LINKS => {
			if shutdown_menu.get() {
				return;
			}
			let _ = ui_tx_menu.send(UiCommand::OpenLinks);
		}
		ID_VIEW_IN_BROWSER => {
			if shutdown_menu.get() {
				return;
			}
			let _ = ui_tx_menu.send(UiCommand::ViewInBrowser);
		}
		ID_VIEW_BOOSTS => {
			if shutdown_menu.get() {
				return;
			}
			let _ = ui_tx_menu.send(UiCommand::ViewBoosts);
		}
		ID_VIEW_FAVORITES => {
			if shutdown_menu.get() {
				return;
			}
			let _ = ui_tx_menu.send(UiCommand::ViewFavorites);
		}
		ID_COPY_POST => {
			if shutdown_menu.get() {
				return;
			}
			let _ = ui_tx_menu.send(UiCommand::CopyPost);
		}
		ID_VIEW_THREAD => {
			if shutdown_menu.get() {
				return;
			}
			let _ = ui_tx_menu.send(UiCommand::ViewThread);
		}
		ID_LOAD_MORE => {
			if shutdown_menu.get() {
				return;
			}
			let _ = ui_tx_menu.send(UiCommand::LoadMore);
		}
		crate::ID_VOTE => {
			if shutdown_menu.get() {
				return;
			}
			let _ = ui_tx_menu.send(UiCommand::Vote);
		}
		ID_VIEW_HELP => {
			if shutdown_menu.get() {
				return;
			}
			let _ = ui_tx_menu.send(UiCommand::ViewHelp);
		}
		ID_SEARCH => {
			if shutdown_menu.get() {
				return;
			}
			let _ = ui_tx_menu.send(UiCommand::Search);
		}
		_ => {}
	});

	let shutdown_close = is_shutting_down.clone();
	let timer_close = timer.clone();
	let frame_close = parts.frame;
	frame_close.on_close(move |event| {
		if !shutdown_close.get() {
			shutdown_close.set(true);
			timer_close.stop();
		}
		event.skip(true);
	});
}
