use std::{
	env, fs, io,
	path::PathBuf,
	time::{SystemTime, UNIX_EPOCH},
};

use anyhow::Result;
use serde::{Deserialize, Serialize};

const APP_NAME: &str = "Fedra";
const CONFIG_FILENAME: &str = "config.json";
const CONFIG_VERSION: u32 = 1;

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
	pub timestamp_format: TimestampFormat,
	#[serde(default)]
	pub content_warning_display: ContentWarningDisplay,
	#[serde(default = "default_preserve_thread_order")]
	pub preserve_thread_order: bool,
	#[serde(default = "default_timelines")]
	pub default_timelines: Vec<DefaultTimeline>,
	#[serde(default)]
	pub notification_preference: NotificationPreference,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum NotificationPreference {
	#[default]
	Classic,
	Disabled,
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
	#[default]
	NewestToOldest,
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
pub enum AutoloadMode {
	Never,
	#[default]
	AtEnd,
	AtBoundary,
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

fn default_timelines() -> Vec<DefaultTimeline> {
	vec![DefaultTimeline::Local, DefaultTimeline::Direct]
}

fn deserialize_autoload_mode<'de, D>(deserializer: D) -> Result<AutoloadMode, D::Error>
where
	D: serde::Deserializer<'de>,
{
	use serde::de::Error;
	let value = serde_json::Value::deserialize(deserializer)?;
	match value {
		serde_json::Value::Bool(b) => Ok(if b { AutoloadMode::AtBoundary } else { AutoloadMode::Never }),
		serde_json::Value::String(s) => match s.as_str() {
			"Never" => Ok(AutoloadMode::Never),
			"AtEnd" => Ok(AutoloadMode::AtEnd),
			"AtBoundary" => Ok(AutoloadMode::AtBoundary),
			_ => Err(D::Error::custom(format!("unknown autoload mode: {s}"))),
		},
		_ => Err(D::Error::custom("expected bool or string for autoload")),
	}
}

const fn default_fetch_limit() -> u8 {
	20
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
			timestamp_format: TimestampFormat::default(),
			content_warning_display: ContentWarningDisplay::default(),
			preserve_thread_order: true,
			default_timelines: default_timelines(),
			notification_preference: NotificationPreference::default(),
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

fn config_path() -> PathBuf {
	let exe_dir = env::current_exe()
		.ok()
		.and_then(|path| path.parent().map(|parent| parent.to_path_buf()))
		.unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
	if is_installed(&exe_dir) {
		if let Ok(appdata) = env::var("APPDATA") {
			return PathBuf::from(appdata).join(APP_NAME).join(CONFIG_FILENAME);
		}
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
		if name.starts_with("unins") && name.ends_with(".exe") {
			return true;
		}
	}
	false
}

fn new_account_id() -> String {
	let millis = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis();
	format!("acct-{millis}")
}
