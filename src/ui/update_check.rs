use std::{env, sync::Arc};

use ship_shape::{UpdateChannel as ShipChannel, UpdaterConfig};
use wxdragon::prelude::*;

const FEDRA_GITHUB_REPO: &str = "trypsynth/fedra";
const FEDRA_MINISIGN_KEY: &str = "RWTlkclKA9G3Jhv3wkicYywPfi5XqULERn6LrK7aIv9nYQUPbhQaxSqZ";

pub fn run_update_check(frame: Frame, silent: bool) {
	let config = crate::config::ConfigStore::new().load();
	let channel = match config.update_channel {
		crate::config::UpdateChannel::Stable => ShipChannel::Stable,
		crate::config::UpdateChannel::Dev => ShipChannel::Dev,
	};
	let updater_config = Arc::new(UpdaterConfig::new(
		FEDRA_GITHUB_REPO,
		"fedra",
		"Fedra",
		FEDRA_MINISIGN_KEY,
		format!("fedra/{}", env!("CARGO_PKG_VERSION")),
	));
	ship_shape::ui::run_update_check(
		updater_config,
		frame.handle_ptr() as usize,
		env!("CARGO_PKG_VERSION"),
		env!("FEDRA_COMMIT_HASH"),
		is_installer_distribution(),
		channel,
		silent,
	);
}

fn is_installer_distribution() -> bool {
	let Ok(exe_path) = env::current_exe() else {
		return false;
	};
	let Some(exe_dir) = exe_path.parent() else {
		return false;
	};
	exe_dir.join("unins000.exe").exists()
}
