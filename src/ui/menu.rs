use wxdragon::prelude::*;

use crate::{
	AppState, ContextMenuState, ID_BOOKMARK, ID_BOOKMARKS_TIMELINE, ID_BOOST, ID_CHECK_FOR_UPDATES, ID_CLOSE_TIMELINE,
	ID_COPY_POST, ID_COPY_POST_LINK, ID_CUSTOMIZE_SHORTCUTS, ID_DELETE_POST, ID_DIRECT_TIMELINE, ID_EDIT_POST,
	ID_EDIT_PROFILE, ID_FAVORITE, ID_FAVORITES_TIMELINE, ID_FEDERATED_TIMELINE, ID_FIND, ID_FIND_NEXT, ID_FIND_PREV,
	ID_HOME_TIMELINE, ID_LOAD_MORE, ID_LOCAL_TIMELINE, ID_MANAGE_ACCOUNTS, ID_MANAGE_FILTERS, ID_MANAGE_LISTS,
	ID_MENTIONS_TIMELINE, ID_NEW_POST, ID_NOTIFICATIONS_TIMELINE, ID_OPEN_INSTANCE_TIMELINE_BY_INPUT, ID_OPEN_LINKS,
	ID_OPEN_LIST, ID_OPEN_USER_TIMELINE_BY_INPUT, ID_OPTIONS, ID_PIN_POST, ID_PLAY_MEDIA, ID_QUOTE, ID_REFRESH,
	ID_REPLY, ID_REPLY_AUTHOR, ID_SEARCH, ID_SENT_TIMELINE, ID_TOGGLE_FOLLOW, ID_VIEW_BOOSTS, ID_VIEW_FAVORITES,
	ID_VIEW_HASHTAGS, ID_VIEW_HELP, ID_VIEW_IN_BROWSER, ID_VIEW_MENTIONS, ID_VIEW_POST, ID_VIEW_PROFILE,
	ID_VIEW_QUOTED_THREAD, ID_VIEW_THREAD, ID_VIEW_USER_TIMELINE, ID_VOTE, config::ActionId,
	ui::commands::get_selected_status,
};

