use std::{
	cell::{Cell, RefCell},
	rc::Rc,
	sync::{
		Arc,
		atomic::{AtomicBool, AtomicU64, Ordering},
	},
	thread,
	time::Duration,
};

use url::Url;
use wxdragon::{prelude::*, widgets::media_ctrl::SeekMode};

thread_local! {
	static ACTIVE_PROGRESS: RefCell<Option<ProgressDialog>> = const { RefCell::new(None) };
}

pub fn show_media_player(_parent: &dyn WxWidget, url: String, _access_token: Option<String>) {
	const ID_MEDIA_CTRL: i32 = 10000;
	let frame = Frame::builder().with_title("Media Player").with_size(Size::new(800, 600)).build();
	let sizer = BoxSizer::builder(Orientation::Vertical).build();
	let media_ctrl = wxdragon::widgets::MediaCtrl::builder(&frame)
		.with_id(ID_MEDIA_CTRL)
		.with_backend_name("wxAMMediaBackend")
		.build();
	media_ctrl.show_player_controls(wxdragon::widgets::media_ctrl::MediaCtrlPlayerControls::None);
	sizer.add(&media_ctrl, 1, SizerFlag::Expand | SizerFlag::All, 10);
	frame.set_sizer(sizer, true);
	let is_playing = Rc::new(Cell::new(false));
	const ID_PLAY_PAUSE: i32 = 10001;
	const ID_SEEK_BACK: i32 = 10002;
	const ID_SEEK_FWD: i32 = 10003;
	const ID_VOL_UP: i32 = 10004;
	const ID_VOL_DOWN: i32 = 10005;
	const ID_DOWNLOAD: i32 = 10006;
	const ID_CLOSE: i32 = 10007;
	let menu = Menu::builder()
		.append_item(ID_PLAY_PAUSE, "Play/Pause\tSpace", "Play or pause the media")
		.append_item(ID_SEEK_BACK, "Seek Backward\tLeft", "Seek backward 10 seconds")
		.append_item(ID_SEEK_FWD, "Seek Forward\tRight", "Seek forward 10 seconds")
		.append_item(ID_VOL_UP, "Volume Up\tUp", "Increase volume")
		.append_item(ID_VOL_DOWN, "Volume Down\tDown", "Decrease volume")
		.append_separator()
		.append_item(ID_DOWNLOAD, "Download\tD", "Download this media file")
		.append_separator()
		.append_item(ID_CLOSE, "Close\tEscape", "Close media player")
		.build();
	let menu_bar = MenuBar::builder().append(menu, "&Playback").build();
	frame.set_menu_bar(menu_bar);
	frame.on_menu_selected({
		let mc = media_ctrl.clone();
		let is_playing = is_playing.clone();
		let frm = frame.clone();
		let url = url.clone();
		move |event| match event.get_id() {
			ID_PLAY_PAUSE => {
				if is_playing.get() {
					mc.pause();
					is_playing.set(false);
				} else {
					mc.play();
					is_playing.set(true);
				}
			}
			ID_SEEK_BACK => {
				mc.seek(-10000, SeekMode::FromCurrent);
			}
			ID_SEEK_FWD => {
				mc.seek(10000, SeekMode::FromCurrent);
			}
			ID_VOL_UP => {
				let v = (mc.get_volume() + 0.1).min(1.0);
				mc.set_volume(v);
			}
			ID_VOL_DOWN => {
				let v = (mc.get_volume() - 0.1).max(0.0);
				mc.set_volume(v);
			}
			ID_DOWNLOAD => {
				let default_file = if let Ok(u) = Url::parse(&url) {
					u.path_segments()
						.and_then(|segments| segments.last())
						.filter(|s| !s.is_empty())
						.unwrap_or("media")
						.to_string()
				} else {
					"media".to_string()
				};
				let dialog = FileDialog::builder(&frm)
					.with_message("Save Media As")
					.with_default_file(&default_file)
					.with_style(FileDialogStyle::Save | FileDialogStyle::OverwritePrompt)
					.build();
				if dialog.show_modal() == ID_OK {
					if let Some(path) = dialog.get_path() {
						let download_url = url.clone();
						let progress = ProgressDialog::builder(&frm, "Downloading Media", "Downloading media...", 100)
							.with_style(
								ProgressDialogStyle::AutoHide
									| ProgressDialogStyle::AppModal
									| ProgressDialogStyle::RemainingTime
									| ProgressDialogStyle::CanAbort,
							)
							.build();
						ACTIVE_PROGRESS.with(|p| {
							*p.borrow_mut() = Some(progress);
						});
						let downloaded = Arc::new(AtomicU64::new(0));
						let total = Arc::new(AtomicU64::new(0));
						let is_running = Arc::new(AtomicBool::new(true));
						let cancelled = Arc::new(AtomicBool::new(false));
						let hb_downloaded = downloaded.clone();
						let hb_total = total.clone();
						let hb_is_running = is_running.clone();
						let hb_cancelled = cancelled.clone();
						thread::spawn(move || {
							while hb_is_running.load(Ordering::Relaxed) {
								let d = hb_downloaded.load(Ordering::Relaxed);
								let t = hb_total.load(Ordering::Relaxed);
								let current_cancelled = hb_cancelled.clone();
								wxdragon::call_after(Box::new(move || {
									ACTIVE_PROGRESS.with(|p| {
										if let Some(dialog) = p.borrow().as_ref() {
											if t > 0 {
												let percent = i32::try_from(d * 100 / t).unwrap_or(i32::MAX);
												if !dialog.update(percent, None) {
													current_cancelled.store(true, Ordering::Relaxed);
												}
											} else if !dialog.pulse(None) {
												current_cancelled.store(true, Ordering::Relaxed);
											}
										}
									});
								}));
								thread::sleep(Duration::from_millis(200));
							}
						});
						let d_downloaded = downloaded;
						let d_total = total;
						let d_is_running = is_running;
						let d_cancelled = cancelled;
						thread::spawn(move || {
							let result: anyhow::Result<()> = (|| {
								let client = reqwest::blocking::Client::builder().user_agent("Fedra/0.1").build()?;
								let mut resp = client.get(download_url).send()?.error_for_status()?;
								let total_size = resp.content_length().unwrap_or(0);
								d_total.store(total_size, Ordering::Relaxed);
								let mut file = std::fs::File::create(&path)?;
								let mut buf = [0u8; 8192];
								let mut current_downloaded = 0;
								loop {
									if d_cancelled.load(Ordering::Relaxed) {
										return Err(anyhow::anyhow!("Download cancelled"));
									}
									let n = std::io::Read::read(&mut resp, &mut buf)?;
									if n == 0 {
										break;
									}
									std::io::Write::write_all(&mut file, &buf[..n])?;
									current_downloaded += n as u64;
									d_downloaded.store(current_downloaded, Ordering::Relaxed);
								}
								Ok(())
							})();
							d_is_running.store(false, Ordering::Relaxed);
							wxdragon::call_after(Box::new(move || {
								ACTIVE_PROGRESS.with(|p| {
									*p.borrow_mut() = None;
								});
							}));
							match result {
								Ok(()) => {
									wxdragon::call_after(Box::new(move || {
										if let Some(top_win) =
											wxdragon::get_app_instance().and_then(|a| a.get_top_window())
										{
											let dlg = MessageDialog::builder(&top_win, "Download complete.", "Fedra")
												.with_style(
													MessageDialogStyle::OK | MessageDialogStyle::IconInformation,
												)
												.build();
											dlg.show_modal();
											dlg.destroy();
											if let Some(mc_win) = top_win.find_window_by_id(ID_MEDIA_CTRL) {
												mc_win.set_focus();
											}
										}
									}));
								}
								Err(e) if e.to_string() == "Download cancelled" => {
									let _ = std::fs::remove_file(&path);
								}
								Err(e) => {
									let msg = format!("Failed to download media: {e}");
									wxdragon::call_after(Box::new(move || {
										if let Some(top_win) =
											wxdragon::get_app_instance().and_then(|a| a.get_top_window())
										{
											let dlg = MessageDialog::builder(&top_win, &msg, "Fedra")
												.with_style(MessageDialogStyle::OK | MessageDialogStyle::IconError)
												.build();
											dlg.show_modal();
											dlg.destroy();
											if let Some(mc_win) = top_win.find_window_by_id(ID_MEDIA_CTRL) {
												mc_win.set_focus();
											}
										}
									}));
								}
							}
						});
					}
				}
				dialog.destroy();
			}
			ID_CLOSE => {
				frm.close(true);
			}
			_ => {}
		}
	});
	if !media_ctrl.load_uri(&url) {
		let dlg = MessageDialog::builder(
			&frame,
			"Failed to load media. Your system may be missing required media components (DirectShow/quartz.dll).",
			"Media Player Error",
		)
		.with_style(MessageDialogStyle::OK | MessageDialogStyle::IconError)
		.build();
		dlg.show_modal();
		dlg.destroy();
		frame.close(true);
		return;
	}
	frame.show(true);
	media_ctrl.set_focus();
}
