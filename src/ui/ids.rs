/// Generates sequential i32 constants starting from a base value.
macro_rules! define_ids {
	(base = $base:expr; $($name:ident),+ $(,)?) => {
		define_ids!(@step $base; $($name),+);
	};
	(@step $n:expr; $name:ident) => {
		pub const $name: i32 = $n;
	};
	(@step $n:expr; $name:ident, $($rest:ident),+) => {
		pub const $name: i32 = $n;
		define_ids!(@step $n + 1; $($rest),+);
	};
}

define_ids! {
	base = 1001;
	// Post actions
	ID_NEW_POST,
	ID_REPLY,
	ID_REPLY_AUTHOR,
	ID_FAVORITE,
	ID_BOOKMARK,
	ID_BOOST,
	ID_DELETE_POST,
	ID_EDIT_POST,
	ID_VOTE,
	// Post navigation
	ID_VIEW_THREAD,
	ID_OPEN_LINKS,
	ID_VIEW_IN_BROWSER,
	ID_VIEW_MENTIONS,
	ID_VIEW_HASHTAGS,
	ID_VIEW_BOOSTS,
	ID_VIEW_FAVORITES,
	ID_COPY_POST,
	ID_VIEW_POST,
	// User actions
	ID_VIEW_PROFILE,
	ID_VIEW_USER_TIMELINE,
	ID_OPEN_USER_TIMELINE_BY_INPUT,
	// Timeline actions
	ID_LOCAL_TIMELINE,
	ID_FEDERATED_TIMELINE,
	ID_DIRECT_TIMELINE,
	ID_BOOKMARKS_TIMELINE,
	ID_FAVORITES_TIMELINE,
	ID_CLOSE_TIMELINE,
	ID_REFRESH,
	ID_LOAD_MORE,
	// Account/settings
	ID_OPTIONS,
	ID_MANAGE_ACCOUNTS,
	ID_EDIT_PROFILE,
	// System tray
	ID_TRAY_TOGGLE,
	ID_TRAY_EXIT,
	// Help
	ID_VIEW_HELP,
	ID_CHECK_FOR_UPDATES,
	ID_SEARCH,
	// Internal
	ID_UI_WAKE,
}

// Key codes
pub const KEY_DELETE: i32 = 127;
