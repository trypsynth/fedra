use std::{
	collections::{HashMap, HashSet},
	fs::{self, File},
	io::{self, Write},
	path::Path,
	sync::{Arc, Mutex},
};

use serde::{Deserialize, Serialize};

use super::AccountCache;

#[derive(Serialize, Deserialize)]
pub struct FileData {
	pub version: u32,
	pub accounts: HashMap<String, AccountCache>,
}

pub struct Loaded {
	pub accounts: HashMap<String, AccountCache>,
	pub error: Option<String>,
	pub writable: bool,
}

pub fn append_log(path: &Path, message: &str) -> io::Result<()> {
	if let Some(parent) = path.parent() {
		fs::create_dir_all(parent)?;
	}
	let mut file = fs::OpenOptions::new().create(true).append(true).open(path)?;
	writeln!(file, "{message}")?;
	file.flush()
}

pub fn load(path: &Path, configured: &HashSet<String>) -> Loaded {
	let mut loaded = Loaded { accounts: HashMap::new(), error: None, writable: true };
	let bytes = match fs::read(path) {
		Ok(bytes) => bytes,
		Err(error) if error.kind() == io::ErrorKind::NotFound => return loaded,
		Err(error) => {
			loaded.error = Some(error.to_string());
			loaded.writable = false;
			return loaded;
		}
	};
	let value = serde_json::from_slice::<serde_json::Value>(&bytes);
	if let Ok(value) = &value
		&& value.get("version").and_then(serde_json::Value::as_u64).is_some_and(|v| v > 1)
	{
		loaded.writable = false;
		loaded.error = Some("Autocomplete persistence is unavailable: the cache uses a newer schema.".into());
		return loaded;
	}
	let mut corrupt = true;
	if let Ok(value) = value
		&& value.get("version").and_then(serde_json::Value::as_u64) == Some(1)
		&& let Some(accounts) = value.get("accounts").and_then(serde_json::Value::as_object)
	{
		corrupt = false;
		for (id, value) in accounts {
			if !configured.contains(id) {
				continue;
			}
			match serde_json::from_value::<AccountCache>(value.clone()) {
				Ok(account) if account.valid() => {
					loaded.accounts.insert(id.clone(), account);
				}
				_ => {
					corrupt = true;
				}
			}
		}
	}
	if corrupt {
		let backup = path.with_extension(format!("corrupt-{}.json", chrono::Utc::now().timestamp_millis()));
		if let Err(error) = fs::copy(path, backup) {
			loaded.writable = false;
			loaded.error = Some(format!("Could not preserve corrupt autocomplete cache: {error}"));
		}
	}
	loaded
}

/// Only replacement holds the removal gate. Serialization and flushing never block mutations.
pub fn save(path: &Path, data: &FileData, epoch: u64, gate: &Arc<Mutex<u64>>) -> io::Result<bool> {
	if let Some(parent) = path.parent() {
		fs::create_dir_all(parent)?;
	}
	let temporary = path.with_extension("json.tmp");
	let result = (|| {
		let mut file = File::create(&temporary)?;
		serde_json::to_writer(&mut file, data)?;
		file.flush()?;
		file.sync_all()?;
		drop(file);
		let current = gate.lock().unwrap();
		if *current != epoch {
			return Ok(false);
		}
		replace(&temporary, path)?;
		drop(current);
		Ok(true)
	})();
	if temporary.exists() {
		let _ = fs::remove_file(&temporary);
	}
	result
}

#[cfg(windows)]
fn replace(source: &Path, destination: &Path) -> io::Result<()> {
	use std::os::windows::ffi::OsStrExt;

	use windows::{
		Win32::Storage::FileSystem::{MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW},
		core::PCWSTR,
	};
	let source: Vec<u16> = source.as_os_str().encode_wide().chain([0]).collect();
	let destination: Vec<u16> = destination.as_os_str().encode_wide().chain([0]).collect();
	unsafe {
		MoveFileExW(
			PCWSTR(source.as_ptr()),
			PCWSTR(destination.as_ptr()),
			MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
		)
	}
	.map_err(io::Error::other)
}

#[cfg(not(windows))]
fn replace(source: &Path, destination: &Path) -> io::Result<()> {
	fs::rename(source, destination)
}
