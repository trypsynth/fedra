use wx_utils::{menu_label, set_menu_item_label};
use wxdragon::prelude::*;

use crate::{
	AppState, ID_BOOKMARK, ID_BOOKMARKS_TIMELINE, ID_BOOST, ID_CHECK_FOR_UPDATES, ID_CLEAR_ALL_TIMELINES,
	ID_CLEAR_TIMELINE, ID_CLOSE_TIMELINE, ID_COPY_POST, ID_COPY_POST_LINK, ID_CUSTOMIZE_SHORTCUTS, ID_DELETE_POST,
	ID_DIRECT_TIMELINE, ID_EDIT_POST, ID_EDIT_PROFILE, ID_FAVORITE, ID_FAVORITES_TIMELINE, ID_FEDERATED_TIMELINE,
	ID_FIND, ID_FIND_NEXT, ID_FIND_PREV, ID_HOME_TIMELINE, ID_LOAD_MORE, ID_LOCAL_TIMELINE, ID_MANAGE_ACCOUNTS,
	ID_MANAGE_FILTERS, ID_MANAGE_LISTS, ID_MENTIONS_TIMELINE, ID_MESSAGE_REQUESTS, ID_NEW_POST,
	ID_NOTIFICATIONS_TIMELINE, ID_OPEN_INSTANCE_TIMELINE_BY_INPUT, ID_OPEN_LINKS, ID_OPEN_LIST,
	ID_OPEN_USER_TIMELINE_BY_INPUT, ID_OPTIONS, ID_PIN_POST, ID_PLAY_MEDIA, ID_QUOTE, ID_REFRESH, ID_REPLY,
	ID_REPLY_AUTHOR, ID_RESET_TIMELINE_SOUND, ID_SEARCH, ID_SENT_TIMELINE, ID_SET_TIMELINE_SOUND, ID_TOGGLE_FOLLOW,
	ID_TOGGLE_PERMANENT_TIMELINE, ID_TOGGLE_TIMELINE_NOTIFICATIONS, ID_VIEW_BOOSTS, ID_VIEW_FAVORITES,
	ID_VIEW_HASHTAGS, ID_VIEW_HELP, ID_VIEW_IN_BROWSER, ID_VIEW_MENTIONS, ID_VIEW_POST, ID_VIEW_PROFILE,
	ID_VIEW_QUOTED_THREAD, ID_VIEW_THREAD, ID_VIEW_USER_TIMELINE, ID_VOTE,
	config::{ActionId, ShortcutsConfig},
	ui::commands::get_selected_status,
};

struct Item {
	id: i32,
	action: ActionId,
	label: &'static str,
	help: &'static str,
}

const fn item(id: i32, action: ActionId, label: &'static str, help: &'static str) -> Item {
	Item { id, action, label, help }
}

