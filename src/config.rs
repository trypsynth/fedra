use std::{
	env, fs, io,
	path::PathBuf,
	time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

const APP_NAME: &str = "Fedra";
const CONFIG_FILENAME: &str = "config.json";
const CONFIG_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
	pub version: u32,
	pub accounts: Vec<Account>,
	pub active_account_id: Option<String>,
}

impl Default for Config {
	fn default() -> Self {
		Self { version: CONFIG_VERSION, accounts: Vec::new(), active_account_id: None }
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
}

impl Account {
	pub fn new(instance: String) -> Self {
		Self { id: new_account_id(), instance, access_token: None, client_id: None, client_secret: None, acct: None }
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

	pub fn save(&self, config: &Config) -> io::Result<()> {
		if let Some(parent) = self.path.parent() {
			fs::create_dir_all(parent)?;
		}
		let contents = serde_json::to_string_pretty(config).unwrap_or_else(|_| "{}".to_string());
		fs::write(&self.path, contents)
	}
}

fn config_path() -> PathBuf {
	if let Ok(appdata) = env::var("APPDATA") {
		return PathBuf::from(appdata).join(APP_NAME).join(CONFIG_FILENAME);
	}
	env::current_dir().unwrap_or_else(|_| PathBuf::from(".")).join(CONFIG_FILENAME)
}

fn new_account_id() -> String {
	let millis = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis();
	format!("acct-{}", millis)
}
