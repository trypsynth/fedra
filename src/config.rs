use std::{
	collections::HashMap,
	env, fs, io,
	path::{Path, PathBuf},
	time::{SystemTime, UNIX_EPOCH},
};

use anyhow::Result;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use url::Url;

use crate::template::{DEFAULT_BOOST_TEMPLATE, DEFAULT_POST_TEMPLATE, DEFAULT_QUOTE_TEMPLATE};

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
	#[serde(default)]
	pub sort_order: SortOrder,
	#[serde(default)]
	pub content_warning_display: ContentWarningDisplay,
	#[serde(default)]
	pub display_name_emoji_mode: DisplayNameEmojiMode,
	#[serde(default = "default_preserve_thread_order")]
	pub preserve_thread_order: bool,
	#[serde(default = "default_timelines")]
	pub default_timelines: Vec<DefaultTimeline>,
	#[serde(default)]
	pub notification_preference: NotificationPreference,
	#[serde(default = "default_check_for_updates")]
	pub check_for_updates_on_startup: bool,
	#[serde(default)]
	pub update_channel: UpdateChannel,
	#[serde(default)]
	pub hotkey: HotkeyConfig,
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
	pub shortcuts: ShortcutsConfig,
	#[serde(default)]
	pub saved_timelines: Vec<crate::timeline::TimelineType>,
	#[serde(default)]
	pub saved_active_timeline: Option<crate::timeline::TimelineType>,
	#[serde(default)]
	pub saved_selected_post_id: Option<String>,
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
			Self::ManageLists => "Manage Lists...",
			Self::EditProfile => "Edit Profile...",
			Self::Options => "Options...",
			Self::CustomizeShortcuts => "Customize Keyboard Shortcuts...",
			Self::CheckForUpdates => "Check for Updates...",
			Self::ViewHelp => "View Help",
		}
	}

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

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct KeyChord {
	pub ctrl: bool,
	pub alt: bool,
	pub shift: bool,
	pub key: String,
}

impl KeyChord {
	pub fn new(ctrl: bool, alt: bool, shift: bool, key: impl Into<String>) -> Self {
		let key_str = key.into();
		let normalized = Self::normalize_key_name(&key_str);
		Self { ctrl, alt, shift, key: normalized }
	}

	pub fn normalize_key_name(key: &str) -> String {
		let trimmed = key.trim();
		if trimmed.eq_ignore_ascii_case("return") || trimmed.eq_ignore_ascii_case("enter") {
			"Enter".to_string()
		} else if trimmed.eq_ignore_ascii_case("space") {
			"Space".to_string()
		} else if trimmed.eq_ignore_ascii_case("tab") {
			"Tab".to_string()
		} else if trimmed.eq_ignore_ascii_case("backspace") || trimmed.eq_ignore_ascii_case("back") {
			"Backspace".to_string()
		} else if trimmed.eq_ignore_ascii_case("delete") || trimmed.eq_ignore_ascii_case("del") {
			"Delete".to_string()
		} else if trimmed.eq_ignore_ascii_case("escape") || trimmed.eq_ignore_ascii_case("esc") {
			"Escape".to_string()
		} else if trimmed.eq_ignore_ascii_case("home") {
			"Home".to_string()
		} else if trimmed.eq_ignore_ascii_case("end") {
			"End".to_string()
		} else if trimmed.eq_ignore_ascii_case("pageup")
			|| trimmed.eq_ignore_ascii_case("page up")
			|| trimmed.eq_ignore_ascii_case("pgup")
		{
			"PageUp".to_string()
		} else if trimmed.eq_ignore_ascii_case("pagedown")
			|| trimmed.eq_ignore_ascii_case("page down")
			|| trimmed.eq_ignore_ascii_case("pgdn")
		{
			"PageDown".to_string()
		} else if trimmed.eq_ignore_ascii_case("left") || trimmed.eq_ignore_ascii_case("left arrow") {
			"Left".to_string()
		} else if trimmed.eq_ignore_ascii_case("right") || trimmed.eq_ignore_ascii_case("right arrow") {
			"Right".to_string()
		} else if trimmed.eq_ignore_ascii_case("up") || trimmed.eq_ignore_ascii_case("up arrow") {
			"Up".to_string()
		} else if trimmed.eq_ignore_ascii_case("down") || trimmed.eq_ignore_ascii_case("down arrow") {
			"Down".to_string()
		} else if trimmed.len() >= 2
			&& trimmed.starts_with(['F', 'f'])
			&& trimmed[1..].chars().all(|c| c.is_ascii_digit())
		{
			format!("F{}", &trimmed[1..])
		} else if trimmed.len() == 1 {
			let ch = trimmed.chars().next().unwrap();
			if ch.is_ascii_alphabetic() { ch.to_ascii_uppercase().to_string() } else { trimmed.to_string() }
		} else {
			trimmed.to_string()
		}
	}

