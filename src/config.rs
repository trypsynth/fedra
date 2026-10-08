use std::{
	collections::HashMap,
	fs, io,
	path::{Path, PathBuf},
	sync::LazyLock,
	time::{SystemTime, UNIX_EPOCH},
};

use anyhow::Result;
use homeport::Homeport;
pub use key_chord::KeyChord;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use url::Url;

use crate::template::{
	DEFAULT_BOOST_TEMPLATE, DEFAULT_FAVORITE_TEMPLATE, DEFAULT_POST_TEMPLATE, DEFAULT_QUOTE_TEMPLATE,
};

const APP_NAME: &str = "Fedra";
const CONFIG_FILENAME: &str = "config.json";
const CONFIG_VERSION: u32 = 1;

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
	pub version: u32,
	pub accounts: Vec<Account>,
	pub active_account_id: Option<String>,
	#[serde(default = "default_enter_to_send")]
	pub enter_to_send: bool,
	#[serde(default = "default_always_show_link_dialog")]
	pub always_show_link_dialog: bool,
	#[serde(default = "default_show_link_previews")]
	pub show_link_previews: bool,
	#[serde(default = "default_quick_action_keys")]
	pub quick_action_keys: bool,
	#[serde(default, deserialize_with = "deserialize_autoload_mode")]
	pub autoload: AutoloadMode,
	#[serde(default = "default_fetch_limit")]
	pub fetch_limit: u8,
	#[serde(default = "default_streaming")]
	pub streaming: bool,
	/// How often timelines that aren't streaming are checked for new posts.
	#[serde(default = "default_refresh_minutes")]
	pub refresh_minutes: u8,
	#[serde(default)]
	pub sort_order: SortOrder,
	#[serde(default)]
	pub content_warning_display: ContentWarningDisplay,
	#[serde(default)]
	pub display_name_emoji_mode: DisplayNameEmojiMode,
	/// Names chosen for people, keyed by their lowercased `user@instance` handle.
	#[serde(default)]
	pub user_aliases: std::collections::BTreeMap<String, String>,
	#[serde(default = "default_preserve_thread_order")]
	pub preserve_thread_order: bool,
	#[serde(default = "default_timelines")]
	pub default_timelines: Vec<DefaultTimeline>,
	#[serde(default)]
	pub notification_preference: NotificationPreference,
	#[serde(default)]
	pub disabled_notification_types: Vec<NotificationKind>,
	/// Types that still show in timelines but never make a sound or notification.
	#[serde(default)]
	pub silent_notification_types: Vec<NotificationKind>,
	#[serde(default = "default_check_for_updates")]
	pub check_for_updates_on_startup: bool,
	#[serde(default)]
	pub start_maximized: bool,
	#[serde(default)]
	pub update_channel: UpdateChannel,
	/// The old single show/hide hotkey, from before it became the `ToggleWindow` global
	/// shortcut. Only read so `ConfigStore::load` can carry a customized one over.
	#[serde(default, rename = "hotkey", skip_serializing)]
	legacy_hotkey: Option<HotkeyConfig>,
	/// Whether the global shortcuts other than showing and hiding the window are registered.
	#[serde(default)]
	pub global_keys: bool,
	/// Whether moving past the first or last post with a global shortcut reads that post again.
	#[serde(default = "default_true")]
	pub repeat_at_timeline_edges: bool,
	#[serde(default = "default_strip_tracking")]
	pub strip_tracking: bool,
	#[serde(default)]
	pub templates: PostTemplates,
	#[serde(default)]
	pub filters: TimelineFilters,
	#[serde(default)]
	pub find_loading_mode: FindLoadingMode,
	#[serde(default = "default_window_title_template")]
	pub window_title_template: String,
	#[serde(default = "default_restore_open_timelines")]
	pub restore_open_timelines: bool,
	#[serde(default)]
	pub sync_read_position: bool,
	#[serde(default)]
	pub load_older_to_restore: bool,
	#[serde(default)]
	pub shortcuts: ShortcutsConfig,
	// Open timelines used to be saved once for the whole app rather than per
	// account. These are only read so `ConfigStore::load` can move them onto
	// the active account; they're never written back.
	#[serde(default, rename = "saved_timelines", skip_serializing)]
	legacy_saved_timelines: Vec<crate::timeline::TimelineType>,
	#[serde(default, rename = "saved_active_timeline", skip_serializing)]
	legacy_saved_active_timeline: Option<crate::timeline::TimelineType>,
	#[serde(default, rename = "saved_selected_post_id", skip_serializing)]
	legacy_saved_selected_post_id: Option<String>,
	#[serde(default)]
	pub saved_window_hidden: bool,
}

impl Config {
	/// Returns whether notifications of the given Mastodon API `type` should be surfaced
	/// (sound/toast and the notification timeline). Unrecognized kinds are always enabled.
	pub fn notification_kind_enabled(&self, kind: &str) -> bool {
		NotificationKind::from_api_kind(kind).is_none_or(|k| !self.disabled_notification_types.contains(&k))
	}

	pub fn notification_kind_alerts(&self, kind: &str) -> bool {
		self.notification_kind_enabled(kind)
			&& NotificationKind::from_api_kind(kind).is_none_or(|k| !self.silent_notification_types.contains(&k))
	}
}

const fn default_restore_open_timelines() -> bool {
	false
}

