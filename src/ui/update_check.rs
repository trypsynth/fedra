use std::{env, thread};

use wxdragon::{prelude::*, window::WxWidget};
use wxdragon_sys as ffi;

use crate::{
	ui::dialogs,
	update::{self, UpdateCheckOutcome, UpdateError},
};

pub fn run_update_check(frame: Frame, silent: bool) {
	let current_version = env!("CARGO_PKG_VERSION").to_string();
	let is_installer = is_installer_distribution();
	let handle_ptr = frame.handle_ptr() as usize;

	thread::spawn(move || {
		let outcome = update::check_for_updates(&current_version, is_installer);
		wxdragon::call_after(Box::new(move || {
			present_update_result(handle_ptr, outcome, silent, &current_version);
		}));
	});
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

struct ParentWindow {
	handle: *mut ffi::wxd_Window_t,
}

impl WxWidget for ParentWindow {
	fn handle_ptr(&self) -> *mut ffi::wxd_Window_t {
		self.handle
	}
}

fn present_update_result(
	handle_addr: usize,
	outcome: Result<UpdateCheckOutcome, UpdateError>,
	silent: bool,
	current_version: &str,
) {
	let handle = handle_addr as *mut ffi::wxd_Window_t;
	let parent = ParentWindow { handle };

	match outcome {
		Ok(UpdateCheckOutcome::UpdateAvailable(result)) => {
			let latest_version =
				if result.latest_version.is_empty() { current_version.to_string() } else { result.latest_version };
			let plain_notes = crate::text::markdown_to_text(&result.release_notes);
			let release_notes =
				if plain_notes.trim().is_empty() { "No release notes provided.".to_string() } else { plain_notes };

			if dialogs::show_update_dialog(&parent, &latest_version, &release_notes) && !result.download_url.is_empty()
			{
				let _ = wxdragon::utils::launch_default_browser(
					&result.download_url,
					wxdragon::utils::BrowserLaunchFlags::Default,
				);
			}
		}
		Ok(UpdateCheckOutcome::UpToDate(ver)) => {
			if !silent {
				let msg = format!("No updates available. Latest version: {ver}");
				let dialog = MessageDialog::builder(&parent, &msg, "Info")
					.with_style(MessageDialogStyle::OK | MessageDialogStyle::IconInformation)
					.build();
				dialog.show_modal();
			}
		}
		Err(e) => {
			if !silent {
				let dialog = MessageDialog::builder(&parent, &e.to_string(), "Fedra")
					.with_style(MessageDialogStyle::OK | MessageDialogStyle::IconError)
					.build();
				dialog.show_modal();
			}
		}
	}
}
