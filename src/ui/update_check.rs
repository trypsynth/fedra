use std::{
	cell::RefCell,
	env,
	os::windows::process::CommandExt,
	process::Command,
	sync::{
		Arc,
		atomic::{AtomicBool, AtomicU64, Ordering},
	},
	thread,
	time::Duration,
};

use wxdragon::{ffi, prelude::*, window::WxWidget};

use crate::{
	ui::dialogs,
	update::{self, UpdateCheckOutcome, UpdateError},
};

thread_local! {
	static ACTIVE_PROGRESS: RefCell<Option<ProgressDialog>> = const { RefCell::new(None) };
}

pub fn run_update_check(frame: Frame, silent: bool) {
	let config = crate::config::ConfigStore::new().load();
	let current_version = env!("CARGO_PKG_VERSION").to_string();
	let current_commit = env!("FEDRA_COMMIT_HASH").to_string();
	let is_installer = is_installer_distribution();
	let handle_ptr = frame.handle_ptr() as usize;

	thread::spawn(move || {
		let outcome = update::check_for_updates(&current_version, &current_commit, is_installer, config.update_channel);
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
				let download_url = result.download_url;
				let progress = ProgressDialog::builder(&parent, "Fedra Update", "Downloading update...", 100)
					.with_style(
						ProgressDialogStyle::AutoHide
							| ProgressDialogStyle::AppModal
							| ProgressDialogStyle::RemainingTime,
					)
					.build();

				ACTIVE_PROGRESS.with(|p| {
					*p.borrow_mut() = Some(progress);
				});

				let downloaded = Arc::new(AtomicU64::new(0));
				let total = Arc::new(AtomicU64::new(0));
				let is_running = Arc::new(AtomicBool::new(true));

				// Heartbeat thread to keep UI alive
				let hb_downloaded = downloaded.clone();
				let hb_total = total.clone();
				let hb_is_running = is_running.clone();
				thread::spawn(move || {
					while hb_is_running.load(Ordering::Relaxed) {
						let d = hb_downloaded.load(Ordering::Relaxed);
						let t = hb_total.load(Ordering::Relaxed);
						wxdragon::call_after(Box::new(move || {
							ACTIVE_PROGRESS.with(|p| {
								if let Some(dialog) = p.borrow().as_ref() {
									if t > 0 {
										let percent = i32::try_from(d * 100 / t).unwrap_or(i32::MAX);
										dialog.update(percent, None);
									} else {
										dialog.pulse(None);
									}
								}
							});
						}));
						thread::sleep(Duration::from_millis(200));
					}
				});

				// Download thread
				let d_downloaded = downloaded;
				let d_total = total;
				let d_is_running = is_running;
				thread::spawn(move || {
					let res = update::download_update_file(&download_url, |d, t| {
						d_downloaded.store(d, Ordering::Relaxed);
						d_total.store(t, Ordering::Relaxed);
					});

					d_is_running.store(false, Ordering::Relaxed);
					wxdragon::call_after(Box::new(move || {
						ACTIVE_PROGRESS.with(|p| {
							*p.borrow_mut() = None;
						});
						execute_update(handle_addr, res);
					}));
				});
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

fn execute_update(handle_addr: usize, result: Result<std::path::PathBuf, UpdateError>) {
	let handle = handle_addr as *mut ffi::wxd_Window_t;
	let parent = ParentWindow { handle };

	match result {
		Ok(path) => {
			let path_str = path.to_string_lossy();
			if path_str.ends_with(".exe") {
				let current_exe = match std::env::current_exe() {
					Ok(p) => p,
					Err(e) => {
						let dlg =
							MessageDialog::builder(&parent, &format!("Failed to get current exe path: {e}"), "Error")
								.with_style(MessageDialogStyle::OK | MessageDialogStyle::IconError)
								.build();
						dlg.show_modal();
						return;
					}
				};
				let pid = std::process::id();
				let script = format!(
					"Start-Sleep -Seconds 1; Wait-Process -Id {} -ErrorAction SilentlyContinue; Start-Process -FilePath '{}' -ArgumentList '/silent' -Wait; Start-Process -FilePath '{}'",
					pid,
					path.display(),
					current_exe.display()
				);
				if let Err(e) = Command::new("powershell.exe")
					.arg("-NoProfile")
					.arg("-ExecutionPolicy")
					.arg("Bypass")
					.arg("-Command")
					.arg(&script)
					.creation_flags(0x0800_0000) // CREATE_NO_WINDOW
					.spawn()
				{
					let dlg =
						MessageDialog::builder(&parent, &format!("Failed to launch installer script: {e}"), "Error")
							.with_style(MessageDialogStyle::OK | MessageDialogStyle::IconError)
							.build();
					dlg.show_modal();
					return;
				}
				std::process::exit(0);
			} else if path_str.ends_with(".zip") {
				let current_exe = match std::env::current_exe() {
					Ok(p) => p,
					Err(e) => {
						let dlg =
							MessageDialog::builder(&parent, &format!("Failed to get current exe path: {e}"), "Error")
								.with_style(MessageDialogStyle::OK | MessageDialogStyle::IconError)
								.build();
						dlg.show_modal();
						return;
					}
				};
				let exe_dir = current_exe.parent().unwrap_or(&current_exe);

				let pid = std::process::id();
				let script = format!(
					"Start-Sleep -Seconds 1; Wait-Process -Id {}; Expand-Archive -Path '{}' -DestinationPath '{}' -Force; Remove-Item -Path '{}' -Force; Start-Process '{}'",
					pid,
					path.display(),
					exe_dir.display(),
					path.display(),
					current_exe.display()
				);

				if let Err(e) = Command::new("powershell.exe")
					.arg("-NoProfile")
					.arg("-ExecutionPolicy")
					.arg("Bypass")
					.arg("-Command")
					.arg(&script)
					.creation_flags(0x0800_0000) // CREATE_NO_WINDOW
					.spawn()
				{
					let dlg = MessageDialog::builder(&parent, &format!("Failed to launch update script: {e}"), "Error")
						.with_style(MessageDialogStyle::OK | MessageDialogStyle::IconError)
						.build();
					dlg.show_modal();
					return;
				}
				std::process::exit(0);
			} else {
				let dlg = MessageDialog::builder(&parent, "Unknown update file format.", "Error")
					.with_style(MessageDialogStyle::OK | MessageDialogStyle::IconError)
					.build();
				dlg.show_modal();
			}
		}
		Err(e) => {
			let dlg = MessageDialog::builder(&parent, &format!("Update failed: {e}"), "Error")
				.with_style(MessageDialogStyle::OK | MessageDialogStyle::IconError)
				.build();
			dlg.show_modal();
		}
	}
}