	pub fn to_shortcut_string(&self) -> String {
		let mut parts = Vec::new();
		if self.ctrl {
			parts.push("Ctrl");
		}
		if self.alt {
			parts.push("Alt");
		}
		if self.shift {
			parts.push("Shift");
		}
		parts.push(&self.key);
		parts.join("+")
	}

	pub fn parse(input: &str) -> Option<Self> {
		let trimmed = input.trim();
		if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("none") {
			return None;
		}
		let mut ctrl = false;
		let mut alt = false;
		let mut shift = false;
		let mut remaining = trimmed;

		while let Some(plus_idx) = remaining.find('+') {
			let prefix = &remaining[..plus_idx];
			if prefix.eq_ignore_ascii_case("ctrl") || prefix.eq_ignore_ascii_case("control") {
				ctrl = true;
				remaining = &remaining[plus_idx + 1..];
			} else if prefix.eq_ignore_ascii_case("alt") {
				alt = true;
				remaining = &remaining[plus_idx + 1..];
			} else if prefix.eq_ignore_ascii_case("shift") {
				shift = true;
				remaining = &remaining[plus_idx + 1..];
			} else {
				break;
			}
		}
		let key = remaining.to_string();
		if key.is_empty() {
			return None;
		}
		let normalized = Self::normalize_key_name(&key);
		Some(Self { ctrl, alt, shift, key: normalized })
	}

	pub fn from_key_code(key_code: i32, ctrl: bool, alt: bool, shift: bool) -> Option<Self> {
		let key_name = match key_code {
			13 | 370 => "Enter".to_string(),
			9 => "Tab".to_string(),
			32 => "Space".to_string(),
			8 => "Backspace".to_string(),
			127 | 308 | 386 => "Delete".to_string(),
			27 => "Escape".to_string(),
			313 | 377 => "Home".to_string(),
			312 | 379 => "End".to_string(),
			366 | 376 => "PageUp".to_string(),
			367 | 381 => "PageDown".to_string(),
			314 | 378 => "Left".to_string(),
			316 | 380 => "Right".to_string(),
			315 | 382 => "Up".to_string(),
			317 | 383 => "Down".to_string(),
			340..=363 => format!("F{}", key_code - 340 + 1),
			65..=90 => (char::from_u32(key_code as u32)?).to_string(),
			97..=122 => (char::from_u32((key_code - 32) as u32)?).to_string(),
			48..=57 => (char::from_u32(key_code as u32)?).to_string(),
			44 | 188 => ",".to_string(),
			46 | 190 | 387 => ".".to_string(),
			47 | 191 | 388 => "/".to_string(),
			91 | 219 => "[".to_string(),
			93 | 221 => "]".to_string(),
			92 | 220 => "\\".to_string(),
			45 | 189 | 390 => "-".to_string(),
			61 | 187 => "=".to_string(),
			59 | 186 => ";".to_string(),
			39 | 222 => "'".to_string(),
			96 | 192 => "`".to_string(),
			_ => return None,
		};
		Some(Self { ctrl, alt, shift, key: key_name })
	}