fn default_window_title_template() -> String {
	crate::template::DEFAULT_WINDOW_TITLE_TEMPLATE.to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum FindLoadingMode {
	#[default]
	None,
	LoadOnNext,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum UpdateChannel {
	#[default]
	Stable,
	Dev,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum NotificationPreference {
	#[default]
	Classic,
	SoundOnly,
	Disabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NotificationKind {
	Mention,
	Boost,
	Favorite,
	Follow,
	FollowRequest,
	Poll,
	Update,
	Status,
	Admin,
}

impl NotificationKind {
	pub const fn all() -> &'static [Self] {
		&[
			Self::Mention,
			Self::Boost,
			Self::Favorite,
			Self::Follow,
			Self::FollowRequest,
			Self::Poll,
			Self::Update,
			Self::Status,
			Self::Admin,
		]
	}

	pub const fn display_name(self) -> &'static str {
		match self {
			Self::Mention => "Mentions",
			Self::Boost => "Boosts",
			Self::Favorite => "Favorites",
			Self::Follow => "New followers",
			Self::FollowRequest => "Follow requests",
			Self::Poll => "Poll results",
			Self::Update => "Edited posts",
			Self::Status => "New posts",
			Self::Admin => "Moderation and admin",
		}
	}

	/// Maps a Mastodon notification `type` string to the coarser kind used for filtering.
	/// Unrecognized kinds return `None` so they are never accidentally hidden.
	pub fn from_api_kind(kind: &str) -> Option<Self> {
		match kind {
			"mention" => Some(Self::Mention),
			"reblog" => Some(Self::Boost),
			"favourite" => Some(Self::Favorite),
			"follow" => Some(Self::Follow),
			"follow_request" => Some(Self::FollowRequest),
			"poll" => Some(Self::Poll),
			"update" => Some(Self::Update),
			"status" => Some(Self::Status),
			"admin.sign_up" | "admin.report" | "severed_relationships" | "moderation_warning" => Some(Self::Admin),
			_ => None,
		}
	}
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HotkeyConfig {
	pub ctrl: bool,
	pub alt: bool,
	pub shift: bool,
	pub win: bool,
	pub key: char,
}

impl Default for HotkeyConfig {
	fn default() -> Self {
		Self { ctrl: true, alt: true, shift: false, win: false, key: 'F' }
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionId {
	NewPost,
	Reply,
	ReplyAuthor,
	Quote,
	ToggleFollow,
	ViewProfile,
	ViewMentions,
	ViewHashtags,
	OpenLinks,
	PlayMedia,
	ViewInBrowser,
	CopyPost,
	CopyPostLink,
	ViewPost,
	ViewThread,
	ViewQuotedThread,
	EditPost,
	DeletePost,
	PinPost,
	Vote,
	Favorite,
	Bookmark,
	Boost,
	ViewBoosts,
	ViewFavorites,
	OpenUserTimeline,
	OpenUserTimelineByInput,
	Search,
	Find,
	FindNext,
	FindPrev,
	HomeTimeline,
	NotificationsTimeline,
	SentTimeline,
	LocalTimeline,
	OpenInstanceTimelineByInput,
	FederatedTimeline,
	DirectTimeline,
	MentionsTimeline,
	BookmarksTimeline,
	FavoritesTimeline,
	OpenList,
	LoadMore,
	CloseTimeline,
	TogglePermanentTimeline,
	ToggleTimelineNotifications,
	SetTimelineSound,
	ResetTimelineSound,
	ClearTimeline,
	ClearAllTimelines,
	Refresh,
	SwitchPrevTimeline,
	SwitchNextTimeline,
	MoveTimelineLeft,
	MoveTimelineRight,
	SwitchPrevAccount,
	SwitchNextAccount,
	ToggleContentWarning,
	ToggleQuickActionKeys,
	ManageAccounts,
	ManageFilters,
	MessageRequests,
	ManageLists,
	EditProfile,
	Options,
	CustomizeShortcuts,
	CheckForUpdates,
	ViewHelp,
}

impl ActionId {
	pub const fn all() -> &'static [Self] {
		&[
			Self::NewPost,
			Self::Reply,
			Self::ReplyAuthor,
			Self::Quote,
			Self::ToggleFollow,
			Self::ViewProfile,
			Self::ViewMentions,
			Self::ViewHashtags,
			Self::OpenLinks,
			Self::PlayMedia,
			Self::ViewInBrowser,
			Self::CopyPost,
			Self::CopyPostLink,
			Self::ViewPost,
			Self::ViewThread,
			Self::ViewQuotedThread,
			Self::EditPost,
			Self::DeletePost,
			Self::PinPost,
			Self::Vote,
			Self::Favorite,
			Self::Bookmark,
			Self::Boost,
			Self::ViewBoosts,
			Self::ViewFavorites,
			Self::OpenUserTimeline,
			Self::OpenUserTimelineByInput,
			Self::Search,
			Self::Find,
			Self::FindNext,
			Self::FindPrev,
			Self::HomeTimeline,
			Self::NotificationsTimeline,
			Self::SentTimeline,
			Self::LocalTimeline,
			Self::OpenInstanceTimelineByInput,
			Self::FederatedTimeline,
			Self::DirectTimeline,
			Self::MentionsTimeline,
			Self::BookmarksTimeline,
			Self::FavoritesTimeline,
			Self::OpenList,
			Self::LoadMore,
			Self::CloseTimeline,
			Self::TogglePermanentTimeline,
			Self::ToggleTimelineNotifications,
			Self::SetTimelineSound,
			Self::ResetTimelineSound,
			Self::ClearTimeline,
			Self::ClearAllTimelines,
			Self::Refresh,
			Self::SwitchPrevTimeline,
			Self::SwitchNextTimeline,
			Self::MoveTimelineLeft,
			Self::MoveTimelineRight,
			Self::SwitchPrevAccount,
			Self::SwitchNextAccount,
			Self::ToggleContentWarning,
			Self::ToggleQuickActionKeys,
			Self::ManageAccounts,
			Self::ManageFilters,
			Self::MessageRequests,
			Self::ManageLists,
			Self::EditProfile,
			Self::Options,
			Self::CustomizeShortcuts,
			Self::CheckForUpdates,
			Self::ViewHelp,
		]
	}

	pub const fn display_name(self) -> &'static str {
		match self {
			Self::NewPost => "New Post...",
			Self::Reply => "Reply...",
			Self::ReplyAuthor => "Reply to Author...",
			Self::Quote => "Quote Post...",
			Self::ToggleFollow => "Toggle Follow",
			Self::ViewProfile => "View Author Profile",
			Self::ViewMentions => "View Mentions",
			Self::ViewHashtags => "View Hashtags",
			Self::OpenLinks => "Open Links",
			Self::PlayMedia => "Play Media",
			Self::ViewInBrowser => "Open in Browser",
			Self::CopyPost => "Copy Post",
			Self::CopyPostLink => "Copy Post Link",
			Self::ViewPost => "View Post Details",
			Self::ViewThread => "View Thread",
			Self::ViewQuotedThread => "View Quoted Thread",
			Self::EditPost => "Edit Post...",
			Self::DeletePost => "Delete Post",
			Self::PinPost => "Pin / Unpin Post",
			Self::Vote => "Vote on Poll...",
			Self::Favorite => "Favorite",
			Self::Bookmark => "Bookmark",
			Self::Boost => "Boost",
			Self::ViewBoosts => "View Boosts",
			Self::ViewFavorites => "View Favorites",
			Self::OpenUserTimeline => "Open User Timeline",
			Self::OpenUserTimelineByInput => "Open User...",
			Self::Search => "Search...",
			Self::Find => "Find in Timeline...",
			Self::FindNext => "Find Next",
			Self::FindPrev => "Find Previous",
			Self::HomeTimeline => "Home Timeline",
			Self::NotificationsTimeline => "Notifications Timeline",
			Self::SentTimeline => "Sent Timeline",
			Self::LocalTimeline => "Local Timeline",
			Self::OpenInstanceTimelineByInput => "Open Instance Timeline...",
			Self::FederatedTimeline => "Federated Timeline",
			Self::DirectTimeline => "Direct Messages",
			Self::MentionsTimeline => "Mentions Timeline",
			Self::BookmarksTimeline => "Bookmarks",
			Self::FavoritesTimeline => "Favorites",
			Self::OpenList => "Open List...",
			Self::LoadMore => "Load More",
			Self::CloseTimeline => "Close Timeline",
			Self::TogglePermanentTimeline => "Make Timeline Permanent or Closable",
			Self::ToggleTimelineNotifications => "Turn New Post Notifications On or Off",
			Self::SetTimelineSound => "Set Notification Sound...",
			Self::ResetTimelineSound => "Use the Default Notification Sound",
			Self::ClearTimeline => "Clear Timeline",
			Self::ClearAllTimelines => "Clear All Timelines",
			Self::Refresh => "Refresh",
			Self::SwitchPrevTimeline => "Previous Timeline",
			Self::SwitchNextTimeline => "Next Timeline",
			Self::MoveTimelineLeft => "Move Timeline Left",
			Self::MoveTimelineRight => "Move Timeline Right",
			Self::SwitchPrevAccount => "Previous Account",
			Self::SwitchNextAccount => "Next Account",
			Self::ToggleContentWarning => "Toggle Content Warning",
			Self::ToggleQuickActionKeys => "Toggle Quick Keys Mode",
			Self::ManageAccounts => "Manage Accounts...",
			Self::ManageFilters => "Manage Filters...",
			Self::MessageRequests => "Message Requests...",
			Self::ManageLists => "Manage Lists...",
			Self::EditProfile => "Edit Profile...",
			Self::Options => "Options...",
			Self::CustomizeShortcuts => "Customize Keyboard Shortcuts...",
			Self::CheckForUpdates => "Check for Updates...",
			Self::ViewHelp => "View Help",
		}
	}

	#[allow(clippy::match_same_arms, reason = "one arm per action keeps the default table readable")]
	pub fn default_chord(self, quick: bool) -> Option<KeyChord> {
		if quick {
			match self {
				Self::NewPost => Some(KeyChord::new(false, false, false, "C")),
				Self::Reply => Some(KeyChord::new(false, false, false, "R")),
				Self::ReplyAuthor => Some(KeyChord::new(true, false, false, "R")),
				Self::Quote => Some(KeyChord::new(false, false, false, "Q")),
				Self::ToggleFollow => Some(KeyChord::new(false, true, false, "F")),
				Self::ViewProfile => Some(KeyChord::new(false, false, false, "P")),
				Self::ViewMentions => Some(KeyChord::new(false, false, false, "M")),
				Self::ViewHashtags => Some(KeyChord::new(false, false, false, "H")),
				Self::OpenLinks => Some(KeyChord::new(false, false, false, "Enter")),
				Self::PlayMedia => Some(KeyChord::new(false, false, false, "I")),
				Self::ViewInBrowser => Some(KeyChord::new(false, false, false, "O")),
				Self::CopyPost => Some(KeyChord::new(true, false, true, "C")),
				Self::CopyPostLink => Some(KeyChord::new(true, false, false, "C")),
				Self::ViewPost => Some(KeyChord::new(false, false, true, "Enter")),
				Self::ViewThread => Some(KeyChord::new(false, true, false, "Enter")),
				Self::ViewQuotedThread => None,
				Self::EditPost => Some(KeyChord::new(false, false, false, "E")),
				Self::DeletePost => Some(KeyChord::new(false, false, false, "Delete")),
				Self::PinPost => None,
				Self::Vote => Some(KeyChord::new(false, false, false, "V")),
				Self::Favorite => Some(KeyChord::new(false, false, false, "F")),
				Self::Bookmark => Some(KeyChord::new(false, false, false, "K")),
				Self::Boost => Some(KeyChord::new(false, false, false, "B")),
				Self::ViewBoosts => None,
				Self::ViewFavorites => None,
				Self::OpenUserTimeline => Some(KeyChord::new(false, false, false, "T")),
				Self::OpenUserTimelineByInput => Some(KeyChord::new(false, false, false, "U")),
				Self::Search => Some(KeyChord::new(false, false, false, "/")),
				Self::Find => Some(KeyChord::new(true, false, false, "F")),
				Self::FindNext => Some(KeyChord::new(false, false, false, "F3")),
				Self::FindPrev => Some(KeyChord::new(false, false, true, "F3")),
				Self::HomeTimeline => None,
				Self::NotificationsTimeline => None,
				Self::SentTimeline => None,
				Self::LocalTimeline => Some(KeyChord::new(true, false, false, "L")),
				Self::OpenInstanceTimelineByInput => Some(KeyChord::new(false, false, true, "I")),
				Self::FederatedTimeline => None,
				Self::DirectTimeline => Some(KeyChord::new(true, false, false, "D")),
				Self::MentionsTimeline => Some(KeyChord::new(true, false, true, "M")),
				Self::BookmarksTimeline => None,
				Self::FavoritesTimeline => None,
				Self::OpenList => None,
				Self::LoadMore => Some(KeyChord::new(false, false, false, ".")),
				Self::CloseTimeline => Some(KeyChord::new(false, false, false, "Backspace")),
				Self::TogglePermanentTimeline => Some(KeyChord::new(true, false, false, "P")),
				Self::ToggleTimelineNotifications => Some(KeyChord::new(true, false, false, "N")),
				Self::SetTimelineSound | Self::ResetTimelineSound => None,
				Self::ClearTimeline => Some(KeyChord::new(true, false, false, "Delete")),
				Self::ClearAllTimelines => Some(KeyChord::new(true, false, true, "Delete")),
				Self::Refresh => Some(KeyChord::new(false, false, false, "F5")),
				Self::SwitchPrevTimeline => Some(KeyChord::new(false, false, false, "Left")),
				Self::SwitchNextTimeline => Some(KeyChord::new(false, false, false, "Right")),
				Self::MoveTimelineLeft => Some(KeyChord::new(false, false, true, "Left")),
				Self::MoveTimelineRight => Some(KeyChord::new(false, false, true, "Right")),
				Self::SwitchPrevAccount => Some(KeyChord::new(true, false, false, "[")),
				Self::SwitchNextAccount => Some(KeyChord::new(true, false, false, "]")),
				Self::ToggleContentWarning => Some(KeyChord::new(false, false, false, "X")),
				Self::ToggleQuickActionKeys => Some(KeyChord::new(true, false, true, "Q")),
				Self::ManageAccounts => Some(KeyChord::new(true, true, false, "A")),
				Self::ManageFilters => None,
				Self::MessageRequests => None,
				Self::ManageLists => None,
				Self::EditProfile => Some(KeyChord::new(true, false, true, "E")),
				Self::Options => Some(KeyChord::new(true, false, false, ",")),
				Self::CustomizeShortcuts => None,
				Self::CheckForUpdates => None,
				Self::ViewHelp => Some(KeyChord::new(false, false, false, "F1")),
			}
		} else {
			match self {
				Self::NewPost => Some(KeyChord::new(true, false, false, "N")),
				Self::Reply => Some(KeyChord::new(true, false, false, "R")),
				Self::ReplyAuthor => Some(KeyChord::new(true, false, true, "R")),
				Self::Quote => Some(KeyChord::new(true, false, false, "Q")),
				Self::ToggleFollow => Some(KeyChord::new(false, true, false, "F")),
				Self::ViewProfile => Some(KeyChord::new(true, false, false, "P")),
				Self::ViewMentions => Some(KeyChord::new(true, false, false, "M")),
				Self::ViewHashtags => Some(KeyChord::new(true, false, false, "H")),
				Self::OpenLinks => Some(KeyChord::new(false, false, false, "Enter")),
				Self::PlayMedia => Some(KeyChord::new(true, false, false, "I")),
				Self::ViewInBrowser => Some(KeyChord::new(true, false, true, "O")),
				Self::CopyPost => Some(KeyChord::new(true, false, true, "C")),
				Self::CopyPostLink => Some(KeyChord::new(true, false, false, "C")),
				Self::ViewPost => Some(KeyChord::new(false, false, true, "Enter")),
				Self::ViewThread => Some(KeyChord::new(false, true, false, "Enter")),
				Self::ViewQuotedThread => None,
				Self::EditPost => Some(KeyChord::new(true, false, false, "E")),
				Self::DeletePost => Some(KeyChord::new(false, false, false, "Delete")),
				Self::PinPost => None,
				Self::Vote => Some(KeyChord::new(true, false, false, "V")),
				Self::Favorite => Some(KeyChord::new(true, false, true, "F")),
				Self::Bookmark => Some(KeyChord::new(true, false, true, "K")),
				Self::Boost => Some(KeyChord::new(true, false, true, "B")),
				Self::ViewBoosts => None,
				Self::ViewFavorites => None,
				Self::OpenUserTimeline => Some(KeyChord::new(true, false, false, "T")),
				Self::OpenUserTimelineByInput => Some(KeyChord::new(true, false, false, "U")),
				Self::Search => Some(KeyChord::new(true, false, false, "/")),
				Self::Find => Some(KeyChord::new(true, false, false, "F")),
				Self::FindNext => Some(KeyChord::new(false, false, false, "F3")),
				Self::FindPrev => Some(KeyChord::new(false, false, true, "F3")),
				Self::HomeTimeline => None,
				Self::NotificationsTimeline => None,
				Self::SentTimeline => None,
				Self::LocalTimeline => Some(KeyChord::new(true, false, false, "L")),
				Self::OpenInstanceTimelineByInput => Some(KeyChord::new(true, false, true, "I")),
				Self::FederatedTimeline => None,
				Self::DirectTimeline => Some(KeyChord::new(true, false, false, "D")),
				Self::MentionsTimeline => Some(KeyChord::new(true, false, true, "M")),
				Self::BookmarksTimeline => None,
				Self::FavoritesTimeline => None,
				Self::OpenList => None,
				Self::LoadMore => Some(KeyChord::new(false, false, false, ".")),
				Self::CloseTimeline => Some(KeyChord::new(true, false, false, "W")),
				Self::TogglePermanentTimeline => Some(KeyChord::new(true, false, true, "P")),
				Self::ToggleTimelineNotifications => Some(KeyChord::new(true, false, true, "N")),
				Self::SetTimelineSound | Self::ResetTimelineSound => None,
				Self::ClearTimeline => Some(KeyChord::new(true, false, false, "Delete")),
				Self::ClearAllTimelines => Some(KeyChord::new(true, false, true, "Delete")),
				Self::Refresh => Some(KeyChord::new(false, false, false, "F5")),
				Self::SwitchPrevTimeline => Some(KeyChord::new(false, false, false, "Left")),
				Self::SwitchNextTimeline => Some(KeyChord::new(false, false, false, "Right")),
				Self::MoveTimelineLeft => Some(KeyChord::new(false, false, true, "Left")),
				Self::MoveTimelineRight => Some(KeyChord::new(false, false, true, "Right")),
				Self::SwitchPrevAccount => Some(KeyChord::new(true, false, false, "[")),
				Self::SwitchNextAccount => Some(KeyChord::new(true, false, false, "]")),
				Self::ToggleContentWarning => Some(KeyChord::new(true, false, false, "X")),
				Self::ToggleQuickActionKeys => Some(KeyChord::new(true, false, true, "Q")),
				Self::ManageAccounts => Some(KeyChord::new(true, true, false, "A")),
				Self::ManageFilters => None,
				Self::MessageRequests => None,
				Self::ManageLists => None,
				Self::EditProfile => Some(KeyChord::new(true, false, true, "E")),
				Self::Options => Some(KeyChord::new(true, false, false, ",")),
				Self::CustomizeShortcuts => None,
				Self::CheckForUpdates => None,
				Self::ViewHelp => Some(KeyChord::new(false, false, false, "F1")),
			}
		}
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ModeShortcuts {
	#[serde(default)]
	pub bindings: HashMap<ActionId, Option<String>>,
}

impl ModeShortcuts {
	pub fn get_chord(&self, action: ActionId, is_quick: bool) -> Option<KeyChord> {
		self.bindings
			.get(&action)
			.map_or_else(|| action.default_chord(is_quick), |entry| entry.as_ref().and_then(|s| KeyChord::parse(s)))
	}

	pub fn get_menu_str(&self, action: ActionId, is_quick: bool) -> String {
		self.get_chord(action, is_quick).map_or_else(String::new, |c| c.to_shortcut_string())
	}

	pub fn set_chord(&mut self, action: ActionId, chord: Option<KeyChord>) {
		self.bindings.insert(action, chord.map(|c| c.to_shortcut_string()));
	}

	pub fn reset_action(&mut self, action: ActionId) {
		self.bindings.remove(&action);
	}

	pub fn reset_all(&mut self) {
		self.bindings.clear();
	}

	#[allow(clippy::fn_params_excessive_bools, reason = "these are the keyboard modifiers, which are separate flags")]
	pub fn find_action(&self, is_quick: bool, key_code: i32, ctrl: bool, alt: bool, shift: bool) -> Option<ActionId> {
		for &action in ActionId::all() {
			if let Some(chord) = self.get_chord(action, is_quick)
				&& chord.matches(key_code, ctrl, alt, shift)
			{
				return Some(action);
			}
		}
		None
	}
}

/// A system-wide shortcut: one that works whether or not Fedra's window is shown or focused.
///
/// Most are ordinary in-window actions under a global key. The rest only make sense without the
/// window: showing it, moving through posts and timelines with speech, and reading a post.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GlobalAction {
	ToggleWindow,
	PreviousPost,
	NextPost,
	FirstPost,
	LastPost,
	ReadPost,
	PreviousTimeline,
	NextTimeline,
	Exit,
	Action(ActionId),
}

impl GlobalAction {
	const GLOBAL_ONLY: [Self; 9] = [
		Self::ToggleWindow,
		Self::PreviousPost,
		Self::NextPost,
		Self::FirstPost,
		Self::LastPost,
		Self::ReadPost,
		Self::PreviousTimeline,
		Self::NextTimeline,
		Self::Exit,
	];

	/// The global-only actions, then every in-window action that works without the window.
	/// Switching timelines is left out because the global versions also read the post you land on,
	/// and quick action keys only apply inside the window.
	pub fn all() -> Vec<Self> {
		Self::GLOBAL_ONLY
			.into_iter()
			.chain(
				ActionId::all()
					.iter()
					.filter(|action| {
						!matches!(
							action,
							ActionId::SwitchPrevTimeline
								| ActionId::SwitchNextTimeline
								| ActionId::ToggleQuickActionKeys
						)
					})
					.map(|&action| Self::Action(action)),
			)
			.collect()
	}

	pub const fn display_name(self) -> &'static str {
		match self {
			Self::ToggleWindow => "Show or hide window",
			Self::PreviousPost => "Previous post",
			Self::NextPost => "Next post",
			Self::FirstPost => "First post",
			Self::LastPost => "Last post",
			Self::ReadPost => "Read current post",
			Self::PreviousTimeline => "Previous timeline",
			Self::NextTimeline => "Next timeline",
			Self::Exit => "Exit",
			Self::Action(action) => action.display_name(),
		}
	}

	/// The key this is saved under. In-window actions use their own config name, so a binding
	/// saved before they could be global still loads.
	fn slug(self) -> String {
		let slug = match self {
			Self::ToggleWindow => "toggle_window",
			Self::PreviousPost => "previous_post",
			Self::NextPost => "next_post",
			Self::FirstPost => "first_post",
			Self::LastPost => "last_post",
			Self::ReadPost => "read_post",
			Self::PreviousTimeline => "previous_timeline",
			Self::NextTimeline => "next_timeline",
			Self::Exit => "exit",
			Self::Action(action) => {
				return serde_json::to_value(action)
					.ok()
					.and_then(|v| v.as_str().map(str::to_string))
					.unwrap_or_default();
			}
		};
		slug.to_string()
	}

	/// Fedra's own shortcut for the same thing, in Normal mode, with Ctrl, Alt and Win added. So
	/// Ctrl+P for a profile becomes Ctrl+Alt+Win+P, and the arrows that move through the list become
	/// Ctrl+Alt+Win and an arrow.
	pub fn default_chord(self) -> Option<KeyChord> {
		let global = |key: &str| KeyChord::new(true, true, false, key).with_win(true);
		match self {
			// The show/hide hotkey Fedra has always had.
			Self::ToggleWindow => Some(KeyChord::new(true, true, false, "F")),
			Self::PreviousPost => Some(global("Up")),
			Self::NextPost => Some(global("Down")),
			Self::FirstPost => Some(global("Home")),
			Self::LastPost => Some(global("End")),
			Self::ReadPost => Some(global("Space")),
			Self::PreviousTimeline => Some(global("Left")),
			Self::NextTimeline => Some(global("Right")),
			// Exit would be Ctrl+Alt+Win+F4, too close to Windows' Ctrl+Win+F4. Find and View thread
			// would land on Follow's and Open links' keys, and the Delete ones on Ctrl+Alt+Delete.
			Self::Exit
			| Self::Action(
				ActionId::Find
				| ActionId::ViewThread
				| ActionId::DeletePost
				| ActionId::ClearTimeline
				| ActionId::ClearAllTimelines,
			) => None,
			// Ctrl+Alt+Shift+Win is the Office key, and Windows opens an Office app for these letters.
			Self::Action(action) => {
				let chord = action.default_chord(false)?;
				if chord.shift && OFFICE_KEYS.contains(&chord.key.as_str()) {
					return None;
				}
				Some(KeyChord::new(true, true, chord.shift, &chord.key).with_win(true))
			}
		}
	}
}

const OFFICE_KEYS: [&str; 10] = ["D", "L", "N", "O", "P", "T", "W", "X", "Y", "Space"];

/// The global keymap, keyed by [`GlobalAction`]'s saved name. An action missing from `bindings`
/// is on its default; one mapped to `None` was unbound on purpose.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct GlobalShortcuts {
	#[serde(default)]
	pub bindings: HashMap<String, Option<String>>,
}

impl GlobalShortcuts {
	pub fn get_chord(&self, action: GlobalAction) -> Option<KeyChord> {
		self.bindings
			.get(&action.slug())
			.map_or_else(|| action.default_chord(), |entry| entry.as_ref().and_then(|s| KeyChord::parse(s)))
	}

	pub fn is_customized(&self, action: GlobalAction) -> bool {
		self.bindings.contains_key(&action.slug())
	}

	pub fn set_chord(&mut self, action: GlobalAction, chord: Option<KeyChord>) {
		self.bindings.insert(action.slug(), chord.map(|c| c.to_shortcut_string()));
	}

	pub fn reset_action(&mut self, action: GlobalAction) {
		self.bindings.remove(&action.slug());
	}

	pub fn reset_all(&mut self) {
		self.bindings.clear();
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ShortcutsConfig {
	#[serde(default)]
	pub normal: ModeShortcuts,
	#[serde(default)]
	pub quick_keys: ModeShortcuts,
	#[serde(default)]
	pub global: GlobalShortcuts,
}

impl ShortcutsConfig {
	pub const fn active_mode(&self, quick: bool) -> &ModeShortcuts {
		if quick { &self.quick_keys } else { &self.normal }
	}

	pub const fn active_mode_mut(&mut self, quick: bool) -> &mut ModeShortcuts {
		if quick { &mut self.quick_keys } else { &mut self.normal }
	}

	pub fn get_chord(&self, quick: bool, action: ActionId) -> Option<KeyChord> {
		self.active_mode(quick).get_chord(action, quick)
	}

	pub fn get_menu_str(&self, quick: bool, action: ActionId) -> String {
		self.active_mode(quick).get_menu_str(action, quick)
	}

	#[allow(clippy::fn_params_excessive_bools, reason = "these are the keyboard modifiers, which are separate flags")]
	pub fn find_action(&self, quick: bool, key_code: i32, ctrl: bool, alt: bool, shift: bool) -> Option<ActionId> {
		self.active_mode(quick).find_action(quick, key_code, ctrl, alt, shift)
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DefaultTimeline {
	Local,
	Federated,
	Direct,
	Bookmarks,
	Favorites,
	Mentions,
	Sent,
}

impl DefaultTimeline {
	pub const fn all() -> &'static [Self] {
		&[Self::Local, Self::Federated, Self::Direct, Self::Bookmarks, Self::Favorites, Self::Mentions, Self::Sent]
	}

	pub const fn display_name(self) -> &'static str {
		match self {
			Self::Local => "Local",
			Self::Federated => "Federated",
			Self::Direct => "Direct Messages",
			Self::Bookmarks => "Bookmarks",
			Self::Favorites => "Favorites",
			Self::Mentions => "Mentions",
			Self::Sent => "Sent",
		}
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SortOrder {
	NewestToOldest,
	#[default]
	OldestToNewest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TimestampFormat {
	#[default]
	Relative,
	Absolute,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ContentWarningDisplay {
	#[default]
	Inline,
	Hidden,
	WarningOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DisplayNameEmojiMode {
	#[default]
	None,
	UnicodeOnly,
	InstanceOnly,
	All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AutoloadMode {
	Never,
	AtEnd,
	#[default]
	AtBoundary,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimelineFilter {
	#[serde(default = "default_true")]
	pub original_posts: bool,
	#[serde(default = "default_true")]
	pub replies_to_others: bool,
	#[serde(default = "default_true")]
	pub replies_to_me: bool,
	#[serde(default = "default_true")]
	pub threads: bool,
	#[serde(default = "default_true")]
	pub boosts: bool,
	#[serde(default = "default_true")]
	pub quote_posts: bool,
	#[serde(default = "default_true")]
	pub media_posts: bool,
	#[serde(default = "default_true")]
	pub text_only_posts: bool,
	#[serde(default = "default_true")]
	pub your_posts: bool,
	#[serde(default = "default_true")]
	pub your_replies: bool,
}

impl Default for TimelineFilter {
	fn default() -> Self {
		Self {
			original_posts: true,
			replies_to_others: true,
			replies_to_me: true,
			threads: true,
			boosts: true,
			quote_posts: true,
			media_posts: true,
			text_only_posts: true,
			your_posts: true,
			your_replies: true,
		}
	}
}

const fn default_true() -> bool {
	true
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TimelineFilters {
	#[serde(default)]
	pub per_timeline: HashMap<String, TimelineFilter>,
}

impl TimelineFilters {
	pub fn resolve(&self, key: &str) -> TimelineFilter {
		self.per_timeline.get(key).cloned().unwrap_or_default()
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PostTemplates {
	#[serde(default)]
	pub per_timeline: HashMap<String, PerTimelineTemplates>,
}

impl PostTemplates {
	pub fn resolve_post_template(&self, key: &str) -> &str {
		self.per_timeline.get(key).and_then(|pt| pt.post.as_deref()).unwrap_or(DEFAULT_POST_TEMPLATE)
	}

	pub fn resolve_boost_template(&self, key: &str) -> &str {
		self.per_timeline.get(key).and_then(|pt| pt.boost.as_deref()).unwrap_or(DEFAULT_BOOST_TEMPLATE)
	}

	pub fn resolve_quote_template(&self, key: &str) -> &str {
		self.per_timeline.get(key).and_then(|pt| pt.quote.as_deref()).unwrap_or(DEFAULT_QUOTE_TEMPLATE)
	}

	pub fn resolve_favorite_template(&self, key: &str) -> &str {
		self.per_timeline.get(key).and_then(|pt| pt.favorite.as_deref()).unwrap_or(DEFAULT_FAVORITE_TEMPLATE)
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PerTimelineTemplates {
	#[serde(rename = "post_template")]
	pub post: Option<String>,
	#[serde(rename = "boost_template")]
	pub boost: Option<String>,
	#[serde(rename = "quote_template")]
	pub quote: Option<String>,
	#[serde(rename = "favorite_template")]
	pub favorite: Option<String>,
}

const fn default_enter_to_send() -> bool {
	true
}

const fn default_always_show_link_dialog() -> bool {
	false
}

const fn default_show_link_previews() -> bool {
	false
}

const fn default_quick_action_keys() -> bool {
	false
}

const fn default_preserve_thread_order() -> bool {
	true
}

const fn default_check_for_updates() -> bool {
	true
}

const fn default_strip_tracking() -> bool {
	true
}

fn default_timelines() -> Vec<DefaultTimeline> {
	vec![DefaultTimeline::Local, DefaultTimeline::Direct, DefaultTimeline::Mentions]
}

fn deserialize_autoload_mode<'de, D>(deserializer: D) -> Result<AutoloadMode, D::Error>
where
	D: Deserializer<'de>,
{
	use serde::de::Error;
	let value = Value::deserialize(deserializer)?;
	match value {
		Value::Bool(b) => Ok(if b { AutoloadMode::AtBoundary } else { AutoloadMode::Never }),
		Value::String(s) => match s.as_str() {
			"Never" => Ok(AutoloadMode::Never),
			"AtEnd" => Ok(AutoloadMode::AtEnd),
			"AtBoundary" => Ok(AutoloadMode::AtBoundary),
			_ => Err(D::Error::custom(format!("unknown autoload mode: {s}"))),
		},
		_ => Err(D::Error::custom("expected bool or string for autoload")),
	}
}

const fn default_fetch_limit() -> u8 {
	40
}

const fn default_streaming() -> bool {
	true
}

const fn default_refresh_minutes() -> u8 {
	1
}

impl Config {
	/// Carries a customized show/hide hotkey over to the `ToggleWindow` global shortcut.
	fn migrate_legacy_hotkey(mut self) -> Self {
		if let Some(hotkey) = self.legacy_hotkey.take()
			&& hotkey != HotkeyConfig::default()
			&& !self.shortcuts.global.is_customized(GlobalAction::ToggleWindow)
		{
			let chord =
				KeyChord::new(hotkey.ctrl, hotkey.alt, hotkey.shift, hotkey.key.to_string()).with_win(hotkey.win);
			self.shortcuts.global.set_chord(GlobalAction::ToggleWindow, Some(chord));
		}
		self
	}

	fn migrate_legacy_timelines(mut self) -> Self {
		if self.legacy_saved_timelines.is_empty() {
			return self;
		}
		let active_id = self.active_account_id.clone();
		let account = match active_id {
			Some(id) => self.accounts.iter_mut().find(|a| a.id == id),
			None => self.accounts.first_mut(),
		};
		if let Some(account) = account
			&& account.saved_timelines.is_empty()
		{
			account.saved_timelines = std::mem::take(&mut self.legacy_saved_timelines);
			account.saved_active_timeline = self.legacy_saved_active_timeline.take();
			account.saved_selected_post_id = self.legacy_saved_selected_post_id.take();
		}
		self
	}
}

impl Default for Config {
	fn default() -> Self {
		Self {
			version: CONFIG_VERSION,
			accounts: Vec::new(),
			active_account_id: None,
			enter_to_send: true,
			always_show_link_dialog: false,
			show_link_previews: false,
			quick_action_keys: false,
			autoload: AutoloadMode::default(),
			fetch_limit: default_fetch_limit(),
			streaming: default_streaming(),
			refresh_minutes: default_refresh_minutes(),
			sort_order: SortOrder::default(),
			content_warning_display: ContentWarningDisplay::default(),
			display_name_emoji_mode: DisplayNameEmojiMode::default(),
			user_aliases: std::collections::BTreeMap::new(),
			preserve_thread_order: true,
			default_timelines: default_timelines(),
			notification_preference: NotificationPreference::default(),
			disabled_notification_types: Vec::new(),
			silent_notification_types: Vec::new(),
			check_for_updates_on_startup: true,
			start_maximized: false,
			update_channel: UpdateChannel::default(),
			legacy_hotkey: None,
			global_keys: false,
			repeat_at_timeline_edges: true,
			strip_tracking: true,
			templates: PostTemplates::default(),
			filters: TimelineFilters::default(),
			find_loading_mode: FindLoadingMode::default(),
			window_title_template: default_window_title_template(),
			restore_open_timelines: default_restore_open_timelines(),
			sync_read_position: false,
			load_older_to_restore: false,
			shortcuts: ShortcutsConfig::default(),
			legacy_saved_timelines: Vec::new(),
			legacy_saved_active_timeline: None,
			legacy_saved_selected_post_id: None,
			saved_window_hidden: false,
		}
	}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
	pub id: String,
	pub instance: String,
	pub access_token: Option<String>,
	pub client_id: Option<String>,
	pub client_secret: Option<String>,
	pub acct: Option<String>,
	pub display_name: Option<String>,
	pub user_id: Option<String>,
	#[serde(default)]
	pub default_post_visibility: Option<String>,
	#[serde(default)]
	pub saved_timelines: Vec<crate::timeline::TimelineType>,
	#[serde(default)]
	pub saved_active_timeline: Option<crate::timeline::TimelineType>,
	#[serde(default)]
	pub saved_selected_post_id: Option<String>,
	#[serde(default)]
	#[serde(alias = "locked_timelines")]
	pub permanent_timelines: Vec<crate::timeline::TimelineType>,
	#[serde(default)]
	pub notifying_timelines: Vec<crate::timeline::TimelineType>,
	#[serde(default)]
	pub timeline_sounds: Vec<TimelineSound>,
}

/// A sound the user picked for one timeline's notifications, in place of the default.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimelineSound {
	pub timeline: crate::timeline::TimelineType,
	pub path: PathBuf,
}

impl Account {
	pub fn timeline_sound(&self, timeline: &crate::timeline::TimelineType) -> Option<&Path> {
		self.timeline_sounds.iter().find(|sound| sound.timeline == *timeline).map(|sound| sound.path.as_path())
	}

	pub fn new(instance: String) -> Self {
		Self {
			id: new_account_id(),
			instance,
			access_token: None,
			client_id: None,
			client_secret: None,
			acct: None,
			display_name: None,
			user_id: None,
			default_post_visibility: None,
			saved_timelines: Vec::new(),
			saved_active_timeline: None,
			saved_selected_post_id: None,
			permanent_timelines: Vec::new(),
			notifying_timelines: Vec::new(),
			timeline_sounds: Vec::new(),
		}
	}

	pub fn full_handle(&self) -> String {
		let host =
			Url::parse(&self.instance).ok().and_then(|u| u.host_str().map(ToString::to_string)).unwrap_or_default();
		let username = self.acct.as_deref().unwrap_or("?");
		if username.contains('@') { format!("@{username}") } else { format!("@{username}@{host}") }
	}
}

pub struct ConfigStore {
	path: PathBuf,
}

impl ConfigStore {
	pub fn new() -> Self {
		Self { path: config_path() }
	}

	pub fn load(&self) -> Config {
		match fs::read_to_string(&self.path) {
			Ok(contents) => serde_json::from_str::<Config>(&contents)
				.map(|config| config.migrate_legacy_timelines().migrate_legacy_hotkey())
				.unwrap_or_default(),
			Err(err) if err.kind() == io::ErrorKind::NotFound => Config::default(),
			Err(_) => Config::default(),
		}
	}

	pub fn save(&self, config: &Config) -> Result<()> {
		if let Some(parent) = self.path.parent() {
			fs::create_dir_all(parent)?;
		}
		let contents = serde_json::to_string_pretty(config)?;
		fs::write(&self.path, contents)?;
		Ok(())
	}
}

impl Default for ConfigStore {
	fn default() -> Self {
		Self::new()
	}
}

/// How Fedra was installed, which decides where its config and resources are.
pub fn home() -> &'static Homeport {
	static HOME: LazyLock<Homeport> = LazyLock::new(|| Homeport::new(APP_NAME));
	&HOME
}

/// The config folder, created if it doesn't exist yet.
pub fn config_dir() -> PathBuf {
	home().create_config_dir().unwrap_or_else(|_| home().config_dir())
}

fn config_path() -> PathBuf {
	config_dir().join(CONFIG_FILENAME)
}

fn new_account_id() -> String {
	let millis = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis();
	format!("acct-{millis}")
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn every_default_global_chord_has_a_modifier_and_is_unique() {
		let chords: Vec<KeyChord> = GlobalAction::all().into_iter().filter_map(GlobalAction::default_chord).collect();
		for (i, chord) in chords.iter().enumerate() {
			// The shortcuts dialog refuses a global chord without one, so a default can't lack it.
			assert!(chord.ctrl || chord.alt || chord.win, "{}", chord.to_shortcut_string());
			assert!(
				!chords[i + 1..].iter().any(|other| other.conflicts_with(chord)),
				"{} is a default twice",
				chord.to_shortcut_string()
			);
		}
	}

	#[test]
	fn no_global_default_takes_an_office_key() {
		for chord in GlobalAction::all().into_iter().filter_map(GlobalAction::default_chord) {
			assert!(
				!(chord.ctrl && chord.alt && chord.shift && chord.win && OFFICE_KEYS.contains(&chord.key.as_str())),
				"{} opens an Office app",
				chord.to_shortcut_string()
			);
		}
	}

	#[test]
	fn no_global_default_takes_an_in_window_default() {
		for global in GlobalAction::all().into_iter().filter_map(GlobalAction::default_chord) {
			for &action in ActionId::all() {
				for quick in [false, true] {
					if let Some(local) = action.default_chord(quick) {
						assert!(
							!global.conflicts_with(&local),
							"{} is both a global default and {:?}'s",
							global.to_shortcut_string(),
							action
						);
					}
				}
			}
		}
	}

	#[test]
	fn a_global_binding_saved_under_its_old_name_still_loads() {
		let json = r#"{"bindings":{"new_post":"Ctrl+Alt+Win+N","favorite":null}}"#;
		let global = serde_json::from_str::<GlobalShortcuts>(json).unwrap();
		let chord = global.get_chord(GlobalAction::Action(ActionId::NewPost)).unwrap();
		assert_eq!(chord.to_shortcut_string(), "Ctrl+Alt+Win+N");
		assert_eq!(global.get_chord(GlobalAction::Action(ActionId::Favorite)), None);
	}

	#[test]
	fn a_customized_old_hotkey_becomes_the_toggle_window_shortcut() {
		let json = r#"{"version":1,"accounts":[],"active_account_id":null,"hotkey":{"ctrl":false,"alt":true,"shift":true,"win":true,"key":"M"}}"#;
		let config = serde_json::from_str::<Config>(json).unwrap().migrate_legacy_hotkey();
		let chord = config.shortcuts.global.get_chord(GlobalAction::ToggleWindow).unwrap();
		assert_eq!(chord.to_shortcut_string(), "Alt+Shift+Win+M");
	}

	#[test]
	fn the_old_default_hotkey_leaves_the_new_default_alone() {
		let json = r#"{"version":1,"accounts":[],"active_account_id":null,"hotkey":{"ctrl":true,"alt":true,"shift":false,"win":false,"key":"F"}}"#;
		let config = serde_json::from_str::<Config>(json).unwrap().migrate_legacy_hotkey();
		assert!(!config.shortcuts.global.is_customized(GlobalAction::ToggleWindow));
	}

	#[test]
	fn test_key_chord_to_string_and_from_str() {
		let chord = KeyChord::new(true, true, true, "R");
		assert_eq!(chord.to_shortcut_string(), "Ctrl+Alt+Shift+R");
		let parsed = KeyChord::parse("ctrl+alt+shift+r").unwrap();
		assert_eq!(parsed, chord);
		let enter_chord = KeyChord::new(false, false, false, "Enter");
		assert_eq!(enter_chord.to_shortcut_string(), "Enter");
		assert_eq!(KeyChord::parse("Enter").unwrap(), enter_chord);
		let alt_enter = KeyChord::new(false, true, false, "Enter");
		assert_eq!(alt_enter.to_shortcut_string(), "Alt+Enter");
		assert_eq!(KeyChord::parse("Alt+Enter").unwrap(), alt_enter);
	}

	#[test]
	fn test_key_chord_matches_event() {
		let chord = KeyChord::new(true, false, true, "R");
		assert!(chord.matches(82, true, false, true));
		assert!(!chord.matches(82, true, false, false));
		assert!(!chord.matches(81, true, false, true));
		let enter_chord = KeyChord::new(false, false, false, "Enter");
		assert!(enter_chord.matches(13, false, false, false));
		assert!(!enter_chord.matches(13, false, true, false));
		let f5_chord = KeyChord::new(false, false, false, "F5");
		assert!(f5_chord.matches(344, false, false, false));
	}

	#[test]
	fn test_key_chord_from_key_code() {
		let chord = KeyChord::from_key_code(13, false, true, false).unwrap();
		assert_eq!(chord, KeyChord::new(false, true, false, "Enter"));
		let chord_r = KeyChord::from_key_code(82, true, false, true).unwrap();
		assert_eq!(chord_r, KeyChord::new(true, false, true, "R"));
		let chord_f3 = KeyChord::from_key_code(342, false, false, true).unwrap();
		assert_eq!(chord_f3, KeyChord::new(false, false, true, "F3"));
	}

	#[test]
	fn test_mode_shortcuts_customization_and_reset() {
		let mut mode = ModeShortcuts::default();
		assert_eq!(mode.get_chord(ActionId::NewPost, false), Some(KeyChord::new(true, false, false, "N")));
		mode.set_chord(ActionId::NewPost, Some(KeyChord::new(true, true, false, "N")));
		assert_eq!(mode.get_chord(ActionId::NewPost, false), Some(KeyChord::new(true, true, false, "N")));
		mode.reset_action(ActionId::NewPost);
		assert_eq!(mode.get_chord(ActionId::NewPost, false), Some(KeyChord::new(true, false, false, "N")));
		mode.set_chord(ActionId::NewPost, None);
		assert_eq!(mode.get_chord(ActionId::NewPost, false), None);
		mode.reset_all();
		assert_eq!(mode.get_chord(ActionId::NewPost, false), Some(KeyChord::new(true, false, false, "N")));
	}

	#[test]
	fn test_find_action() {
		let mode = ModeShortcuts::default();
		let action = mode.find_action(false, 78, true, false, false);
		assert_eq!(action, Some(ActionId::NewPost));
		let action_f5 = mode.find_action(false, 344, false, false, false);
		assert_eq!(action_f5, Some(ActionId::Refresh));
		let quick_mode = ModeShortcuts::default();
		let action_c = quick_mode.find_action(true, 67, false, false, false);
		assert_eq!(action_c, Some(ActionId::NewPost));
	}

	#[test]
	fn test_shortcuts_config_serialization() {
		let mut sc = ShortcutsConfig::default();
		sc.normal.set_chord(ActionId::Quote, Some(KeyChord::new(true, true, false, "Q")));
		let json = serde_json::to_string(&sc).unwrap();
		let deserialized: ShortcutsConfig = serde_json::from_str(&json).unwrap();
		assert_eq!(sc, deserialized);
		let empty_deserialized: ShortcutsConfig = serde_json::from_str("{}").unwrap();
		assert_eq!(empty_deserialized, ShortcutsConfig::default());
	}
}