pub fn build_menu_bar() -> MenuBar {
	let file_menu = Menu::builder().build();
	file_menu.append(
		ID_MANAGE_ACCOUNTS,
		"Manage &Accounts...\tCtrl+Alt+A",
		"Add, remove or switch accounts",
		ItemKind::Normal,
	);
	file_menu.append(ID_MANAGE_FILTERS, "Manage &Filters...", "Manage content filters", ItemKind::Normal);
	file_menu.append(ID_MANAGE_LISTS, "Manage &Lists...", "Create and manage lists", ItemKind::Normal);
	file_menu.append(
		ID_EDIT_PROFILE,
		"Edit &Profile...\tCtrl+Shift+E",
		"Edit current account profile",
		ItemKind::Normal,
	);
	file_menu.append(
		ID_CUSTOMIZE_SHORTCUTS,
		"Customize &Keyboard Shortcuts...",
		"Customize application keyboard shortcuts",
		ItemKind::Normal,
	);
	file_menu.append_separator();
	file_menu.append(ID_OPTIONS, "&Options\tCtrl+,", "Configure application settings", ItemKind::Normal);
	let post_menu = Menu::builder().build();
	post_menu
		.append(ID_NEW_POST, "&New Post...\tCtrl+N", "Create a new post", ItemKind::Normal)
		.expect("Failed to append new post menu item");
	post_menu
		.append(ID_REPLY, "&Reply...\tCtrl+R", "Reply to all mentioned users", ItemKind::Normal)
		.expect("Failed to append reply menu item");
	post_menu
		.append(ID_REPLY_AUTHOR, "Reply to &Author...\tCtrl+Shift+R", "Reply to author only", ItemKind::Normal)
		.expect("Failed to append reply author menu item");
	post_menu
		.append(ID_QUOTE, "&Quote...\tCtrl+Q", "Quote this post", ItemKind::Normal)
		.expect("Failed to append quote menu item");
	post_menu
		.append(ID_TOGGLE_FOLLOW, "Toggle &Follow\tAlt+F", "Follow or unfollow the author", ItemKind::Normal)
		.expect("Failed to append toggle follow menu item");
	post_menu
		.append(ID_VIEW_PROFILE, "View &Profile\tCtrl+P", "View profile of selected post's author", ItemKind::Normal)
		.expect("Failed to append view profile menu item");
	post_menu
		.append(ID_VIEW_MENTIONS, "View &Mentions\tCtrl+M", "View mentions in selected post", ItemKind::Normal)
		.expect("Failed to append view mentions menu item");
	post_menu
		.append(ID_VIEW_HASHTAGS, "View &Hashtags\tCtrl+H", "View hashtags in selected post", ItemKind::Normal)
		.expect("Failed to append view hashtags menu item");
	post_menu
		.append(ID_OPEN_LINKS, "Open &Links\tEnter", "Open links in selected post", ItemKind::Normal)
		.expect("Failed to append open links menu item");
	post_menu
		.append(ID_PLAY_MEDIA, "Play &Media\tCtrl+I", "Play media attached to selected post", ItemKind::Normal)
		.expect("Failed to append play media menu item");
	post_menu
		.append(
			ID_VIEW_IN_BROWSER,
			"&Open in Browser\tCtrl+Shift+O",
			"Open selected post in web browser",
			ItemKind::Normal,
		)
		.expect("Failed to append open in browser menu item");
	post_menu
		.append(ID_COPY_POST, "&Copy Post\tCtrl+Shift+C", "Copy selected post text", ItemKind::Normal)
		.expect("Failed to append copy post menu item");
	post_menu
		.append(ID_COPY_POST_LINK, "Copy Post &Link\tCtrl+C", "Copy selected post URL", ItemKind::Normal)
		.expect("Failed to append copy post link menu item");
	post_menu
		.append(ID_VIEW_POST, "View &Post Details\tShift+Enter", "View post content in a dialog", ItemKind::Normal)
		.expect("Failed to append view post menu item");
	post_menu
		.append(
			ID_VIEW_THREAD,
			"View &Thread\tAlt+Enter",
			"View conversation thread for selected post",
			ItemKind::Normal,
		)
		.expect("Failed to append view thread menu item");
	post_menu
		.append(
			ID_VIEW_QUOTED_THREAD,
			"View &Quoted Thread",
			"View conversation thread for quoted post",
			ItemKind::Normal,
		)
		.expect("Failed to append view quoted thread menu item");
	post_menu.append_separator();
	post_menu
		.append(ID_EDIT_POST, "&Edit Post...\tCtrl+E", "Edit selected post", ItemKind::Normal)
		.expect("Failed to append edit post menu item");
	post_menu
		.append(ID_DELETE_POST, "&Delete Post", "Delete selected post", ItemKind::Normal)
		.expect("Failed to append delete post menu item");
	post_menu.append_separator();
	let vote_shortcut = "Ctrl+V";
	post_menu
		.append(ID_VOTE, &format!("&Vote\t{vote_shortcut}"), "Vote on poll in selected post...", ItemKind::Normal)
		.expect("Failed to append vote menu item");
	post_menu
		.append(ID_FAVORITE, "&Favorite\tCtrl+Shift+F", "Favorite or unfavorite selected post", ItemKind::Normal)
		.expect("Failed to append favorite menu item");
	post_menu
		.append(ID_BOOKMARK, "&Bookmark\tCtrl+Shift+K", "Bookmark or unbookmark selected post", ItemKind::Normal)
		.expect("Failed to append bookmark menu item");
	post_menu
		.append(ID_BOOST, "&Boost\tCtrl+Shift+B", "Boost or unboost selected post", ItemKind::Normal)
		.expect("Failed to append boost menu item");
	post_menu.append_separator();
	let timelines_menu = Menu::builder()
		.append_item(ID_VIEW_USER_TIMELINE, "&User Timeline\tCtrl+T", "Open timeline of selected post's author")
		.append_item(ID_OPEN_USER_TIMELINE_BY_INPUT, "Open &User...\tCtrl+U", "Open a user by username")
		.append_item(ID_SEARCH, "&Search...\tCtrl+/", "Search for accounts, hashtags, or posts")
		.append_separator()
		.append_item(ID_FIND, "&Find in Timeline...\tCtrl+F", "Find text in current timeline")
		.append_item(ID_FIND_NEXT, "Find &Next\tF3", "Find next occurrence")
		.append_item(ID_FIND_PREV, "Find &Previous\tShift+F3", "Find previous occurrence")
		.append_separator()
		.append_item(ID_HOME_TIMELINE, "&Home Timeline", "Open home timeline")
		.append_item(ID_NOTIFICATIONS_TIMELINE, "&Notifications", "Open notifications timeline")
		.append_item(ID_SENT_TIMELINE, "Se&nt", "Open a timeline of your own posts")
		.append_item(ID_LOCAL_TIMELINE, "&Local Timeline\tCtrl+L", "Open local timeline")
		.append_item(
			ID_OPEN_INSTANCE_TIMELINE_BY_INPUT,
			"Open &Instance Timeline...\tCtrl+Shift+I",
			"Open an instance's local timeline by domain",
		)
		.append_item(ID_FEDERATED_TIMELINE, "&Federated Timeline", "Open federated timeline")
		.append_item(ID_DIRECT_TIMELINE, "&Direct Messages\tCtrl+D", "Open direct messages timeline")
		.append_item(ID_MENTIONS_TIMELINE, "&Mentions\tCtrl+Shift+M", "Open mentions timeline")
		.append_item(ID_BOOKMARKS_TIMELINE, "&Bookmarks", "Open bookmarks timeline")
		.append_item(ID_FAVORITES_TIMELINE, "F&avorites", "Open favorites timeline")
		.append_item(ID_OPEN_LIST, "Open &List...", "Open a Mastodon list")
		.append_separator()
		.append_item(ID_LOAD_MORE, "Load &More\t.", "Load more posts from server")
		.append_separator()
		.append_item(ID_CLOSE_TIMELINE, "&Close Timeline", "Close current timeline")
		.append_separator()
		.append_item(ID_REFRESH, "&Refresh\tF5", "Refresh current timeline")
		.build();
	let help_menu = Menu::builder()
		.append_item(ID_CHECK_FOR_UPDATES, "Check for &Updates...", "Check for application updates")
		.append_item(ID_VIEW_HELP, "View &Help\tF1", "Open documentation")
		.build();
	MenuBar::builder()
		.append(file_menu, "&Options")
		.append(post_menu, "&Post")
		.append(timelines_menu, "&Timelines")
		.append(help_menu, "&Help")
		.build()
}