/// Every menu item, so the menu bar, the context menu and menu dispatch can't drift apart.
const ITEMS: &[Item] = &[
	item(ID_MANAGE_ACCOUNTS, ActionId::ManageAccounts, "Manage &Accounts...", "Add, remove or switch accounts"),
	item(ID_MANAGE_FILTERS, ActionId::ManageFilters, "Manage &Filters...", "Manage content filters"),
	item(
		ID_MESSAGE_REQUESTS,
		ActionId::MessageRequests,
		"Message &Requests...",
		"Accept or dismiss messages from people you don't follow",
	),
	item(ID_MANAGE_LISTS, ActionId::ManageLists, "Manage &Lists...", "Create and manage lists"),
	item(ID_EDIT_PROFILE, ActionId::EditProfile, "Edit &Profile...", "Edit current account profile"),
	item(
		ID_CUSTOMIZE_SHORTCUTS,
		ActionId::CustomizeShortcuts,
		"Customize &Keyboard Shortcuts...",
		"Customize application keyboard shortcuts",
	),
	item(ID_OPTIONS, ActionId::Options, "&Options", "Configure application settings"),
	item(ID_NEW_POST, ActionId::NewPost, "&New Post...", "Create a new post"),
	item(ID_REPLY, ActionId::Reply, "&Reply...", "Reply to all mentioned users"),
	item(ID_REPLY_AUTHOR, ActionId::ReplyAuthor, "Reply to &Author...", "Reply to author only"),
	item(ID_QUOTE, ActionId::Quote, "&Quote...", "Quote this post"),
	item(ID_TOGGLE_FOLLOW, ActionId::ToggleFollow, "Toggle &Follow", "Follow or unfollow the author"),
	item(ID_VIEW_PROFILE, ActionId::ViewProfile, "View &Profile", "View profile of selected post's author"),
	item(ID_VIEW_MENTIONS, ActionId::ViewMentions, "View &Mentions", "View mentions in selected post"),
	item(ID_VIEW_HASHTAGS, ActionId::ViewHashtags, "View &Hashtags", "View hashtags in selected post"),
	item(ID_VIEW_BOOSTS, ActionId::ViewBoosts, "View Boo&sts", "View users who boosted this post"),
	item(ID_VIEW_FAVORITES, ActionId::ViewFavorites, "View Fa&vorites", "View users who favorited this post"),
	item(ID_OPEN_LINKS, ActionId::OpenLinks, "Open &Links", "Open links in selected post"),
	item(ID_PLAY_MEDIA, ActionId::PlayMedia, "Play &Media", "Play media attached to selected post"),
	item(ID_VIEW_IN_BROWSER, ActionId::ViewInBrowser, "&Open in Browser", "Open selected post in web browser"),
	item(ID_COPY_POST, ActionId::CopyPost, "&Copy Post", "Copy selected post text"),
	item(ID_COPY_POST_LINK, ActionId::CopyPostLink, "Copy Post &Link", "Copy selected post URL"),
	item(ID_VIEW_POST, ActionId::ViewPost, "View &Post Details", "View post content in a dialog"),
	item(ID_VIEW_THREAD, ActionId::ViewThread, "View &Thread", "View conversation thread for selected post"),
	item(
		ID_VIEW_QUOTED_THREAD,
		ActionId::ViewQuotedThread,
		"View &Quoted Thread",
		"View conversation thread for quoted post",
	),
	item(ID_EDIT_POST, ActionId::EditPost, "&Edit Post...", "Edit selected post"),
	item(ID_DELETE_POST, ActionId::DeletePost, "&Delete Post", "Delete selected post"),
	item(ID_PIN_POST, ActionId::PinPost, "&Pin Post", "Pin or unpin this post on your profile"),
	item(ID_VOTE, ActionId::Vote, "&Vote", "Vote on poll in selected post..."),
	item(ID_FAVORITE, ActionId::Favorite, "&Favorite", "Favorite or unfavorite selected post"),
	item(ID_BOOKMARK, ActionId::Bookmark, "&Bookmark", "Bookmark or unbookmark selected post"),
	item(ID_BOOST, ActionId::Boost, "&Boost", "Boost or unboost selected post"),
	item(
		ID_VIEW_USER_TIMELINE,
		ActionId::OpenUserTimeline,
		"&User Timeline",
		"Open timeline of selected post's author",
	),
	item(ID_OPEN_USER_TIMELINE_BY_INPUT, ActionId::OpenUserTimelineByInput, "Open &User...", "Open a user by username"),
	item(ID_SEARCH, ActionId::Search, "&Search...", "Search for accounts, hashtags, or posts"),
	item(ID_FIND, ActionId::Find, "&Find in Timeline...", "Find text in current timeline"),
	item(ID_FIND_NEXT, ActionId::FindNext, "Find &Next", "Find next occurrence"),
	item(ID_FIND_PREV, ActionId::FindPrev, "Find &Previous", "Find previous occurrence"),
	item(ID_HOME_TIMELINE, ActionId::HomeTimeline, "&Home Timeline", "Open home timeline"),
	item(ID_NOTIFICATIONS_TIMELINE, ActionId::NotificationsTimeline, "&Notifications", "Open notifications timeline"),
	item(ID_SENT_TIMELINE, ActionId::SentTimeline, "Se&nt", "Open a timeline of your own posts"),
	item(ID_LOCAL_TIMELINE, ActionId::LocalTimeline, "&Local Timeline", "Open local timeline"),
	item(
		ID_OPEN_INSTANCE_TIMELINE_BY_INPUT,
		ActionId::OpenInstanceTimelineByInput,
		"Open &Instance Timeline...",
		"Open an instance's local timeline by domain",
	),
	item(ID_FEDERATED_TIMELINE, ActionId::FederatedTimeline, "&Federated Timeline", "Open federated timeline"),
	item(ID_DIRECT_TIMELINE, ActionId::DirectTimeline, "&Direct Messages", "Open direct messages timeline"),
	item(ID_MENTIONS_TIMELINE, ActionId::MentionsTimeline, "&Mentions", "Open mentions timeline"),
	item(ID_BOOKMARKS_TIMELINE, ActionId::BookmarksTimeline, "&Bookmarks", "Open bookmarks timeline"),
	item(ID_FAVORITES_TIMELINE, ActionId::FavoritesTimeline, "F&avorites", "Open favorites timeline"),
	item(ID_OPEN_LIST, ActionId::OpenList, "Open &List...", "Open a Mastodon list"),
	item(ID_LOAD_MORE, ActionId::LoadMore, "Load &More", "Load more posts from server"),
	item(ID_CLOSE_TIMELINE, ActionId::CloseTimeline, "&Close Timeline", "Close current timeline"),
	item(
		ID_TOGGLE_PERMANENT_TIMELINE,
		ActionId::TogglePermanentTimeline,
		"Make Timeline &Permanent or Closable",
		"Stop or allow closing the current timeline",
	),
	item(
		ID_TOGGLE_TIMELINE_NOTIFICATIONS,
		ActionId::ToggleTimelineNotifications,
		"Turn New Post &Notifications On or Off",
		"Get notified of new posts in the current timeline",
	),
	item(
		ID_SET_TIMELINE_SOUND,
		ActionId::SetTimelineSound,
		"Set Notification S&ound...",
		"Pick the sound this timeline's notifications play",
	),
	item(
		ID_RESET_TIMELINE_SOUND,
		ActionId::ResetTimelineSound,
		"Use the Default Notification Sound",
		"Go back to the usual notification sound",
	),
	item(ID_CLEAR_TIMELINE, ActionId::ClearTimeline, "Cl&ear Timeline", "Empty the current timeline"),
	item(ID_CLEAR_ALL_TIMELINES, ActionId::ClearAllTimelines, "Clear All &Timelines", "Empty every open timeline"),
	item(ID_REFRESH, ActionId::Refresh, "&Refresh", "Refresh current timeline"),
	item(ID_CHECK_FOR_UPDATES, ActionId::CheckForUpdates, "Check for &Updates...", "Check for application updates"),
	item(ID_VIEW_HELP, ActionId::ViewHelp, "View &Help", "Open documentation"),
];

