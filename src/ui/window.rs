use std::{
	cell::{Cell, RefCell},
	rc::Rc,
};

use wxdragon::prelude::*;

use crate::{
	ContextMenuState, ID_BOOKMARK, ID_BOOKMARKS_TIMELINE, ID_BOOST, ID_CHECK_FOR_UPDATES, ID_CLOSE_TIMELINE,
	ID_COPY_POST, ID_COPY_POST_LINK, ID_CUSTOMIZE_SHORTCUTS, ID_DELETE_POST, ID_DIRECT_TIMELINE, ID_EDIT_POST,
	ID_EDIT_PROFILE, ID_FAVORITE, ID_FAVORITES_TIMELINE, ID_FEDERATED_TIMELINE, ID_FIND, ID_FIND_NEXT, ID_FIND_PREV,
	ID_HOME_TIMELINE, ID_LOAD_MORE, ID_LOCAL_TIMELINE, ID_MANAGE_ACCOUNTS, ID_MANAGE_FILTERS, ID_MANAGE_LISTS,
	ID_MENTIONS_TIMELINE, ID_NEW_POST, ID_NOTIFICATIONS_TIMELINE, ID_OPEN_INSTANCE_TIMELINE_BY_INPUT, ID_OPEN_LINKS,
	ID_OPEN_LIST, ID_OPEN_USER_TIMELINE_BY_INPUT, ID_OPTIONS, ID_PIN_POST, ID_PLAY_MEDIA, ID_QUOTE, ID_REFRESH,
	ID_REPLY, ID_REPLY_AUTHOR, ID_SEARCH, ID_SENT_TIMELINE, ID_TOGGLE_FOLLOW, ID_VIEW_BOOSTS, ID_VIEW_FAVORITES,
	ID_VIEW_HASHTAGS, ID_VIEW_HELP, ID_VIEW_IN_BROWSER, ID_VIEW_MENTIONS, ID_VIEW_POST, ID_VIEW_PROFILE,
	ID_VIEW_QUOTED_THREAD, ID_VIEW_THREAD, ID_VIEW_USER_TIMELINE, ID_VOTE, UiCommand,
	config::{ActionId, AutoloadMode, ShortcutsConfig, SortOrder},
	ui::{dialogs, menu::build_menu_bar},
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
	let timelines_label = StaticText::builder(&panel).with_label("Timelines").build();
	let timelines_selector = ListBox::builder(&panel).with_choices(vec!["Home".to_string()]).build();
	timelines_selector.set_selection(0_u32, true);
	let timeline_list = crate::ui::timeline_list::TimelineList::new(&panel);
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
	context_menu_state: Rc<Cell<ContextMenuState>>,
	shortcuts_cell: Rc<RefCell<ShortcutsConfig>>,
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
			&& let Ok(index) = usize::try_from(index)
		{
			let _ = ui_tx_selector.send(UiCommand::TimelineSelectionChanged(index));
		}
	});

	let ui_tx_delete = ui_tx.clone();
	let shutdown_delete = is_shutting_down.clone();
	let quick_action_keys_selector = quick_action_keys_enabled.clone();
	let timelines_selector_delete = parts.timelines_selector;
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
				if ctrl && (49..=57).contains(&k) {
					if let Ok(index) = usize::try_from(k - 49) {
						let _ = ui_tx_delete.send(UiCommand::SwitchTimelineByIndex(index));
					}
					event.skip(false);
					return;
				}
				if quick_mode && !ctrl && !shift && !alt && (49..=57).contains(&k) {
					if let Ok(index) = usize::try_from(k - 49) {
						let _ = ui_tx_delete.send(UiCommand::SwitchTimelineByIndex(index));
					}
					event.skip(false);
					return;
				}
				if let Some(action) = shortcuts_selector.borrow().find_action(quick_mode, k, ctrl, alt, shift) {
					match action {
						ActionId::SwitchPrevAccount => {
							let _ = ui_tx_delete.send(UiCommand::SwitchPrevAccount);
							event.skip(false);
							return;
						}
						ActionId::SwitchNextAccount => {
							let _ = ui_tx_delete.send(UiCommand::SwitchNextAccount);
							event.skip(false);
							return;
						}
						ActionId::MoveTimelineLeft => {
							let _ = ui_tx_delete.send(UiCommand::MoveTimelineLeft);
							event.skip(false);
							return;
						}
						ActionId::MoveTimelineRight => {
							let _ = ui_tx_delete.send(UiCommand::MoveTimelineRight);
							event.skip(false);
							return;
						}
						ActionId::CloseTimeline => {
							let _ = ui_tx_delete.send(UiCommand::CloseTimeline);
							event.skip(false);
							return;
						}
						ActionId::SwitchPrevTimeline => {
							let _ = ui_tx_delete.send(UiCommand::SwitchPrevTimeline);
							event.skip(false);
							return;
						}
						ActionId::SwitchNextTimeline => {
							let _ = ui_tx_delete.send(UiCommand::SwitchNextTimeline);
							event.skip(false);
							return;
						}
						_ => {}
					}
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
		if let WindowEventData::Keyboard(key_event) = event {
			if shutdown_list_key.get() {
				event.skip(true);
				return;
			}
			let ctrl = key_event.control_down();
			let shift = key_event.shift_down();
			let alt = key_event.alt_down();
			let Some(k) = key_event.get_key_code() else {
				event.skip(true);
				return;
			};

			let quick_mode = quick_action_keys_list.get();

			if ctrl && (49..=57).contains(&k) {
				if let Ok(index) = usize::try_from(k - 49) {
					let _ = ui_tx_list_key.send(UiCommand::SwitchTimelineByIndex(index));
				}
				event.skip(false);
				return;
			}

			if quick_mode && !ctrl && !shift && !alt && (49..=57).contains(&k) {
				if let Ok(index) = usize::try_from(k - 49) {
					let _ = ui_tx_list_key.send(UiCommand::SwitchTimelineByIndex(index));
				}
				event.skip(false);
				return;
			}

			if !ctrl && !shift && !alt {
				let mode = autoload_mode_list.get();
				if mode == AutoloadMode::AtBoundary || mode == AutoloadMode::AtEnd {
					let sort_order = sort_order_list.get();
					let selection = timeline_list_key.get_selection();
					let count = timeline_list_key.get_count();
					if let Some(index) = selection {
						if k == 315 {
							if sort_order == SortOrder::OldestToNewest && index == 0 {
								let _ = ui_tx_list_key.send(UiCommand::LoadMore);
								event.skip(false);
								return;
							}
						} else if k == 317 {
							if mode == AutoloadMode::AtBoundary
								&& sort_order == SortOrder::NewestToOldest
								&& index + 1 == count
							{
								let _ = ui_tx_list_key.send(UiCommand::LoadMore);
								event.skip(false);
								return;
							}
						}
					}
				}
				if k == 313 && sort_order_list.get() == SortOrder::OldestToNewest {
					let _ = ui_tx_list_key.send(UiCommand::HomePressed);
					event.skip(false);
					return;
				}
			}

			if let Some(action) = shortcuts_list_key.borrow().find_action(quick_mode, k, ctrl, alt, shift) {
				match action {
					ActionId::NewPost => {
						let _ = ui_tx_list_key.send(UiCommand::NewPost);
					}
					ActionId::Reply => {
						let _ = ui_tx_list_key.send(UiCommand::Reply { reply_all: true });
					}
					ActionId::ReplyAuthor => {
						let _ = ui_tx_list_key.send(UiCommand::Reply { reply_all: false });
					}
					ActionId::Quote => {
						let _ = ui_tx_list_key.send(UiCommand::Quote);
					}
					ActionId::ToggleFollow => {
						let _ = ui_tx_list_key.send(UiCommand::ToggleFollow);
					}
					ActionId::ViewProfile => {
						let _ = ui_tx_list_key.send(UiCommand::ViewProfile);
					}
					ActionId::ViewMentions => {
						let _ = ui_tx_list_key.send(UiCommand::ViewMentions);
					}
					ActionId::ViewHashtags => {
						let _ = ui_tx_list_key.send(UiCommand::ViewHashtags);
					}
					ActionId::OpenLinks => {
						let _ = ui_tx_list_key.send(UiCommand::OpenLinks);
					}
					ActionId::PlayMedia => {
						let _ = ui_tx_list_key.send(UiCommand::PlayMedia);
					}
					ActionId::ViewInBrowser => {
						let _ = ui_tx_list_key.send(UiCommand::ViewInBrowser);
					}
					ActionId::CopyPost => {
						let _ = ui_tx_list_key.send(UiCommand::CopyPost);
					}
					ActionId::CopyPostLink => {
						let _ = ui_tx_list_key.send(UiCommand::CopyPostLink);
					}
					ActionId::ViewPost => {
						let _ = ui_tx_list_key.send(UiCommand::ViewPost);
					}
					ActionId::ViewThread => {
						let _ = ui_tx_list_key.send(UiCommand::ViewThread);
					}
					ActionId::ViewQuotedThread => {
						let _ = ui_tx_list_key.send(UiCommand::ViewQuotedThread);
					}
					ActionId::EditPost => {
						let _ = ui_tx_list_key.send(UiCommand::EditPost);
					}
					ActionId::DeletePost => {
						let _ = ui_tx_list_key.send(UiCommand::DeletePost);
					}
					ActionId::PinPost => {
						let _ = ui_tx_list_key.send(UiCommand::Pin);
					}
					ActionId::Vote => {
						let _ = ui_tx_list_key.send(UiCommand::Vote);
					}
					ActionId::Favorite => {
						let _ = ui_tx_list_key.send(UiCommand::Favorite);
					}
					ActionId::Bookmark => {
						let _ = ui_tx_list_key.send(UiCommand::Bookmark);
					}
					ActionId::Boost => {
						let _ = ui_tx_list_key.send(UiCommand::Boost);
					}
					ActionId::ViewBoosts => {
						let _ = ui_tx_list_key.send(UiCommand::ViewBoosts);
					}
					ActionId::ViewFavorites => {
						let _ = ui_tx_list_key.send(UiCommand::ViewFavorites);
					}
					ActionId::OpenUserTimeline => {
						let _ = ui_tx_list_key.send(UiCommand::OpenUserTimeline);
					}
					ActionId::OpenUserTimelineByInput => {
						let _ = ui_tx_list_key.send(UiCommand::OpenUserTimelineByInput);
					}
					ActionId::Search => {
						let _ = ui_tx_list_key.send(UiCommand::Search);
					}
					ActionId::Find => {
						if let Some(query) = dialogs::show_find_dialog(&find_frame) {
							let _ = ui_tx_list_key.send(UiCommand::Find(query));
						}
					}
					ActionId::FindNext => {
						let _ = ui_tx_list_key.send(UiCommand::FindNext);
					}
					ActionId::FindPrev => {
						let _ = ui_tx_list_key.send(UiCommand::FindPrev);
					}
					ActionId::HomeTimeline => {
						let _ = ui_tx_list_key.send(UiCommand::OpenTimeline(crate::timeline::TimelineType::Home));
					}
					ActionId::NotificationsTimeline => {
						let _ =
							ui_tx_list_key.send(UiCommand::OpenTimeline(crate::timeline::TimelineType::Notifications));
					}
					ActionId::SentTimeline => {
						let _ = ui_tx_list_key.send(UiCommand::SentTimeline);
					}
					ActionId::LocalTimeline => {
						let _ = ui_tx_list_key.send(UiCommand::OpenTimeline(crate::timeline::TimelineType::Local));
					}
					ActionId::OpenInstanceTimelineByInput => {
						let _ = ui_tx_list_key.send(UiCommand::OpenInstanceTimelineByInput);
					}
					ActionId::FederatedTimeline => {
						let _ = ui_tx_list_key.send(UiCommand::OpenTimeline(crate::timeline::TimelineType::Federated));
					}
					ActionId::DirectTimeline => {
						let _ = ui_tx_list_key.send(UiCommand::OpenTimeline(crate::timeline::TimelineType::Direct));
					}
					ActionId::MentionsTimeline => {
						let _ = ui_tx_list_key.send(UiCommand::OpenTimeline(crate::timeline::TimelineType::Mentions));
					}
					ActionId::BookmarksTimeline => {
						let _ = ui_tx_list_key.send(UiCommand::OpenTimeline(crate::timeline::TimelineType::Bookmarks));
					}
					ActionId::FavoritesTimeline => {
						let _ = ui_tx_list_key.send(UiCommand::OpenTimeline(crate::timeline::TimelineType::Favorites));
					}
					ActionId::OpenList => {
						let _ = ui_tx_list_key.send(UiCommand::OpenList);
					}
					ActionId::LoadMore => {
						let _ = ui_tx_list_key.send(UiCommand::LoadMore);
					}
					ActionId::CloseTimeline => {
						let _ = ui_tx_list_key.send(UiCommand::CloseTimeline);
					}
					ActionId::Refresh => {
						let _ = ui_tx_list_key.send(UiCommand::Refresh);
					}
					ActionId::SwitchPrevTimeline => {
						let _ = ui_tx_list_key.send(UiCommand::SwitchPrevTimeline);
					}
					ActionId::SwitchNextTimeline => {
						let _ = ui_tx_list_key.send(UiCommand::SwitchNextTimeline);
					}
					ActionId::MoveTimelineLeft => {
						let _ = ui_tx_list_key.send(UiCommand::MoveTimelineLeft);
					}
					ActionId::MoveTimelineRight => {
						let _ = ui_tx_list_key.send(UiCommand::MoveTimelineRight);
					}
					ActionId::SwitchPrevAccount => {
						let _ = ui_tx_list_key.send(UiCommand::SwitchPrevAccount);
					}
					ActionId::SwitchNextAccount => {
						let _ = ui_tx_list_key.send(UiCommand::SwitchNextAccount);
					}
					ActionId::ToggleContentWarning => {
						let _ = ui_tx_list_key.send(UiCommand::ToggleContentWarning);
					}
					ActionId::ToggleQuickActionKeys => {
						let new_value = !quick_action_keys_list.get();
						quick_action_keys_list.set(new_value);
						let _ = ui_tx_list_key.send(UiCommand::SetQuickActionKeysEnabled(new_value));
					}
					ActionId::ManageAccounts => {
						let _ = ui_tx_list_key.send(UiCommand::ManageAccounts);
					}
					ActionId::ManageFilters => {
						let _ = ui_tx_list_key.send(UiCommand::ManageFilters);
					}
					ActionId::ManageLists => {
						let _ = ui_tx_list_key.send(UiCommand::ManageLists);
					}
					ActionId::EditProfile => {
						let _ = ui_tx_list_key.send(UiCommand::EditProfile);
					}
					ActionId::Options => {
						let _ = ui_tx_list_key.send(UiCommand::ShowOptions);
					}
					ActionId::CustomizeShortcuts => {
						let _ = ui_tx_list_key.send(UiCommand::CustomizeShortcuts);
					}
					ActionId::CheckForUpdates => {
						let _ = ui_tx_list_key.send(UiCommand::CheckForUpdates);
					}
					ActionId::ViewHelp => {
						let _ = ui_tx_list_key.send(UiCommand::ViewHelp);
					}
				}
				event.skip(false);
				return;
			}

			if !quick_mode && !ctrl && !shift && !alt && k >= 32 && k <= 126 {
				if let Some(ch) = char::from_u32(k as u32) {
					if ch.is_alphanumeric() {
						timeline_list_key.type_ahead(ch);
						event.skip(false);
						return;
					}
				}
			}

			event.skip(true);
		} else {
			event.skip(true);
		}
	});

	let shutdown_ctx = is_shutting_down.clone();
	let context_menu_state_ctx = context_menu_state;
	let timeline_list_ctx = parts.timeline_list.clone();
	let shortcuts_ctx = shortcuts_cell;
	let _ = parts.timeline_list.bind_internal(EventType::CONTEXT_MENU, move |_event| {
		if shutdown_ctx.get() {
			return;
		}
		let cms = context_menu_state_ctx.get();
		let q = cms.quick_action_keys;
		let sc = shortcuts_ctx.borrow();
		let mut menu = Menu::builder().build();

		let append_item = |menu: &mut Menu, id: i32, base: &str, action: ActionId, help: &str| {
			let shortcut = sc.get_menu_str(q, action);
			let label = if shortcut.is_empty() { base.to_string() } else { format!("{base}\t{shortcut}") };
			menu.append(id, &label, help, ItemKind::Normal);
		};

		append_item(&mut menu, ID_REPLY, "&Reply...", ActionId::Reply, "Reply to all mentioned users");
		append_item(&mut menu, ID_REPLY_AUTHOR, "Reply to &Author...", ActionId::ReplyAuthor, "Reply to author only");
		append_item(&mut menu, ID_QUOTE, "&Quote...", ActionId::Quote, "Quote this post");
		menu.append_separator();

		let fav_base = if cms.favourited { "Un&favorite" } else { "&Favorite" };
		append_item(&mut menu, ID_FAVORITE, fav_base, ActionId::Favorite, "Favorite or unfavorite selected post");

		let bookmark_base = if cms.bookmarked { "Un&bookmark" } else { "&Bookmark" };
		append_item(&mut menu, ID_BOOKMARK, bookmark_base, ActionId::Bookmark, "Bookmark or unbookmark selected post");

		if !cms.is_direct {
			let boost_base = if cms.reblogged { "Un&boost" } else { "&Boost" };
			append_item(&mut menu, ID_BOOST, boost_base, ActionId::Boost, "Boost or unboost selected post");
		}
		menu.append_separator();
		append_item(&mut menu, ID_VIEW_POST, "View &Post Details", ActionId::ViewPost, "View post content in a dialog");
		append_item(&mut menu, ID_VIEW_THREAD, "View &Thread", ActionId::ViewThread, "View conversation thread");
		append_item(
			&mut menu,
			ID_VIEW_QUOTED_THREAD,
			"View &Quoted Thread",
			ActionId::ViewQuotedThread,
			"View conversation thread for quoted post",
		);
		append_item(&mut menu, ID_OPEN_LINKS, "Open &Links", ActionId::OpenLinks, "Open links in selected post");
		append_item(
			&mut menu,
			ID_PLAY_MEDIA,
			"Play &Media",
			ActionId::PlayMedia,
			"Play media attached to selected post",
		);
		append_item(
			&mut menu,
			ID_VIEW_IN_BROWSER,
			"&Open in Browser",
			ActionId::ViewInBrowser,
			"Open selected post in web browser",
		);
		append_item(&mut menu, ID_COPY_POST, "&Copy Post", ActionId::CopyPost, "Copy selected post text");
		append_item(&mut menu, ID_COPY_POST_LINK, "Copy Post &Link", ActionId::CopyPostLink, "Copy selected post URL");
		menu.append_separator();
		append_item(
			&mut menu,
			ID_VIEW_PROFILE,
			"View &Profile",
			ActionId::ViewProfile,
			"View profile of selected post's author",
		);
		append_item(
			&mut menu,
			ID_VIEW_USER_TIMELINE,
			"&User Timeline",
			ActionId::OpenUserTimeline,
			"Open timeline of selected post's author",
		);
		append_item(
			&mut menu,
			ID_VIEW_MENTIONS,
			"View &Mentions",
			ActionId::ViewMentions,
			"View mentions in selected post",
		);
		append_item(
			&mut menu,
			ID_VIEW_HASHTAGS,
			"View &Hashtags",
			ActionId::ViewHashtags,
			"View hashtags in selected post",
		);

		if cms.is_own {
			menu.append_separator();
			append_item(&mut menu, ID_EDIT_POST, "&Edit Post...", ActionId::EditPost, "Edit selected post");
			append_item(&mut menu, ID_DELETE_POST, "&Delete Post", ActionId::DeletePost, "Delete selected post");
			let pin_base = if cms.pinned { "&Unpin Post" } else { "&Pin Post" };
			append_item(&mut menu, ID_PIN_POST, pin_base, ActionId::PinPost, "Pin or unpin this post on your profile");
		}
		let _ = timeline_list_ctx.popup_menu(&mut menu, None);
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
			let count = timeline_list_sel.get_count() as usize;
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

	let ui_tx_menu = ui_tx;
	let shutdown_menu = is_shutting_down;
	let frame_menu = parts.frame;
	frame_menu.on_menu_selected(move |event| match event.get_id() {
		ID_TOGGLE_FOLLOW => {
			if shutdown_menu.get() {
				return;
			}
			let _ = ui_tx_menu.send(UiCommand::ToggleFollow);
		}
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
		ID_CUSTOMIZE_SHORTCUTS => {
			if shutdown_menu.get() {
				return;
			}
			let _ = ui_tx_menu.send(UiCommand::CustomizeShortcuts);
		}
		ID_MANAGE_ACCOUNTS => {
			if shutdown_menu.get() {
				return;
			}
			let _ = ui_tx_menu.send(UiCommand::ManageAccounts);
		}
		ID_MANAGE_FILTERS => {
			if shutdown_menu.get() {
				return;
			}
			let _ = ui_tx_menu.send(UiCommand::ManageFilters);
		}
		ID_MANAGE_LISTS => {
			if shutdown_menu.get() {
				return;
			}
			let _ = ui_tx_menu.send(UiCommand::ManageLists);
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
		ID_QUOTE => {
			if shutdown_menu.get() {
				return;
			}
			let _ = ui_tx_menu.send(UiCommand::Quote);
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
		ID_PIN_POST => {
			if shutdown_menu.get() {
				return;
			}
			let _ = ui_tx_menu.send(UiCommand::Pin);
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
		ID_OPEN_INSTANCE_TIMELINE_BY_INPUT => {
			if shutdown_menu.get() {
				return;
			}
			let _ = ui_tx_menu.send(UiCommand::OpenInstanceTimelineByInput);
		}
		ID_HOME_TIMELINE => {
			if shutdown_menu.get() {
				return;
			}
			let _ = ui_tx_menu.send(UiCommand::OpenTimeline(crate::timeline::TimelineType::Home));
		}
		ID_NOTIFICATIONS_TIMELINE => {
			if shutdown_menu.get() {
				return;
			}
			let _ = ui_tx_menu.send(UiCommand::OpenTimeline(crate::timeline::TimelineType::Notifications));
		}
		ID_SENT_TIMELINE => {
			if shutdown_menu.get() {
				return;
			}
			let _ = ui_tx_menu.send(UiCommand::SentTimeline);
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
		ID_DIRECT_TIMELINE => {
			if shutdown_menu.get() {
				return;
			}
			let _ = ui_tx_menu.send(UiCommand::OpenTimeline(crate::timeline::TimelineType::Direct));
		}
		ID_OPEN_LIST => {
			if shutdown_menu.get() {
				return;
			}
			let _ = ui_tx_menu.send(UiCommand::OpenList);
		}
		ID_MENTIONS_TIMELINE => {
			if shutdown_menu.get() {
				return;
			}
			let _ = ui_tx_menu.send(UiCommand::OpenTimeline(crate::timeline::TimelineType::Mentions));
		}
		ID_BOOKMARKS_TIMELINE => {
			if shutdown_menu.get() {
				return;
			}
			let _ = ui_tx_menu.send(UiCommand::OpenTimeline(crate::timeline::TimelineType::Bookmarks));
		}
		ID_FAVORITES_TIMELINE => {
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
		ID_PLAY_MEDIA => {
			if shutdown_menu.get() {
				return;
			}
			let _ = ui_tx_menu.send(UiCommand::PlayMedia);
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
		ID_COPY_POST_LINK => {
			if shutdown_menu.get() {
				return;
			}
			let _ = ui_tx_menu.send(UiCommand::CopyPostLink);
		}
		ID_VIEW_POST => {
			if shutdown_menu.get() {
				return;
			}
			let _ = ui_tx_menu.send(UiCommand::ViewPost);
		}
		ID_VIEW_THREAD => {
			if shutdown_menu.get() {
				return;
			}
			let _ = ui_tx_menu.send(UiCommand::ViewThread);
		}
		ID_VIEW_QUOTED_THREAD => {
			if shutdown_menu.get() {
				return;
			}
			let _ = ui_tx_menu.send(UiCommand::ViewQuotedThread);
		}
		ID_LOAD_MORE => {
			if shutdown_menu.get() {
				return;
			}
			let _ = ui_tx_menu.send(UiCommand::LoadMore);
		}
		ID_VOTE => {
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
		ID_FIND => {
			if shutdown_menu.get() {
				return;
			}
			if let Some(query) = dialogs::show_find_dialog(&frame_menu) {
				let _ = ui_tx_menu.send(UiCommand::Find(query));
			}
		}
		ID_FIND_NEXT => {
			if shutdown_menu.get() {
				return;
			}
			let _ = ui_tx_menu.send(UiCommand::FindNext);
		}
		ID_FIND_PREV => {
			if shutdown_menu.get() {
				return;
			}
			let _ = ui_tx_menu.send(UiCommand::FindPrev);
		}
		ID_CHECK_FOR_UPDATES => {
			if shutdown_menu.get() {
				return;
			}
			let _ = ui_tx_menu.send(UiCommand::CheckForUpdates);
		}
		_ => {}
	});
}
