use wxdragon::prelude::*;

use crate::{
	AppState, ID_BOOST, ID_CLOSE_TIMELINE, ID_DELETE_POST, ID_EDIT_POST, ID_FAVOURITE, ID_FEDERATED_TIMELINE,
	ID_LOAD_MORE, ID_LOCAL_TIMELINE, ID_MANAGE_ACCOUNTS, ID_NEW_POST, ID_OPEN_LINKS, ID_OPEN_USER_TIMELINE_BY_INPUT,
	ID_OPTIONS, ID_REFRESH, ID_REPLY, ID_REPLY_AUTHOR, ID_VIEW_HASHTAGS, ID_VIEW_MENTIONS, ID_VIEW_PROFILE,
	ID_VIEW_THREAD, ID_VIEW_USER_TIMELINE, get_selected_status,
};

pub fn build_menu_bar() -> MenuBar {
	let file_menu = Menu::builder().build();
	file_menu
		.append(ID_VIEW_PROFILE, "View &Profile\tCtrl+P", "View profile of selected post's author", ItemKind::Normal)
		.expect("Failed to append view profile menu item");

	file_menu.append(ID_MANAGE_ACCOUNTS, "Manage &Accounts...", "Add, remove or switch accounts", ItemKind::Normal);
	file_menu.append_separator();
	file_menu.append(ID_OPTIONS, "&Options\tCtrl+,", "Configure application settings", ItemKind::Normal);

	let post_menu = Menu::builder().build();
	post_menu
		.append(ID_NEW_POST, "&New Post\tCtrl+N", "Create a new post", ItemKind::Normal)
		.expect("Failed to append new post menu item");
	post_menu
		.append(ID_REPLY, "&Reply\tCtrl+R", "Reply to all mentioned users", ItemKind::Normal)
		.expect("Failed to append reply menu item");
	post_menu
		.append(ID_REPLY_AUTHOR, "Reply to &Author\tCtrl+Shift+R", "Reply to author only", ItemKind::Normal)
		.expect("Failed to append reply author menu item");
	post_menu
		.append(ID_VIEW_MENTIONS, "View &Mentions\tCtrl+M", "View mentions in selected post", ItemKind::Normal)
		.expect("Failed to append view mentions menu item");
	post_menu
		.append(ID_VIEW_HASHTAGS, "View &Hashtags\tCtrl+H", "View hashtags in selected post", ItemKind::Normal)
		.expect("Failed to append view hashtags menu item");
	post_menu
		.append(ID_OPEN_LINKS, "Open &Links\tShift+Enter", "Open links in selected post", ItemKind::Normal)
		.expect("Failed to append open links menu item");
	post_menu
		.append(ID_VIEW_THREAD, "View &Thread\tEnter", "View conversation thread for selected post", ItemKind::Normal)
		.expect("Failed to append view thread menu item");
	post_menu.append_separator();

	post_menu
		.append(ID_EDIT_POST, "&Edit Post\tCtrl+E", "Edit selected post", ItemKind::Normal)
		.expect("Failed to append edit post menu item");
	post_menu
		.append(ID_DELETE_POST, "&Delete Post\tCtrl+Delete", "Delete selected post", ItemKind::Normal)
		.expect("Failed to append delete post menu item");
	post_menu.append_separator();

	post_menu
		.append(crate::ID_VOTE, "&Vote", "Vote on poll in selected post", ItemKind::Normal)
		.expect("Failed to append vote menu item");

	post_menu
		.append(ID_FAVOURITE, "&Favourite\tCtrl+Shift+F", "Favourite or unfavourite selected post", ItemKind::Normal)
		.expect("Failed to append favourite menu item");
	post_menu
		.append(ID_BOOST, "&Boost\tCtrl+Shift+B", "Boost or unboost selected post", ItemKind::Normal)
		.expect("Failed to append boost menu item");
	post_menu.append_separator();

	let timelines_menu = Menu::builder()
		.append_item(ID_VIEW_USER_TIMELINE, "&User Timeline\tCtrl+T", "Open timeline of selected post's author")
		.append_item(ID_OPEN_USER_TIMELINE_BY_INPUT, "Open &User...\tCtrl+U", "Open a user by username")
		.append_item(ID_LOCAL_TIMELINE, "&Local Timeline\tCtrl+L", "Open local timeline")
		.append_item(ID_FEDERATED_TIMELINE, "&Federated Timeline", "Open federated timeline")
		.append_item(crate::ID_BOOKMARKS_TIMELINE, "&Bookmarks", "Open bookmarks timeline")
		.append_item(crate::ID_FAVOURITES_TIMELINE, "F&avourites", "Open favourites timeline")
		.append_separator()
		.append_item(ID_LOAD_MORE, "Load &More\t.", "Load more posts from server")
		.append_separator()
		.append_item(ID_CLOSE_TIMELINE, "&Close Timeline", "Close current timeline")
		.append_separator()
		.append_item(ID_REFRESH, "&Refresh\tF5", "Refresh current timeline")
		.build();
	MenuBar::builder()
		.append(file_menu, "&Options")
		.append(post_menu, "&Post")
		.append(timelines_menu, "&Timelines")
		.build()
}

pub fn update_menu_labels(menu_bar: &MenuBar, state: &AppState) {
	let status = get_selected_status(state);
	let target = status.and_then(|s| s.reblog.as_ref().map(|r| r.as_ref()).or(Some(s)));

	if let Some(fav_item) = menu_bar.find_item(ID_FAVOURITE) {
		let shortcut = if state.config.quick_action_keys { "F" } else { "Ctrl+Shift+F" };
		let label = if target.map(|t| t.favourited).unwrap_or(false) {
			format!("Un&favourite\t{shortcut}")
		} else {
			format!("&Favourite\t{shortcut}")
		};
		fav_item.set_label(&label);
	}

	if let Some(boost_item) = menu_bar.find_item(ID_BOOST) {
		let shortcut = if state.config.quick_action_keys { "B" } else { "Ctrl+Shift+B" };
		let label = if target.map(|t| t.reblogged).unwrap_or(false) {
			format!("Un&boost\t{shortcut}")
		} else {
			format!("&Boost\t{shortcut}")
		};
		boost_item.set_label(&label);
	}

	if let Some(new_post_item) = menu_bar.find_item(ID_NEW_POST) {
		let shortcut = if state.config.quick_action_keys { "C" } else { "Ctrl+N" };
		let label = format!("&New Post\t{shortcut}");
		new_post_item.set_label(&label);
	}

	if let Some(reply_item) = menu_bar.find_item(ID_REPLY) {
		let shortcut = if state.config.quick_action_keys { "R" } else { "Ctrl+R" };
		let label = format!("&Reply\t{shortcut}");
		reply_item.set_label(&label);
	}

	if let Some(reply_author_item) = menu_bar.find_item(ID_REPLY_AUTHOR) {
		let shortcut = if state.config.quick_action_keys { "Ctrl+R" } else { "Ctrl+Shift+R" };
		let label = format!("Reply to &Author\t{shortcut}");
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

	if let Some(edit_post_item) = menu_bar.find_item(ID_EDIT_POST) {
		let shortcut = if state.config.quick_action_keys { "E" } else { "Ctrl+E" };
		let label = format!("&Edit Post\t{shortcut}");
		edit_post_item.set_label(&label);
	}

	if let Some(delete_post_item) = menu_bar.find_item(ID_DELETE_POST) {
		let shortcut = if state.config.quick_action_keys { "D" } else { "Ctrl+Delete" };
		let label = format!("&Delete Post\t{shortcut}");
		delete_post_item.set_label(&label);
	}
}