/// Stands for a separator in a menu layout; no item uses id 0.
const SEPARATOR: i32 = 0;

const OPTIONS_MENU: &[i32] = &[
	ID_MANAGE_ACCOUNTS,
	ID_MANAGE_FILTERS,
	ID_MESSAGE_REQUESTS,
	ID_MANAGE_LISTS,
	ID_EDIT_PROFILE,
	ID_CUSTOMIZE_SHORTCUTS,
	SEPARATOR,
	ID_OPTIONS,
];

const POST_MENU: &[i32] = &[
	ID_NEW_POST,
	ID_REPLY,
	ID_REPLY_AUTHOR,
	ID_QUOTE,
	ID_TOGGLE_FOLLOW,
	ID_VIEW_PROFILE,
	ID_VIEW_MENTIONS,
	ID_VIEW_HASHTAGS,
	ID_VIEW_BOOSTS,
	ID_VIEW_FAVORITES,
	ID_OPEN_LINKS,
	ID_PLAY_MEDIA,
	ID_VIEW_IN_BROWSER,
	ID_COPY_POST,
	ID_COPY_POST_LINK,
	ID_VIEW_POST,
	ID_VIEW_THREAD,
	ID_VIEW_QUOTED_THREAD,
	SEPARATOR,
	ID_EDIT_POST,
	ID_DELETE_POST,
	ID_PIN_POST,
	SEPARATOR,
	ID_VOTE,
	ID_FAVORITE,
	ID_BOOKMARK,
	ID_BOOST,
];

const TIMELINES_MENU: &[i32] = &[
	ID_VIEW_USER_TIMELINE,
	ID_OPEN_USER_TIMELINE_BY_INPUT,
	ID_SEARCH,
	SEPARATOR,
	ID_FIND,
	ID_FIND_NEXT,
	ID_FIND_PREV,
	SEPARATOR,
	ID_HOME_TIMELINE,
	ID_NOTIFICATIONS_TIMELINE,
	ID_SENT_TIMELINE,
	ID_LOCAL_TIMELINE,
	ID_OPEN_INSTANCE_TIMELINE_BY_INPUT,
	ID_FEDERATED_TIMELINE,
	ID_DIRECT_TIMELINE,
	ID_MENTIONS_TIMELINE,
	ID_BOOKMARKS_TIMELINE,
	ID_FAVORITES_TIMELINE,
	ID_OPEN_LIST,
	SEPARATOR,
	ID_LOAD_MORE,
	SEPARATOR,
	ID_CLOSE_TIMELINE,
	ID_TOGGLE_PERMANENT_TIMELINE,
	ID_TOGGLE_TIMELINE_NOTIFICATIONS,
	ID_SET_TIMELINE_SOUND,
	ID_RESET_TIMELINE_SOUND,
	ID_CLEAR_TIMELINE,
	ID_CLEAR_ALL_TIMELINES,
	SEPARATOR,
	ID_REFRESH,
];

