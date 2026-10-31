use std::{sync::Arc, thread, time::Duration};

use ship_shape::{UpdateChannel as ShipChannel, UpdaterConfig, ui::CheckTrigger};
use wxdragon::prelude::*;

use crate::{UiCommand, ui_wake::UiCommandSender};

const FEDRA_GITHUB_REPO: &str = "trypsynth/fedra";
const FEDRA_MINISIGN_KEY: &str = "RWTlkclKA9G3Jhv3wkicYywPfi5XqULERn6LrK7aIv9nYQUPbhQaxSqZ";

pub fn run_update_check(frame: Frame, silent: bool, ui_tx: UiCommandSender) {
	let config = crate::config::ConfigStore::new().load();
	let channel = match config.update_channel {
		crate::config::UpdateChannel::Stable => ShipChannel::Stable,
		crate::config::UpdateChannel::Dev => ShipChannel::Dev,
	};
	let updater_config = Arc::new(
		UpdaterConfig::new(FEDRA_GITHUB_REPO, "fedra", "Fedra", FEDRA_MINISIGN_KEY, env!("CARGO_PKG_VERSION"))
			.with_commit(env!("FEDRA_COMMIT_HASH"))
			.with_install_kind(crate::config::home().kind().into())
			.with_asset_suffix(if cfg!(target_arch = "aarch64") { "-arm64" } else { "-x64" }),
	);
	let trigger = if silent { CheckTrigger::Automatic } else { CheckTrigger::Manual };
	// Quitting through the normal close saves the config and read positions, which ship-shape's own exit skips by leaving the main loop at once.
	ship_shape::ui::run_update_check_with_exit_handler(updater_config, &frame, channel, trigger, move || {
		crate::speech::speak("Installing the update. Fedra will restart.");
		// A live region speaks on a later run-loop turn, so the window stays up long enough to say it.
		thread::spawn(move || {
			thread::sleep(Duration::from_secs(2));
			let _ = ui_tx.send(UiCommand::ExitApp);
		});
	});
}