fn set_item_label(menu_bar: &MenuBar, id: i32, base: &str, shortcut: &str) {
	if let Some(item) = menu_bar.find_item(id) {
		let label = if shortcut.is_empty() { base.to_string() } else { format!("{base}\t{shortcut}") };
		item.set_label(&label);
	}
}

pub fn update_menu_labels(menu_bar: &MenuBar, state: &AppState) {
	let status = get_selected_status(state);
	let target = status.and_then(|s| s.reblog.as_deref().or(Some(s)));
	let q = state.config.quick_action_keys;
	let sc = &state.config.shortcuts;

	let fav_shortcut = sc.get_menu_str(q, ActionId::Favorite);
	let fav_base = if target.is_some_and(|t| t.favourited) { "Un&favorite" } else { "&Favorite" };
	set_item_label(menu_bar, ID_FAVORITE, fav_base, &fav_shortcut);

	let bookmark_shortcut = sc.get_menu_str(q, ActionId::Bookmark);
	let bookmark_base = if target.is_some_and(|t| t.bookmarked) { "Un&bookmark" } else { "&Bookmark" };
	set_item_label(menu_bar, ID_BOOKMARK, bookmark_base, &bookmark_shortcut);

	let boost_shortcut = sc.get_menu_str(q, ActionId::Boost);
	let boost_base = if target.is_some_and(|t| t.reblogged) { "Un&boost" } else { "&Boost" };

	if let Some((_, post_menu)) = menu_bar.find_item_and_menu(ID_BOOKMARK) {
		let is_direct = target.is_some_and(|t| t.visibility == "direct");
		let boost_exists = post_menu.find_item(ID_BOOST).is_some();

		if is_direct && boost_exists {
			post_menu.delete(ID_BOOST);
		} else if !is_direct {
			if !boost_exists {
				let mut bookmark_pos = None;
				for i in 0..post_menu.get_item_count() {
					if let Some(item) = post_menu.find_item_by_position(i)
						&& item.get_id() == ID_BOOKMARK
					{
						bookmark_pos = Some(i);
						break;
					}
				}

				if let Some(pos) = bookmark_pos {
					let label = if boost_shortcut.is_empty() {
						boost_base.to_string()
					} else {
						format!("{boost_base}\t{boost_shortcut}")
					};
					post_menu.insert(pos + 1, ID_BOOST, &label, "Boost or unboost selected post", ItemKind::Normal);
				}
			} else if let Some(boost_item) = post_menu.find_item(ID_BOOST) {
				let label = if boost_shortcut.is_empty() {
					boost_base.to_string()
				} else {
					format!("{boost_base}\t{boost_shortcut}")
				};
				boost_item.set_label(&label);
			}
		}
	} else if let Some(boost_item) = menu_bar.find_item(ID_BOOST) {
		let label =
			if boost_shortcut.is_empty() { boost_base.to_string() } else { format!("{boost_base}\t{boost_shortcut}") };
		boost_item.set_label(&label);
	}

	set_item_label(menu_bar, ID_NEW_POST, "&New Post...", &sc.get_menu_str(q, ActionId::NewPost));
	set_item_label(menu_bar, ID_REPLY, "&Reply...", &sc.get_menu_str(q, ActionId::Reply));
	set_item_label(menu_bar, ID_REPLY_AUTHOR, "Reply to &Author...", &sc.get_menu_str(q, ActionId::ReplyAuthor));
	set_item_label(menu_bar, ID_QUOTE, "&Quote...", &sc.get_menu_str(q, ActionId::Quote));
	set_item_label(menu_bar, ID_VIEW_PROFILE, "View &Profile", &sc.get_menu_str(q, ActionId::ViewProfile));
	set_item_label(menu_bar, ID_VIEW_HASHTAGS, "View &Hashtags", &sc.get_menu_str(q, ActionId::ViewHashtags));
	set_item_label(menu_bar, ID_VIEW_MENTIONS, "View &Mentions", &sc.get_menu_str(q, ActionId::ViewMentions));
	set_item_label(menu_bar, ID_OPEN_LINKS, "Open &Links", &sc.get_menu_str(q, ActionId::OpenLinks));
	set_item_label(menu_bar, ID_PLAY_MEDIA, "Play &Media", &sc.get_menu_str(q, ActionId::PlayMedia));
	set_item_label(menu_bar, ID_VIEW_IN_BROWSER, "&Open in Browser", &sc.get_menu_str(q, ActionId::ViewInBrowser));
	set_item_label(menu_bar, ID_COPY_POST, "&Copy Post", &sc.get_menu_str(q, ActionId::CopyPost));
	set_item_label(menu_bar, ID_COPY_POST_LINK, "Copy Post &Link", &sc.get_menu_str(q, ActionId::CopyPostLink));
	set_item_label(menu_bar, ID_VIEW_POST, "View &Post Details", &sc.get_menu_str(q, ActionId::ViewPost));
	set_item_label(menu_bar, ID_VIEW_THREAD, "View &Thread", &sc.get_menu_str(q, ActionId::ViewThread));
	set_item_label(
		menu_bar,
		ID_VIEW_QUOTED_THREAD,
		"View &Quoted Thread",
		&sc.get_menu_str(q, ActionId::ViewQuotedThread),
	);
	set_item_label(menu_bar, ID_TOGGLE_FOLLOW, "Toggle &Follow", &sc.get_menu_str(q, ActionId::ToggleFollow));

	if let Some(copy_post_item) = menu_bar.find_item(ID_COPY_POST) {
		copy_post_item.enable(status.is_some());
	}
	if let Some(copy_post_link_item) = menu_bar.find_item(ID_COPY_POST_LINK) {
		let enable = status.map_or(false, |s| s.reblog.as_ref().map_or(s, std::convert::AsRef::as_ref).url.is_some());
		copy_post_link_item.enable(enable);
	}
	if let Some((_, post_menu)) = menu_bar.find_item_and_menu(ID_VIEW_HASHTAGS) {
		let mut anchor_pos = None;
		for i in 0..post_menu.get_item_count() {
			if let Some(item) = post_menu.find_item_by_position(i)
				&& item.get_id() == ID_VIEW_HASHTAGS
			{
				anchor_pos = Some(i);
				break;
			}
		}
		if let Some(pos) = anchor_pos {
			let boosts = target.map_or(0, |t| t.reblogs_count);
			let favorites = target.map_or(0, |t| t.favourites_count);
			let boosts_exists = post_menu.find_item(ID_VIEW_BOOSTS).is_some();
			let boosts_shortcut = sc.get_menu_str(q, ActionId::ViewBoosts);
			let boosts_label = if boosts_shortcut.is_empty() {
				"&View Boosts".to_string()
			} else {
				format!("&View Boosts\t{boosts_shortcut}")
			};
			if boosts > 0 && !boosts_exists {
				post_menu.insert(
					pos + 1,
					ID_VIEW_BOOSTS,
					&boosts_label,
					"View users who boosted this post",
					ItemKind::Normal,
				);
			} else if boosts == 0 && boosts_exists {
				post_menu.delete(ID_VIEW_BOOSTS);
			} else if boosts > 0
				&& boosts_exists
				&& let Some(item) = post_menu.find_item(ID_VIEW_BOOSTS)
			{
				item.set_label(&boosts_label);
			}

			let favorites_exists = post_menu.find_item(ID_VIEW_FAVORITES).is_some();
			let favorites_shortcut = sc.get_menu_str(q, ActionId::ViewFavorites);
			let favorites_label = if favorites_shortcut.is_empty() {
				"&View Favorites".to_string()
			} else {
				format!("&View Favorites\t{favorites_shortcut}")
			};
			if favorites > 0 && !favorites_exists {
				let insert_pos = if boosts > 0 { pos + 2 } else { pos + 1 };
				post_menu.insert(
					insert_pos,
					ID_VIEW_FAVORITES,
					&favorites_label,
					"View users who favorited this post",
					ItemKind::Normal,
				);
			} else if favorites == 0 && favorites_exists {
				post_menu.delete(ID_VIEW_FAVORITES);
			} else if favorites > 0
				&& favorites_exists
				&& let Some(item) = post_menu.find_item(ID_VIEW_FAVORITES)
			{
				item.set_label(&favorites_label);
			}
		}
	}
	let is_own = target.is_some_and(|t| Some(&t.account.id) == state.current_user_id.as_ref());
	let has_poll = target.is_some_and(|t| t.poll.is_some());

	if let Some((_, post_menu)) = menu_bar.find_item_and_menu(ID_VIEW_THREAD) {
		let mut anchor_pos = None;
		let count = post_menu.get_item_count();
		for i in 0..count {
			if let Some(item) = post_menu.find_item_by_position(i)
				&& item.get_id() == ID_VIEW_THREAD
			{
				anchor_pos = Some(i);
				break;
			}
		}

		if let Some(pos) = anchor_pos {
			let edit_exists = post_menu.find_item(ID_EDIT_POST).is_some();
			let edit_shortcut = sc.get_menu_str(q, ActionId::EditPost);
			let edit_label = if edit_shortcut.is_empty() {
				"&Edit Post...".to_string()
			} else {
				format!("&Edit Post...\t{edit_shortcut}")
			};
			if is_own && !edit_exists {
				post_menu.insert(pos + 2, ID_EDIT_POST, &edit_label, "Edit selected post", ItemKind::Normal);
			} else if !is_own && edit_exists {
				post_menu.delete(ID_EDIT_POST);
			} else if is_own
				&& edit_exists
				&& let Some(item) = post_menu.find_item(ID_EDIT_POST)
			{
				item.set_label(&edit_label);
			}

			let delete_exists = post_menu.find_item(ID_DELETE_POST).is_some();
			let delete_shortcut = sc.get_menu_str(q, ActionId::DeletePost);
			let delete_label = if delete_shortcut.is_empty() {
				"&Delete Post".to_string()
			} else {
				format!("&Delete Post\t{delete_shortcut}")
			};
			if is_own && !delete_exists {
				post_menu.insert(pos + 3, ID_DELETE_POST, &delete_label, "Delete selected post", ItemKind::Normal);
			} else if !is_own && delete_exists {
				post_menu.delete(ID_DELETE_POST);
			} else if is_own
				&& delete_exists
				&& let Some(item) = post_menu.find_item(ID_DELETE_POST)
			{
				item.set_label(&delete_label);
			}

			let pin_exists = post_menu.find_item(ID_PIN_POST).is_some();
			if is_own {
				let is_pinned = target.is_some_and(|t| t.pinned);
				let pin_base = if is_pinned { "&Unpin Post" } else { "&Pin Post" };
				let pin_shortcut = sc.get_menu_str(q, ActionId::PinPost);
				let pin_label =
					if pin_shortcut.is_empty() { pin_base.to_string() } else { format!("{pin_base}\t{pin_shortcut}") };
				if !pin_exists {
					let mut delete_pos = None;
					for i in 0..post_menu.get_item_count() {
						if let Some(item) = post_menu.find_item_by_position(i)
							&& item.get_id() == ID_DELETE_POST
						{
							delete_pos = Some(i);
							break;
						}
					}
					if let Some(dp) = delete_pos {
						post_menu.insert(
							dp + 1,
							ID_PIN_POST,
							&pin_label,
							"Pin or unpin this post on your profile",
							ItemKind::Normal,
						);
					}
				} else if let Some(item) = post_menu.find_item(ID_PIN_POST) {
					item.set_label(&pin_label);
				}
			} else if pin_exists {
				post_menu.delete(ID_PIN_POST);
			}

			let mut fav_pos = None;
			for i in 0..post_menu.get_item_count() {
				if let Some(item) = post_menu.find_item_by_position(i)
					&& item.get_id() == ID_FAVORITE
				{
					fav_pos = Some(i);
					break;
				}
			}

			if let Some(f_pos) = fav_pos {
				let vote_exists = post_menu.find_item(ID_VOTE).is_some();
				let vote_shortcut = sc.get_menu_str(q, ActionId::Vote);
				let vote_label =
					if vote_shortcut.is_empty() { "&Vote".to_string() } else { format!("&Vote\t{vote_shortcut}") };
				if has_poll && !vote_exists {
					post_menu.insert(f_pos, ID_VOTE, &vote_label, "Vote on poll in selected post...", ItemKind::Normal);
				} else if !has_poll && vote_exists {
					post_menu.delete(ID_VOTE);
				} else if has_poll
					&& vote_exists && let Some(vote_item) = post_menu.find_item(ID_VOTE)
				{
					vote_item.set_label(&vote_label);
				}
			}
		}
	}

	state.context_menu_state.set(ContextMenuState {
		favourited: target.is_some_and(|t| t.favourited),
		reblogged: target.is_some_and(|t| t.reblogged),
		bookmarked: target.is_some_and(|t| t.bookmarked),
		pinned: target.is_some_and(|t| t.pinned),
		is_direct: target.is_some_and(|t| t.visibility == "direct"),
		is_own,
		quick_action_keys: state.config.quick_action_keys,
	});

	let supports_paging =
		state.timeline_manager.active().map_or(false, |timeline| timeline.timeline_type.supports_paging());
	set_item_label(menu_bar, ID_LOAD_MORE, "Load &More", &sc.get_menu_str(q, ActionId::LoadMore));
	if let Some(load_more_item) = menu_bar.find_item(ID_LOAD_MORE) {
		load_more_item.enable(supports_paging);
	}
	set_item_label(menu_bar, ID_SEARCH, "&Search...", &sc.get_menu_str(q, ActionId::Search));
	set_item_label(
		menu_bar,
		ID_OPEN_USER_TIMELINE_BY_INPUT,
		"Open &User...",
		&sc.get_menu_str(q, ActionId::OpenUserTimelineByInput),
	);
	set_item_label(
		menu_bar,
		ID_OPEN_INSTANCE_TIMELINE_BY_INPUT,
		"Open &Instance Timeline...",
		&sc.get_menu_str(q, ActionId::OpenInstanceTimelineByInput),
	);
	set_item_label(menu_bar, ID_PLAY_MEDIA, "Play &Media", &sc.get_menu_str(q, ActionId::PlayMedia));
	set_item_label(menu_bar, ID_VIEW_USER_TIMELINE, "&User Timeline", &sc.get_menu_str(q, ActionId::OpenUserTimeline));
	set_item_label(menu_bar, ID_FIND, "&Find in Timeline...", &sc.get_menu_str(q, ActionId::Find));
	set_item_label(menu_bar, ID_FIND_NEXT, "Find &Next", &sc.get_menu_str(q, ActionId::FindNext));
	set_item_label(menu_bar, ID_FIND_PREV, "Find &Previous", &sc.get_menu_str(q, ActionId::FindPrev));
	set_item_label(menu_bar, ID_HOME_TIMELINE, "&Home Timeline", &sc.get_menu_str(q, ActionId::HomeTimeline));
	set_item_label(
		menu_bar,
		ID_NOTIFICATIONS_TIMELINE,
		"&Notifications",
		&sc.get_menu_str(q, ActionId::NotificationsTimeline),
	);
	set_item_label(menu_bar, ID_SENT_TIMELINE, "Se&nt", &sc.get_menu_str(q, ActionId::SentTimeline));
	set_item_label(menu_bar, ID_LOCAL_TIMELINE, "&Local Timeline", &sc.get_menu_str(q, ActionId::LocalTimeline));
	set_item_label(
		menu_bar,
		ID_FEDERATED_TIMELINE,
		"&Federated Timeline",
		&sc.get_menu_str(q, ActionId::FederatedTimeline),
	);
	set_item_label(menu_bar, ID_DIRECT_TIMELINE, "&Direct Messages", &sc.get_menu_str(q, ActionId::DirectTimeline));
	set_item_label(menu_bar, ID_MENTIONS_TIMELINE, "&Mentions", &sc.get_menu_str(q, ActionId::MentionsTimeline));
	set_item_label(menu_bar, ID_BOOKMARKS_TIMELINE, "&Bookmarks", &sc.get_menu_str(q, ActionId::BookmarksTimeline));
	set_item_label(menu_bar, ID_FAVORITES_TIMELINE, "F&avorites", &sc.get_menu_str(q, ActionId::FavoritesTimeline));
	set_item_label(menu_bar, ID_OPEN_LIST, "Open &List...", &sc.get_menu_str(q, ActionId::OpenList));
	set_item_label(menu_bar, ID_CLOSE_TIMELINE, "&Close Timeline", &sc.get_menu_str(q, ActionId::CloseTimeline));
	set_item_label(menu_bar, ID_REFRESH, "&Refresh", &sc.get_menu_str(q, ActionId::Refresh));
	set_item_label(menu_bar, ID_MANAGE_ACCOUNTS, "Manage &Accounts...", &sc.get_menu_str(q, ActionId::ManageAccounts));
	set_item_label(menu_bar, ID_MANAGE_FILTERS, "Manage &Filters...", &sc.get_menu_str(q, ActionId::ManageFilters));
	set_item_label(menu_bar, ID_MANAGE_LISTS, "Manage &Lists...", &sc.get_menu_str(q, ActionId::ManageLists));
	set_item_label(menu_bar, ID_EDIT_PROFILE, "Edit &Profile...", &sc.get_menu_str(q, ActionId::EditProfile));
	set_item_label(
		menu_bar,
		ID_CUSTOMIZE_SHORTCUTS,
		"Customize &Keyboard Shortcuts...",
		&sc.get_menu_str(q, ActionId::CustomizeShortcuts),
	);
	set_item_label(menu_bar, ID_OPTIONS, "&Options", &sc.get_menu_str(q, ActionId::Options));
	set_item_label(
		menu_bar,
		ID_CHECK_FOR_UPDATES,
		"Check for &Updates...",
		&sc.get_menu_str(q, ActionId::CheckForUpdates),
	);
	set_item_label(menu_bar, ID_VIEW_HELP, "View &Help", &sc.get_menu_str(q, ActionId::ViewHelp));
}