const HELP_MENU: &[i32] = &[ID_CHECK_FOR_UPDATES, ID_VIEW_HELP];

const CONTEXT_MENU: &[i32] = &[
	ID_REPLY,
	ID_REPLY_AUTHOR,
	ID_QUOTE,
	SEPARATOR,
	ID_FAVORITE,
	ID_BOOKMARK,
	ID_BOOST,
	SEPARATOR,
	ID_VIEW_POST,
	ID_VIEW_THREAD,
	ID_VIEW_QUOTED_THREAD,
	ID_OPEN_LINKS,
	ID_PLAY_MEDIA,
	ID_VIEW_IN_BROWSER,
	ID_COPY_POST,
	ID_COPY_POST_LINK,
	SEPARATOR,
	ID_VIEW_PROFILE,
	ID_VIEW_USER_TIMELINE,
	ID_VIEW_MENTIONS,
	ID_VIEW_HASHTAGS,
	ID_VIEW_BOOSTS,
	ID_VIEW_FAVORITES,
	SEPARATOR,
	ID_EDIT_POST,
	ID_DELETE_POST,
	ID_PIN_POST,
];

const POST_MENU_INDEX: usize = 1;

/// Everything about the selected post that the post menus show, so they're only rebuilt when it changes.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools, reason = "each flag shows, hides or relabels one menu item")]
pub struct PostMenuState {
	favourited: bool,
	reblogged: bool,
	bookmarked: bool,
	pinned: bool,
	is_direct: bool,
	is_own: bool,
	has_boosts: bool,
	has_favorites: bool,
	has_poll: bool,
	has_post: bool,
	has_link: bool,
	quick_action_keys: bool,
}

impl PostMenuState {
	fn of(state: &AppState) -> Self {
		let target = get_selected_status(state).map(|s| s.reblog.as_deref().unwrap_or(s));
		Self {
			favourited: target.is_some_and(|t| t.favourited),
			reblogged: target.is_some_and(|t| t.reblogged),
			bookmarked: target.is_some_and(|t| t.bookmarked),
			pinned: target.is_some_and(|t| t.pinned),
			is_direct: target.is_some_and(|t| t.visibility == "direct"),
			is_own: target.is_some_and(|t| Some(&t.account.id) == state.current_user_id.as_ref()),
			has_boosts: target.is_some_and(|t| t.reblogs_count > 0),
			has_favorites: target.is_some_and(|t| t.favourites_count > 0),
			has_poll: target.is_some_and(|t| t.poll.is_some()),
			has_post: target.is_some(),
			has_link: target.is_some_and(|t| t.url.is_some()),
			quick_action_keys: state.config.quick_action_keys,
		}
	}

	const fn shows(self, id: i32) -> bool {
		match id {
			ID_BOOST => !self.is_direct,
			ID_VIEW_BOOSTS => self.has_boosts,
			ID_VIEW_FAVORITES => self.has_favorites,
			ID_EDIT_POST | ID_DELETE_POST | ID_PIN_POST => self.is_own,
			ID_VOTE => self.has_poll,
			_ => true,
		}
	}

	const fn enables(self, id: i32) -> bool {
		match id {
			ID_COPY_POST => self.has_post,
			ID_COPY_POST_LINK => self.has_link,
			_ => true,
		}
	}

	const fn label(self, item: &Item) -> &'static str {
		match item.id {
			ID_FAVORITE if self.favourited => "Un&favorite",
			ID_BOOKMARK if self.bookmarked => "Un&bookmark",
			ID_BOOST if self.reblogged => "Un&boost",
			ID_PIN_POST if self.pinned => "&Unpin Post",
			_ => item.label,
		}
	}
}

fn find_item(id: i32) -> &'static Item {
	ITEMS.iter().find(|item| item.id == id).expect("every menu layout id is in ITEMS")
}

/// The shortcut action a menu item runs.
pub fn action_for_id(id: i32) -> Option<ActionId> {
	ITEMS.iter().find(|item| item.id == id).map(|item| item.action)
}

