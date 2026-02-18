use std::{
	collections::HashMap,
	env, fs, io,
	path::{Path, PathBuf},
	time::{SystemTime, UNIX_EPOCH},
};

use anyhow::Result;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

use crate::template::{DEFAULT_BOOST_TEMPLATE, DEFAULT_POST_TEMPLATE};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DefaultTimeline {
	Local,
	Federated,
	Direct,
	Bookmarks,
	Favorites,
}

impl DefaultTimeline {
	pub const fn all() -> &'static [Self] {
		&[Self::Local, Self::Federated, Self::Direct, Self::Bookmarks, Self::Favorites]
	}

	pub const fn display_name(self) -> &'static str {
		match self {
			Self::Local => "Local",
			Self::Federated => "Federated",
			Self::Direct => "Direct Messages",
			Self::Bookmarks => "Bookmarks",
			Self::Favorites => "Favorites",
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostTemplates {
	#[serde(default = "default_post_template")]
	pub post_template: String,
	#[serde(default = "default_boost_template")]
	pub boost_template: String,
	#[serde(default)]
	pub per_timeline: HashMap<String, PerTimelineTemplates>,
}

impl Default for PostTemplates {
	fn default() -> Self {
		Self {
			post_template: default_post_template(),
			boost_template: default_boost_template(),
			per_timeline: HashMap::new(),
		}
	}
}

impl PostTemplates {
	pub fn resolve_post_template(&self, key: &str) -> &str {
		self.per_timeline.get(key).and_then(|pt| pt.post_template.as_deref()).unwrap_or(&self.post_template)
	}

	pub fn resolve_boost_template(&self, key: &str) -> &str {
		self.per_timeline.get(key).and_then(|pt| pt.boost_template.as_deref()).unwrap_or(&self.boost_template)
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PerTimelineTemplates {
	pub post_template: Option<String>,
	pub boost_template: Option<String>,
}

fn default_post_template() -> String {
	DEFAULT_POST_TEMPLATE.to_string()
}

fn default_boost_template() -> String {
	DEFAULT_BOOST_TEMPLATE.to_string()
}

const fn default_enter_to_send() -> bool {
	true
}

const fn default_always_show_link_dialog() -> bool {
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
	vec![DefaultTimeline::Local, DefaultTimeline::Direct]
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
		}
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