	pub fn matches(&self, key_code: i32, ctrl: bool, alt: bool, shift: bool) -> bool {
		if self.ctrl != ctrl || self.alt != alt || self.shift != shift {
			return false;
		}
		let key_str = self.key.as_str();
		if key_str.eq_ignore_ascii_case("Enter") {
			key_code == 13 || key_code == 370
		} else if key_str.eq_ignore_ascii_case("Tab") {
			key_code == 9
		} else if key_str.eq_ignore_ascii_case("Space") {
			key_code == 32
		} else if key_str.eq_ignore_ascii_case("Backspace") {
			key_code == 8
		} else if key_str.eq_ignore_ascii_case("Delete") {
			key_code == 127 || key_code == 308 || key_code == 386
		} else if key_str.eq_ignore_ascii_case("Escape") {
			key_code == 27
		} else if key_str.eq_ignore_ascii_case("Home") {
			key_code == 313 || key_code == 377
		} else if key_str.eq_ignore_ascii_case("End") {
			key_code == 312 || key_code == 379
		} else if key_str.eq_ignore_ascii_case("PageUp") {
			key_code == 366 || key_code == 376
		} else if key_str.eq_ignore_ascii_case("PageDown") {
			key_code == 367 || key_code == 381
		} else if key_str.eq_ignore_ascii_case("Left") {
			key_code == 314 || key_code == 378
		} else if key_str.eq_ignore_ascii_case("Right") {
			key_code == 316 || key_code == 380
		} else if key_str.eq_ignore_ascii_case("Up") {
			key_code == 315 || key_code == 382
		} else if key_str.eq_ignore_ascii_case("Down") {
			key_code == 317 || key_code == 383
		} else if key_str.starts_with(['F', 'f'])
			&& let Ok(num) = key_str[1..].parse::<i32>()
			&& (1..=24).contains(&num)
		{
			key_code == 340 + num - 1
		} else if key_str.len() == 1 {
			let ch = key_str.chars().next().unwrap();
			if ch.is_ascii_alphabetic() {
				let upper = ch.to_ascii_uppercase() as i32;
				key_code == upper || key_code == upper + 32
			} else if ch.is_ascii_digit() {
				key_code == ch as i32
			} else {
				match ch {
					',' => key_code == 44 || key_code == 188,
					'.' => key_code == 46 || key_code == 190 || key_code == 387,
					'/' => key_code == 47 || key_code == 191 || key_code == 388,
					'\\' => key_code == 92 || key_code == 220,
					'[' => key_code == 91 || key_code == 219,
					']' => key_code == 93 || key_code == 221,
					'-' => key_code == 45 || key_code == 189 || key_code == 390,
					'=' => key_code == 61 || key_code == 187,
					';' => key_code == 59 || key_code == 186,
					'\'' => key_code == 39 || key_code == 222,
					'`' => key_code == 96 || key_code == 192,
					_ => key_code == ch as i32,
				}
			}
		} else {
			false
		}
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnterBehaviorPreset {
	EnterLinksAltThread,
	EnterThreadAltLinks,
	Custom,
}

impl EnterBehaviorPreset {
	pub const fn all() -> &'static [Self] {
		&[Self::EnterLinksAltThread, Self::EnterThreadAltLinks, Self::Custom]
	}

	pub const fn display_name(self) -> &'static str {
		match self {
			Self::EnterLinksAltThread => "Enter opens links, Alt+Enter views thread",
			Self::EnterThreadAltLinks => "Enter views thread, Alt+Enter opens links",
			Self::Custom => "Custom",
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
		if let Some(entry) = self.bindings.get(&action) {
			match entry {
				Some(s) => KeyChord::parse(s),
				None => None,
			}
		} else {
			action.default_chord(is_quick)
		}
	}

	pub fn get_display_str(&self, action: ActionId, is_quick: bool) -> String {
		self.get_chord(action, is_quick).map_or_else(|| "None".to_string(), |c| c.to_shortcut_string())
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

	pub fn enter_behavior_preset(&self, is_quick: bool) -> EnterBehaviorPreset {
		let links = self.get_chord(ActionId::OpenLinks, is_quick);
		let thread = self.get_chord(ActionId::ViewThread, is_quick);
		let enter = Some(KeyChord::new(false, false, false, "Enter"));
		let alt_enter = Some(KeyChord::new(false, true, false, "Enter"));

		if links == enter && thread == alt_enter {
			EnterBehaviorPreset::EnterLinksAltThread
		} else if links == alt_enter && thread == enter {
			EnterBehaviorPreset::EnterThreadAltLinks
		} else {
			EnterBehaviorPreset::Custom
		}
	}

	pub fn set_enter_behavior(&mut self, _is_quick: bool, preset: EnterBehaviorPreset) {
		match preset {
			EnterBehaviorPreset::EnterLinksAltThread => {
				self.set_chord(ActionId::OpenLinks, Some(KeyChord::new(false, false, false, "Enter")));
				self.set_chord(ActionId::ViewThread, Some(KeyChord::new(false, true, false, "Enter")));
			}
			EnterBehaviorPreset::EnterThreadAltLinks => {
				self.set_chord(ActionId::OpenLinks, Some(KeyChord::new(false, true, false, "Enter")));
				self.set_chord(ActionId::ViewThread, Some(KeyChord::new(false, false, false, "Enter")));
			}
			EnterBehaviorPreset::Custom => {}
		}
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ShortcutsConfig {
	#[serde(default)]
	pub normal: ModeShortcuts,
	#[serde(default)]
	pub quick_keys: ModeShortcuts,
}

impl ShortcutsConfig {
	pub fn active_mode(&self, quick: bool) -> &ModeShortcuts {
		if quick { &self.quick_keys } else { &self.normal }
	}

	pub fn active_mode_mut(&mut self, quick: bool) -> &mut ModeShortcuts {
		if quick { &mut self.quick_keys } else { &mut self.normal }
	}

	pub fn get_chord(&self, quick: bool, action: ActionId) -> Option<KeyChord> {
		self.active_mode(quick).get_chord(action, quick)
	}

	pub fn get_menu_str(&self, quick: bool, action: ActionId) -> String {
		self.active_mode(quick).get_menu_str(action, quick)
	}

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
}

impl DefaultTimeline {
	pub const fn all() -> &'static [Self] {
		&[Self::Local, Self::Federated, Self::Direct, Self::Bookmarks, Self::Favorites, Self::Mentions]
	}

	pub const fn display_name(self) -> &'static str {
		match self {
			Self::Local => "Local",
			Self::Federated => "Federated",
			Self::Direct => "Direct Messages",
			Self::Bookmarks => "Bookmarks",
			Self::Favorites => "Favorites",
			Self::Mentions => "Mentions",
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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PerTimelineTemplates {
	#[serde(rename = "post_template")]
	pub post: Option<String>,
	#[serde(rename = "boost_template")]
	pub boost: Option<String>,
	#[serde(rename = "quote_template")]
	pub quote: Option<String>,
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
			sort_order: SortOrder::default(),
			content_warning_display: ContentWarningDisplay::default(),
			display_name_emoji_mode: DisplayNameEmojiMode::default(),
			preserve_thread_order: true,
			default_timelines: default_timelines(),
			notification_preference: NotificationPreference::default(),
			check_for_updates_on_startup: true,
			update_channel: UpdateChannel::default(),
			hotkey: HotkeyConfig::default(),
			strip_tracking: true,
			templates: PostTemplates::default(),
			filters: TimelineFilters::default(),
			find_loading_mode: FindLoadingMode::default(),
			window_title_template: default_window_title_template(),
			restore_open_timelines: default_restore_open_timelines(),
			shortcuts: ShortcutsConfig::default(),
			saved_timelines: Vec::new(),
			saved_active_timeline: None,
			saved_selected_post_id: None,
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
}

impl Account {
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
			Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
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

pub fn config_dir() -> PathBuf {
	let exe_dir = env::current_exe()
		.ok()
		.and_then(|path| path.parent().map(std::path::Path::to_path_buf))
		.unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
	if is_installed(&exe_dir)
		&& let Ok(appdata) = env::var("APPDATA")
	{
		return PathBuf::from(appdata).join(APP_NAME);
	}
	exe_dir
}

fn config_path() -> PathBuf {
	let exe_dir = env::current_exe()
		.ok()
		.and_then(|path| path.parent().map(std::path::Path::to_path_buf))
		.unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
	if is_installed(&exe_dir)
		&& let Ok(appdata) = env::var("APPDATA")
	{
		return PathBuf::from(appdata).join(APP_NAME).join(CONFIG_FILENAME);
	}
	exe_dir.join(CONFIG_FILENAME)
}

fn is_installed(exe_dir: &PathBuf) -> bool {
	let Ok(entries) = fs::read_dir(exe_dir) else {
		return false;
	};
	for entry in entries.flatten() {
		let path = entry.path();
		if !path.is_file() {
			continue;
		}
		let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
			continue;
		};
		let name = name.to_ascii_lowercase();
		if name.starts_with("unins") && Path::new(&name).extension().is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
		{
			return true;
		}
	}
	false
}

fn new_account_id() -> String {
	let millis = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis();
	format!("acct-{millis}")
}

#[cfg(test)]
mod tests {
	use super::*;

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
	fn test_enter_behavior_preset_switching() {
		let mut mode = ModeShortcuts::default();
		assert_eq!(mode.enter_behavior_preset(false), EnterBehaviorPreset::EnterLinksAltThread);

		mode.set_enter_behavior(false, EnterBehaviorPreset::EnterThreadAltLinks);
		assert_eq!(mode.enter_behavior_preset(false), EnterBehaviorPreset::EnterThreadAltLinks);
		assert_eq!(mode.get_chord(ActionId::OpenLinks, false), Some(KeyChord::new(false, true, false, "Enter")));
		assert_eq!(mode.get_chord(ActionId::ViewThread, false), Some(KeyChord::new(false, false, false, "Enter")));

		mode.set_chord(ActionId::OpenLinks, Some(KeyChord::new(true, false, false, "O")));
		assert_eq!(mode.enter_behavior_preset(false), EnterBehaviorPreset::Custom);
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