/// Appends `layout`'s items to `menu`, leaving out hidden ones and the separators they'd strand.
fn fill_menu(menu: &Menu, layout: &[i32], post: PostMenuState, shortcuts: &ShortcutsConfig) {
	let mut separator_due = false;
	for &id in layout {
		if id == SEPARATOR {
			separator_due = menu.get_item_count() > 0;
			continue;
		}
		if !post.shows(id) {
			continue;
		}
		if separator_due {
			menu.append_separator();
			separator_due = false;
		}
		let item = find_item(id);
		let shortcut = shortcuts.get_menu_str(post.quick_action_keys, item.action);
		menu.append(id, &menu_label(post.label(item), &shortcut), item.help, ItemKind::Normal);
		if !post.enables(id) {
			menu.enable_item(id, false);
		}
	}
}

fn build_menu(layout: &[i32], post: PostMenuState, shortcuts: &ShortcutsConfig) -> Menu {
	let menu = Menu::builder().build();
	fill_menu(&menu, layout, post, shortcuts);
	menu
}

/// Built with the default shortcuts; [`relabel_menus`] applies the user's once the config is loaded.
pub fn build_menu_bar() -> MenuBar {
	let post = PostMenuState::default();
	let shortcuts = ShortcutsConfig::default();
	MenuBar::builder()
		.append(build_menu(OPTIONS_MENU, post, &shortcuts), "&Options")
		.append(build_menu(POST_MENU, post, &shortcuts), "&Post")
		.append(build_menu(TIMELINES_MENU, post, &shortcuts), "&Timelines")
		.append(build_menu(HELP_MENU, post, &shortcuts), "&Help")
		.build()
}

pub fn build_context_menu(post: PostMenuState, shortcuts: &ShortcutsConfig) -> Menu {
	build_menu(CONTEXT_MENU, post, shortcuts)
}

fn rebuild_post_menu(menu_bar: &MenuBar, post: PostMenuState, shortcuts: &ShortcutsConfig) {
	let Some(menu) = menu_bar.get_menu(POST_MENU_INDEX) else { return };
	// Emptied and refilled rather than replaced, since it may be open and a replaced menu is destroyed.
	while let Some(item) = menu.find_item_by_position(0) {
		menu.delete_item(&item);
	}
	fill_menu(&menu, POST_MENU, post, shortcuts);
}

/// Brings the menus in line with the selected post, the active timeline and pending message requests.
pub fn update_menu_labels(menu_bar: &MenuBar, state: &AppState) {
	let post = PostMenuState::of(state);
	if state.post_menu_state.replace(post) != post {
		rebuild_post_menu(menu_bar, post, &state.config.shortcuts);
	}
	let supports_paging = state.timeline_manager.active().is_some_and(|t| t.timeline_type.supports_paging());
	menu_bar.enable_item(ID_LOAD_MORE, supports_paging);
	let message_requests = match state.pending_message_requests {
		Some(count) if count > 0 => format!("Message &Requests ({count})..."),
		_ => "Message &Requests...".to_string(),
	};
	let shortcut = state.config.shortcuts.get_menu_str(post.quick_action_keys, ActionId::MessageRequests);
	set_menu_item_label(menu_bar, ID_MESSAGE_REQUESTS, &message_requests, &shortcut);
}

/// Relabels every item with the current shortcuts, after they or quick action mode change.
pub fn relabel_menus(menu_bar: &MenuBar, state: &AppState) {
	let post = PostMenuState::of(state);
	let shortcuts = &state.config.shortcuts;
	for item in ITEMS.iter().filter(|item| !POST_MENU.contains(&item.id)) {
		set_menu_item_label(
			menu_bar,
			item.id,
			item.label,
			&shortcuts.get_menu_str(post.quick_action_keys, item.action),
		);
	}
	state.post_menu_state.set(post);
	rebuild_post_menu(menu_bar, post, shortcuts);
	update_menu_labels(menu_bar, state);
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn every_item_is_in_the_menu_bar_once_and_every_layout_id_is_an_item() {
		let bar = [OPTIONS_MENU, POST_MENU, TIMELINES_MENU, HELP_MENU].concat();
		for item in ITEMS {
			assert_eq!(bar.iter().filter(|&&id| id == item.id).count(), 1, "{}", item.label);
		}
		for &id in bar.iter().chain(CONTEXT_MENU).filter(|&&id| id != SEPARATOR) {
			assert_eq!(ITEMS.iter().filter(|item| item.id == id).count(), 1, "menu id {id}");
		}
	}
}
