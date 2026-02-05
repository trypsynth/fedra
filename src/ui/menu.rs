use wxdragon::prelude::*;

use crate::{
	AppState, ID_BOOKMARK, ID_BOOST, ID_CLOSE_TIMELINE, ID_COPY_POST, ID_DELETE_POST, ID_DIRECT_TIMELINE, ID_EDIT_POST,
	ID_EDIT_PROFILE, ID_FAVORITE, ID_FEDERATED_TIMELINE, ID_LOAD_MORE, ID_LOCAL_TIMELINE, ID_MANAGE_ACCOUNTS,
	ID_NEW_POST, ID_OPEN_LINKS, ID_OPEN_USER_TIMELINE_BY_INPUT, ID_OPTIONS, ID_REFRESH, ID_REPLY, ID_REPLY_AUTHOR,
	ID_SEARCH, ID_VIEW_BOOSTS, ID_VIEW_FAVORITES, ID_VIEW_HASHTAGS, ID_VIEW_HELP, ID_VIEW_IN_BROWSER, ID_VIEW_MENTIONS,
	ID_VIEW_PROFILE, ID_VIEW_THREAD, ID_VIEW_USER_TIMELINE, commands::get_selected_status,
};

pub fn build_menu_bar() -> MenuBar {
	let file_menu = Menu::builder().build();
	file_menu.append(
		ID_MANAGE_ACCOUNTS,
		"Manage &Accounts...\tCtrl+Alt+A",
		"Add, remove or switch accounts",
		ItemKind::Normal,
	);
	file_menu.append(
		ID_EDIT_PROFILE,
		"Edit &Profile...\tCtrl+Shift+E",
		"Edit current account profile",
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
		.append(ID_VIEW_PROFILE, "View &Profile\tCtrl+P", "View profile of selected post's author", ItemKind::Normal)
		.expect("Failed to append view profile menu item");
	post_menu
		.append(ID_VIEW_MENTIONS, "View &Mentions\tCtrl+M", "View mentions in selected post", ItemKind::Normal)
		.expect("Failed to append view mentions menu item");
	post_menu
		.append(ID_VIEW_HASHTAGS, "View &Hashtags\tCtrl+H", "View hashtags in selected post", ItemKind::Normal)
		.expect("Failed to append view hashtags menu item");
	post_menu
		.append(ID_OPEN_LINKS, "Open &Links\tAlt+Enter", "Open links in selected post", ItemKind::Normal)
		.expect("Failed to append open links menu item");
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
		.append(ID_VIEW_THREAD, "View &Thread\tEnter", "View conversation thread for selected post", ItemKind::Normal)
		.expect("Failed to append view thread menu item");
	post_menu.append_separator();
	post_menu
		.append(ID_EDIT_POST, "&Edit Post...\tCtrl+E", "Edit selected post", ItemKind::Normal)
		.expect("Failed to append edit post menu item");
	post_menu
		.append(ID_DELETE_POST, "&Delete Post\tCtrl+Delete", "Delete selected post", ItemKind::Normal)
		.expect("Failed to append delete post menu item");
	post_menu.append_separator();
	let vote_shortcut = "Ctrl+V";
	post_menu
		.append(
			crate::ID_VOTE,
			&format!("&Vote\t{vote_shortcut}"),
			"Vote on poll in selected post...",
			ItemKind::Normal,
		)
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
		.append_item(ID_LOCAL_TIMELINE, "&Local Timeline\tCtrl+L", "Open local timeline")
		.append_item(ID_FEDERATED_TIMELINE, "&Federated Timeline", "Open federated timeline")
		.append_item(ID_DIRECT_TIMELINE, "&Direct Messages\tCtrl+D", "Open direct messages timeline")
		.append_item(crate::ID_BOOKMARKS_TIMELINE, "&Bookmarks", "Open bookmarks timeline")
		.append_item(crate::ID_FAVORITES_TIMELINE, "F&avorites", "Open favorites timeline")
		.append_separator()
		.append_item(ID_LOAD_MORE, "Load &More\t.", "Load more posts from server")
		.append_separator()
		.append_item(ID_CLOSE_TIMELINE, "&Close Timeline", "Close current timeline")
		.append_separator()
		.append_item(ID_REFRESH, "&Refresh\tF5", "Refresh current timeline")
		.build();
	let help_menu = Menu::builder().append_item(ID_VIEW_HELP, "View &Help\tF1", "Open documentation").build();
	MenuBar::builder()
		.append(file_menu, "&Options")
		.append(post_menu, "&Post")
		.append(timelines_menu, "&Timelines")
		.append(help_menu, "&Help")
		.build()
}

pub fn update_menu_labels(menu_bar: &MenuBar, state: &AppState) {
	let status = get_selected_status(state);
	let target = status.and_then(|s| s.reblog.as_deref().or(Some(s)));
	if let Some(fav_item) = menu_bar.find_item(ID_FAVORITE) {
		let shortcut = if state.config.quick_action_keys { "F" } else { "Ctrl+Shift+F" };
		let label = if target.is_some_and(|t| t.favourited) {
			format!("Un&favorite\t{shortcut}")
		} else {
			format!("&Favorite\t{shortcut}")
		};
		fav_item.set_label(&label);
	}
	if let Some(bookmark_item) = menu_bar.find_item(ID_BOOKMARK) {
		let shortcut = if state.config.quick_action_keys { "K" } else { "Ctrl+Shift+K" };
		let label = if target.is_some_and(|t| t.bookmarked) {
			format!("Un&bookmark\t{shortcut}")
		} else {
			format!("&Bookmark\t{shortcut}")
		};
		bookmark_item.set_label(&label);
	}
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
					let shortcut = if state.config.quick_action_keys { "B" } else { "Ctrl+Shift+B" };
					let label = if target.is_some_and(|t| t.reblogged) {
						format!("Un&boost\t{shortcut}")
					} else {
						format!("&Boost\t{shortcut}")
					};
					post_menu.insert(pos + 1, ID_BOOST, &label, "Boost or unboost selected post", ItemKind::Normal);
				}
			} else if let Some(boost_item) = post_menu.find_item(ID_BOOST) {
				let shortcut = if state.config.quick_action_keys { "B" } else { "Ctrl+Shift+B" };
				let label = if target.is_some_and(|t| t.reblogged) {
					format!("Un&boost\t{shortcut}")
				} else {
					format!("&Boost\t{shortcut}")
				};
				boost_item.set_label(&label);
			}
		}
	} else if let Some(boost_item) = menu_bar.find_item(ID_BOOST) {
		let shortcut = if state.config.quick_action_keys { "B" } else { "Ctrl+Shift+B" };
		let label = if target.is_some_and(|t| t.reblogged) {
			format!("Un&boost\t{shortcut}")
		} else {
			format!("&Boost\t{shortcut}")
		};
		boost_item.set_label(&label);
	}
	if let Some(new_post_item) = menu_bar.find_item(ID_NEW_POST) {
		let shortcut = if state.config.quick_action_keys { "C" } else { "Ctrl+N" };
		let label = format!("&New Post...\t{shortcut}");
		new_post_item.set_label(&label);
	}
	if let Some(reply_item) = menu_bar.find_item(ID_REPLY) {
		let shortcut = if state.config.quick_action_keys { "R" } else { "Ctrl+R" };
		let label = format!("&Reply...\t{shortcut}");
		reply_item.set_label(&label);
	}
	if let Some(reply_author_item) = menu_bar.find_item(ID_REPLY_AUTHOR) {
		let shortcut = if state.config.quick_action_keys { "Ctrl+R" } else { "Ctrl+Shift+R" };
		let label = format!("Reply to &Author...\t{shortcut}");
		reply_author_item.set_label(&label);
	}
	if let Some(view_profile_item) = menu_bar.find_item(ID_VIEW_PROFILE) {
		let shortcut = if state.config.quick_action_keys { "P" } else { "Ctrl+P" };
		let label = format!("View &Profile\t{shortcut}");
		view_profile_item.set_label(&label);
	}
	if let Some(view_hashtags_item) = menu_bar.find_item(ID_VIEW_HASHTAGS) {
		let shortcut = if state.config.quick_action_keys { "H" } else { "Ctrl+H" };
		let label = format!("View &Hashtags\t{shortcut}");
		view_hashtags_item.set_label(&label);
	}
	if let Some(view_mentions_item) = menu_bar.find_item(ID_VIEW_MENTIONS) {
		let shortcut = if state.config.quick_action_keys { "M" } else { "Ctrl+M" };
		let label = format!("View &Mentions\t{shortcut}");
		view_mentions_item.set_label(&label);
	}
	if let Some(copy_post_item) = menu_bar.find_item(ID_COPY_POST) {
		copy_post_item.enable(status.is_some());
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
			if boosts > 0 && !boosts_exists {
				post_menu.insert(
					pos + 1,
					ID_VIEW_BOOSTS,
					"&View Boosts",
					"View users who boosted this post",
					ItemKind::Normal,
				);
			} else if boosts == 0 && boosts_exists {
				post_menu.delete(ID_VIEW_BOOSTS);
			}
			let favorites_exists = post_menu.find_item(ID_VIEW_FAVORITES).is_some();
			if favorites > 0 && !favorites_exists {
				let insert_pos = if boosts > 0 { pos + 2 } else { pos + 1 };
				post_menu.insert(
					insert_pos,
					ID_VIEW_FAVORITES,
					"&View Favorites",
					"View users who favorited this post",
					ItemKind::Normal,
				);
			} else if favorites == 0 && favorites_exists {
				post_menu.delete(ID_VIEW_FAVORITES);
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
			if is_own && !edit_exists {
				let shortcut = if state.config.quick_action_keys { "E" } else { "Ctrl+E" };
				let label = format!("&Edit Post...\t{shortcut}");
				post_menu.insert(pos + 2, ID_EDIT_POST, &label, "Edit selected post", ItemKind::Normal);
			} else if !is_own && edit_exists {
				post_menu.delete(ID_EDIT_POST);
			} else if is_own
				&& edit_exists
				&& let Some(item) = post_menu.find_item(ID_EDIT_POST)
			{
				let shortcut = if state.config.quick_action_keys { "E" } else { "Ctrl+E" };
				let label = format!("&Edit Post...\t{shortcut}");
				item.set_label(&label);
			}

			let delete_exists = post_menu.find_item(ID_DELETE_POST).is_some();
			if is_own && !delete_exists {
				let shortcut = if state.config.quick_action_keys { "D" } else { "Ctrl+Delete" };
				let label = format!("&Delete Post\t{shortcut}");
				post_menu.insert(pos + 3, ID_DELETE_POST, &label, "Delete selected post", ItemKind::Normal);
			} else if !is_own && delete_exists {
				post_menu.delete(ID_DELETE_POST);
			} else if is_own
				&& delete_exists
				&& let Some(item) = post_menu.find_item(ID_DELETE_POST)
			{
				let shortcut = if state.config.quick_action_keys { "D" } else { "Ctrl+Delete" };
				let label = format!("&Delete Post\t{shortcut}");
				item.set_label(&label);
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
				let vote_exists = post_menu.find_item(crate::ID_VOTE).is_some();
				if has_poll && !vote_exists {
					let shortcut = if state.config.quick_action_keys { "V" } else { "Ctrl+V" };
					let label = format!("&Vote\t{shortcut}");
					post_menu.insert(
						f_pos,
						crate::ID_VOTE,
						&label,
						"Vote on poll in selected post...",
						ItemKind::Normal,
					);
				} else if !has_poll && vote_exists {
					post_menu.delete(crate::ID_VOTE);
				} else if has_poll
					&& vote_exists && let Some(vote_item) = post_menu.find_item(crate::ID_VOTE)
				{
					let shortcut = if state.config.quick_action_keys { "V" } else { "Ctrl+V" };
					let label = format!("&Vote\t{shortcut}");
					vote_item.set_label(&label);
				}
			}
		}
	}

	if let Some(load_more_item) = menu_bar.find_item(ID_LOAD_MORE) {
		let shortcut = if state.config.quick_action_keys { "." } else { "Ctrl+." };
		let label = format!("Load &More\t{shortcut}");
		load_more_item.set_label(&label);
		let supports_paging =
			state.timeline_manager.active().is_some_and(|timeline| timeline.timeline_type.supports_paging());
		load_more_item.enable(supports_paging);
	}
	if let Some(search_item) = menu_bar.find_item(ID_SEARCH) {
		let shortcut = if state.config.quick_action_keys { "/" } else { "Ctrl+/" };
		let label = format!("&Search...\t{shortcut}");
		search_item.set_label(&label);
	}
	if let Some(open_user_item) = menu_bar.find_item(ID_OPEN_USER_TIMELINE_BY_INPUT) {
		let shortcut = if state.config.quick_action_keys { "U" } else { "Ctrl+U" };
		let label = format!("Open &User...\t{shortcut}");
		open_user_item.set_label(&label);
	}
}
