use std::{
	cell::RefCell,
	collections::HashMap,
	path::PathBuf,
	rc::Rc,
	sync::{
		Arc, Mutex,
		atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
	},
	thread,
	time::Duration,
};

use accesskit::{ActionHandler, ActionRequest, ActivationHandler, Node, NodeId, Role, TreeInfo, TreeUpdate};
use accesskit_windows::SubclassingAdapter;
use rodio::Source;
use url::Url;
use windows::Win32::Foundation::HWND;
use wxdragon::prelude::*;

use crate::audio;

// Background threads (downloading, decoding) never touch wxWidgets handles
// or Rc-based state directly, since those aren't `Send`. Instead they only
// exchange plain `Send` data (ids, atomics, decoders) with the main thread
// via `wxdragon::call_after`, and callbacks that DO need to close over
// widgets/Rc state are registered here, keyed by task id, so they're only
// ever stored and invoked on the main thread.
thread_local! {
	static ACTIVE_PROGRESS: RefCell<Option<ProgressDialog>> = const { RefCell::new(None) };
	static ACTIVE_DOWNLOAD_DONE: RefCell<HashMap<usize, Box<dyn FnOnce(DownloadOutcome)>>> = RefCell::new(HashMap::new());
	static ACTIVE_LOAD_DONE: RefCell<HashMap<usize, Box<dyn FnOnce(Result<DecodedSource, String>)>>> = RefCell::new(HashMap::new());
	static ACTIVE_TICKS: RefCell<HashMap<usize, Box<dyn Fn(TickerUpdate)>>> = RefCell::new(HashMap::new());
}

/// A progress report sent to whatever's registered in [`ACTIVE_TICKS`].
#[derive(Clone, Copy)]
enum TickerUpdate {
	Downloading { downloaded: u64, total: u64 },
	/// The download itself finished, but decoding hasn't caught up yet (for
	/// some formats, working out an accurate duration means scanning a good
	/// chunk of the file). Without this, the status label would sit on
	/// whatever download percentage it last showed — which, thanks to the
	/// ticker's own polling interval, is rarely a clean 100% — making it
	/// look stuck or wrong right when the file is actually fully there.
	Finishing,
}

static NEXT_TASK_ID: AtomicUsize = AtomicUsize::new(0);

/// Queues `f` to run on the UI thread, like [`wxdragon::call_after`], and
/// also wakes the idle loop so it actually runs promptly.
///
/// `call_after` on its own only gets processed the next time something else
/// pumps the event loop — wx only checks the queue when it goes idle after
/// dispatching a message, and if the window is just sitting there with no
/// mouse movement or other input, nothing pumps it. That left a queued
/// update (e.g. "the file just finished loading") sitting invisible until
/// some unrelated input arrived, so the *next* keypress would still see the
/// stale state (reporting "still loading" for media that had, in fact,
/// finished loading a while ago) and only the keypress *after that* would
/// see it as ready, since it pumped the queue itself.
fn ui_call_after(f: impl FnOnce() + Send + 'static) {
	wxdragon::call_after(Box::new(f));
	wxdragon::wake_up_idle();
}

const LR_ROOT_ID: NodeId = NodeId(1);
const LR_ANNOUNCEMENT_ID: NodeId = NodeId(2);

struct MediaActivationHandler;

impl ActivationHandler for MediaActivationHandler {
	fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
		let mut root = Node::new(Role::Window);
		root.set_children(vec![LR_ANNOUNCEMENT_ID]);

		let mut ann_node = Node::new(Role::Label);
		ann_node.set_value("");
		ann_node.set_live(accesskit::Live::Polite);

		Some(TreeUpdate {
			nodes: vec![(LR_ANNOUNCEMENT_ID, ann_node), (LR_ROOT_ID, root)],
			tree: Some(TreeInfo::new(LR_ROOT_ID)),
			focus: LR_ROOT_ID,
			tree_id: accesskit::TreeId::ROOT,
		})
	}
}

struct MediaActionHandler;

impl ActionHandler for MediaActionHandler {
	fn do_action(&mut self, _request: ActionRequest) {}
}

#[derive(Clone)]
struct MediaLiveRegion {
	adapter: Rc<RefCell<SubclassingAdapter>>,
	last_announcement: Rc<RefCell<Option<String>>>,
}

impl MediaLiveRegion {
	fn new(frame: &Frame) -> Self {
		let hwnd = HWND(frame.get_handle() as *mut _);
		let last_announcement = Rc::new(RefCell::new(None::<String>));
		let adapter = SubclassingAdapter::new(hwnd, MediaActivationHandler, MediaActionHandler);
		Self { adapter: Rc::new(RefCell::new(adapter)), last_announcement }
	}

	fn announce(&self, text: &str) {
		let mut new_text = text.to_string();
		let mut last = self.last_announcement.borrow_mut();
		if let Some(old) = last.as_ref() {
			if *old == new_text {
				new_text.push('\u{00A0}');
			}
		}
		*last = Some(new_text.clone());

		let mut node = Node::new(Role::Label);
		node.set_value(new_text);
		node.set_live(accesskit::Live::Polite);

		let mut root = Node::new(Role::Window);
		root.set_children(vec![LR_ANNOUNCEMENT_ID]);

		let update = TreeUpdate {
			nodes: vec![(LR_ANNOUNCEMENT_ID, node), (LR_ROOT_ID, root)],
			tree: None,
			focus: LR_ROOT_ID,
			tree_id: accesskit::TreeId::ROOT,
		};
		let mut adapter = self.adapter.borrow_mut();
		if let Some(events) = adapter.update_if_active(|| update) {
			events.raise();
		}
	}
}

/// Shared state for a media file being downloaded to a local temp file in
/// the background, so its progress can be reported while the player window
/// waits for it to finish. Playback only starts once the whole file has
/// arrived: decoding or seeking a file that's still being written to made
/// fast seeking freeze the player (a seek past what had downloaded so far
/// blocked rodio's whole audio thread waiting for more bytes), so nothing
/// touches the file for playback until it's complete.
struct DownloadProgress {
	downloaded: AtomicU64,
	total: AtomicU64,
	/// Set once the background download thread has stopped, for any reason.
	done: AtomicBool,
	/// Ask the background download thread to stop early.
	cancel: AtomicBool,
	error: Mutex<Option<String>>,
	dest: PathBuf,
}

impl Drop for DownloadProgress {
	fn drop(&mut self) {
		// The last reference to this is only ever dropped once nothing needs
		// the file anymore, including the download thread itself (it holds
		// its own clone until it exits), so it's always safe to delete here.
		let _ = std::fs::remove_file(&self.dest);
	}
}

/// Downloads `url` to `progress.dest` on a background thread, updating
/// `progress` as it goes. Returns immediately; does not wait for completion.
/// Nothing reads `progress.dest` back until `progress.done` is set, so
/// there's no race with this thread creating the file.
fn start_background_download(url: String, progress: Arc<DownloadProgress>) {
	thread::spawn(move || {
		let result: anyhow::Result<()> = (|| {
			let client = reqwest::blocking::Client::builder().user_agent("Fedra/0.1").build()?;
			let mut resp = client.get(&url).send()?.error_for_status()?;
			progress.total.store(resp.content_length().unwrap_or(0), Ordering::Release);
			let mut file = std::fs::File::create(&progress.dest)?;
			let mut buf = vec![0u8; 65536];
			let mut current_downloaded = 0u64;
			loop {
				if progress.cancel.load(Ordering::Acquire) {
					return Ok(());
				}
				let n = std::io::Read::read(&mut resp, &mut buf)?;
				if n == 0 {
					break;
				}
				std::io::Write::write_all(&mut file, &buf[..n])?;
				current_downloaded += n as u64;
				progress.downloaded.store(current_downloaded, Ordering::Release);
			}
			Ok(())
		})();
		if let Err(e) = result {
			*progress.error.lock().unwrap() = Some(e.to_string());
		}
		progress.done.store(true, Ordering::Release);
	});
}

/// The result of successfully decoding a fully-downloaded media file on a
/// background thread. Everything in here is `Send` so it can cross back to
/// the UI thread via `call_after`; the `AudioOutput`/`Player` are only ever
/// created on the UI thread afterward.
struct DecodedSource {
	decoder: rodio::Decoder<std::io::BufReader<std::fs::File>>,
	total_duration: Option<Duration>,
	progress: Arc<DownloadProgress>,
}

/// Builds a decoder against `progress`'s destination file. Only call this
/// once `progress.done` is set: it doesn't wait, and always reads the file
/// as a complete, static file rather than one still being written to.
fn build_decoded_source(progress: Arc<DownloadProgress>) -> Result<DecodedSource, String> {
	let error = progress.error.lock().unwrap().clone();
	if let Some(err) = error {
		return Err(err);
	}
	let file = std::fs::File::open(&progress.dest).map_err(|e| e.to_string())?;
	let decoder = rodio::Decoder::try_from(file).map_err(|e| format!("Could not decode media: {e}"))?;
	let total_duration = decoder.total_duration();
	Ok(DecodedSource { decoder, total_duration, progress })
}

/// A rodio player set up to play a fully-downloaded media attachment.
struct PlaybackSession {
	// Kept alive for as long as the session lives; dropping it stops playback.
	_output: audio::AudioOutput,
	player: rodio::Player,
	total_duration: Option<Duration>,
	progress: Arc<DownloadProgress>,
}

impl Drop for PlaybackSession {
	fn drop(&mut self) {
		self.progress.cancel.store(true, Ordering::Release);
	}
}

/// Either still waiting for enough of the attachment to arrive to start
/// decoding it, or fully set up and ready to play.
enum PlayerState {
	Loading(Arc<DownloadProgress>),
	Ready(PlaybackSession),
}

impl Drop for PlayerState {
	fn drop(&mut self) {
		if let Self::Loading(progress) = self {
			progress.cancel.store(true, Ordering::Release);
		}
	}
}

/// What happened to a download started by [`spawn_progress_download`].
enum DownloadOutcome {
	Success,
	Cancelled,
	Failed(String),
}

/// Downloads `url` to `dest` on a background thread, showing a cancellable
/// progress dialog parented on `frame`. `on_done` runs on the UI thread once
/// the download finishes, is cancelled, or fails; it is responsible for any
/// cleanup of `dest` that the outcome calls for.
///
/// This is used for the explicit "save a copy of this attachment" command.
/// Playback itself uses the simpler [`start_background_download`], since it
/// reports progress through the player window itself rather than a dialog.
fn spawn_progress_download(
	frame: &Frame,
	url: String,
	dest: PathBuf,
	title: &str,
	message: &str,
	on_done: impl FnOnce(DownloadOutcome) + 'static,
) {
	let progress = ProgressDialog::builder(frame, title, message, 100)
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
			ui_call_after(move || {
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
			});
			thread::sleep(Duration::from_millis(200));
		}
	});

	let task_id = NEXT_TASK_ID.fetch_add(1, Ordering::Relaxed);
	ACTIVE_DOWNLOAD_DONE.with(|d| {
		d.borrow_mut().insert(task_id, Box::new(on_done));
	});

	let d_downloaded = downloaded;
	let d_total = total;
	let d_is_running = is_running;
	let d_cancelled = cancelled;
	let dest_thread = dest;
	thread::spawn(move || {
		let result: anyhow::Result<()> = (|| {
			let client = reqwest::blocking::Client::builder().user_agent("Fedra/0.1").build()?;
			let mut resp = client.get(&url).send()?.error_for_status()?;
			let total_size = resp.content_length().unwrap_or(0);
			d_total.store(total_size, Ordering::Relaxed);
			let mut file = std::fs::File::create(&dest_thread)?;
			let mut buf = [0u8; 8192];
			let mut current_downloaded = 0u64;
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
		let outcome = match result {
			Ok(()) => DownloadOutcome::Success,
			Err(e) if e.to_string() == "Download cancelled" => DownloadOutcome::Cancelled,
			Err(e) => DownloadOutcome::Failed(e.to_string()),
		};
		ui_call_after(move || {
			ACTIVE_PROGRESS.with(|p| {
				*p.borrow_mut() = None;
			});
			let done = ACTIVE_DOWNLOAD_DONE.with(|d| d.borrow_mut().remove(&task_id));
			if let Some(done) = done {
				done(outcome);
			}
		});
	});
}

/// Starts a background ticker that periodically reports `progress`'s
/// download counters (until `still_loading` is cleared or the download
/// ends) to whatever callback is registered under `id` in [`ACTIVE_TICKS`].
/// If the download finishes before `still_loading` is cleared (decoding
/// hasn't caught up yet), sends one last [`TickerUpdate::Finishing`].
fn spawn_loading_ticker(id: usize, progress: Arc<DownloadProgress>, still_loading: Arc<AtomicBool>) {
	thread::spawn(move || {
		while still_loading.load(Ordering::Acquire) && !progress.done.load(Ordering::Acquire) {
			let downloaded = progress.downloaded.load(Ordering::Acquire);
			let total = progress.total.load(Ordering::Acquire);
			ui_call_after(move || {
				ACTIVE_TICKS.with(|t| {
					if let Some(cb) = t.borrow().get(&id) {
						cb(TickerUpdate::Downloading { downloaded, total });
					}
				});
			});
			thread::sleep(Duration::from_millis(250));
		}
		if still_loading.load(Ordering::Acquire) {
			ui_call_after(move || {
				ACTIVE_TICKS.with(|t| {
					if let Some(cb) = t.borrow().get(&id) {
						cb(TickerUpdate::Finishing);
					}
				});
			});
		}
		ui_call_after(move || {
			ACTIVE_TICKS.with(|t| {
				t.borrow_mut().remove(&id);
			});
		});
	});
}

pub fn show_media_player(_parent: &dyn WxWidget, url: String, _access_token: Option<String>) {
	const ID_PLAY_PAUSE: i32 = 10001;
	const ID_SEEK_BACK: i32 = 10002;
	const ID_SEEK_FWD: i32 = 10003;
	const ID_VOL_UP: i32 = 10004;
	const ID_VOL_DOWN: i32 = 10005;
	const ID_DOWNLOAD: i32 = 10006;
	const ID_CLOSE: i32 = 10007;
	const ID_ELAPSED: i32 = 10008;
	const ID_REMAINING: i32 = 10009;
	const ID_TOTAL: i32 = 10010;

	let frame = Frame::builder().with_title("Media Player").with_size(Size::new(480, 200)).build();
	let lr = MediaLiveRegion::new(&frame);
	let panel = Panel::builder(&frame).build();
	let status_label = StaticText::builder(&panel).with_label("Loading media...").build();
	let panel_sizer = BoxSizer::builder(Orientation::Vertical).build();
	panel_sizer.add(&status_label, 1, SizerFlag::Expand | SizerFlag::All, 10);
	panel.set_sizer(panel_sizer, true);
	let frame_sizer = BoxSizer::builder(Orientation::Vertical).build();
	frame_sizer.add(&panel, 1, SizerFlag::Expand, 0);
	frame.set_sizer(frame_sizer, true);

	let menu = Menu::builder()
		.append_item(ID_PLAY_PAUSE, "Play/Pause\tSpace", "Play or pause the media")
		.append_item(ID_SEEK_BACK, "Seek Backward\tLeft", "Seek backward 10 seconds")
		.append_item(ID_SEEK_FWD, "Seek Forward\tRight", "Seek forward 10 seconds")
		.append_item(ID_VOL_UP, "Volume Up\tUp", "Increase volume")
		.append_item(ID_VOL_DOWN, "Volume Down\tDown", "Decrease volume")
		.append_separator()
		.append_item(ID_ELAPSED, "Elapsed Time\tE", "Announce elapsed time")
		.append_item(ID_REMAINING, "Remaining Time\tR", "Announce remaining time")
		.append_item(ID_TOTAL, "Total Time\tT", "Announce total duration")
		.append_separator()
		.append_item(ID_DOWNLOAD, "Download\tD", "Download this media file")
		.append_separator()
		.append_item(ID_CLOSE, "Close\tEscape", "Close media player")
		.build();
	let menu_bar = MenuBar::builder().append(menu, "&Playback").build();
	frame.set_menu_bar(menu_bar);

	let temp_path = std::env::temp_dir().join(format!(
		"fedra-media-{}-{}.tmp",
		std::process::id(),
		std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or_default(),
	));
	let progress = Arc::new(DownloadProgress {
		downloaded: AtomicU64::new(0),
		total: AtomicU64::new(0),
		done: AtomicBool::new(false),
		cancel: AtomicBool::new(false),
		error: Mutex::new(None),
		dest: temp_path,
	});
	start_background_download(url.clone(), progress.clone());

	let state: Rc<RefCell<Option<PlayerState>>> = Rc::new(RefCell::new(Some(PlayerState::Loading(progress.clone()))));
	let still_loading = Arc::new(AtomicBool::new(true));

	let ticker_id = NEXT_TASK_ID.fetch_add(1, Ordering::Relaxed);
	ACTIVE_TICKS.with(|t| {
		t.borrow_mut().insert(ticker_id, {
			let frame = frame.clone();
			Box::new(move |update: TickerUpdate| {
				if !frame.is_valid() {
					return;
				}
				let text = match update {
					TickerUpdate::Downloading { downloaded, total } if total > 0 => {
						let percent = (downloaded.saturating_mul(100) / total).min(100);
						format!("Loading media... {percent}%")
					}
					TickerUpdate::Downloading { .. } => "Loading media...".to_string(),
					TickerUpdate::Finishing => "Loading media... almost ready".to_string(),
				};
				status_label.set_label(&text);
			})
		});
	});
	spawn_loading_ticker(ticker_id, progress.clone(), still_loading.clone());

	frame.on_menu_selected({
		let state = state.clone();
		let frm = frame.clone();
		move |event| match event.get_id() {
			ID_PLAY_PAUSE => {
				with_session(&state, &lr, |s| {
					if s.player.empty() {
						// Reached the end: start over from the beginning
						// rather than doing nothing.
						match std::fs::File::open(&s.progress.dest).ok().and_then(|f| rodio::Decoder::try_from(f).ok()) {
							Some(decoder) => {
								s.player.append(decoder);
								s.player.play();
							}
							None => lr.announce("Could not restart media"),
						}
					} else if s.player.is_paused() {
						s.player.play();
					} else {
						s.player.pause();
					}
				});
			}
			ID_SEEK_BACK => {
				with_session(&state, &lr, |s| {
					let pos = s.player.get_pos().saturating_sub(Duration::from_secs(10));
					let _ = s.player.try_seek(pos);
				});
			}
			ID_SEEK_FWD => {
				with_session(&state, &lr, |s| {
					let pos = s.player.get_pos() + Duration::from_secs(10);
					let _ = s.player.try_seek(pos);
				});
			}
			ID_VOL_UP => {
				with_session(&state, &lr, |s| {
					let v = (s.player.volume() + 0.1).min(1.0);
					s.player.set_volume(v);
				});
			}
			ID_VOL_DOWN => {
				with_session(&state, &lr, |s| {
					let v = (s.player.volume() - 0.1).max(0.0);
					s.player.set_volume(v);
				});
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
						let path = PathBuf::from(path);
						let target = frm.clone();
						let cleanup_path = path.clone();
						spawn_progress_download(
							&frm,
							url.clone(),
							path,
							"Downloading Media",
							"Downloading media...",
							move |outcome| match outcome {
								DownloadOutcome::Success => {
									if target.is_valid() {
										let dlg = MessageDialog::builder(&target, "Download complete.", "Fedra")
											.with_style(MessageDialogStyle::OK | MessageDialogStyle::IconInformation)
											.build();
										dlg.show_modal();
										dlg.destroy();
										target.set_focus();
									}
								}
								DownloadOutcome::Cancelled => {
									let _ = std::fs::remove_file(&cleanup_path);
								}
								DownloadOutcome::Failed(e) => {
									if target.is_valid() {
										let msg = format!("Failed to download media: {e}");
										let dlg = MessageDialog::builder(&target, &msg, "Fedra")
											.with_style(MessageDialogStyle::OK | MessageDialogStyle::IconError)
											.build();
										dlg.show_modal();
										dlg.destroy();
										target.set_focus();
									}
								}
							},
						);
					}
				}
				dialog.destroy();
			}
			ID_ELAPSED => {
				with_session(&state, &lr, |s| {
					let pos = s.player.get_pos();
					lr.announce(&format!("Elapsed: {}", format_duration(pos)));
				});
			}
			ID_REMAINING => {
				with_session(&state, &lr, |s| match s.total_duration {
					Some(total) => {
						let remaining = total.saturating_sub(s.player.get_pos());
						lr.announce(&format!("Remaining: {}", format_duration(remaining)));
					}
					None => lr.announce("Remaining time is unknown"),
				});
			}
			ID_TOTAL => {
				with_session(&state, &lr, |s| match s.total_duration {
					Some(total) => lr.announce(&format!("Total: {}", format_duration(total))),
					None => lr.announce("Total time is unknown"),
				});
			}
			ID_CLOSE => {
				frm.close(true);
			}
			_ => {}
		}
	});

	frame.show(true);
	// Deliberately focus the frame, not status_label: a screen reader treats
	// the focused control's text as live and re-speaks it on every change
	// (and sometimes just on a keypress), which was making every loading
	// percentage tick, "almost ready", and "Ready" all get spoken on their
	// own even though nothing calls announce() for them. The label stays
	// visible for sighted users; it's just not what's focused.
	frame.set_focus();

	let load_id = NEXT_TASK_ID.fetch_add(1, Ordering::Relaxed);
	ACTIVE_LOAD_DONE.with(|d| {
		d.borrow_mut().insert(load_id, {
			let frm = frame.clone();
			Box::new(move |result: Result<DecodedSource, String>| {
				still_loading.store(false, Ordering::Release);
				if !frm.is_valid() {
					return;
				}
				let outcome = result.and_then(|decoded| {
					audio::AudioOutput::open().map(|output| (output, decoded)).map_err(|e| format!("Could not open audio output: {e}"))
				});
				match outcome {
					Ok((output, decoded)) => {
						let player = rodio::Player::connect_new(output.mixer());
						// Wait for the user to press play rather than starting immediately.
						player.pause();
						player.append(decoded.decoder);
						let session = PlaybackSession {
							_output: output,
							player,
							total_duration: decoded.total_duration,
							progress: decoded.progress,
						};
						// Deliberately silent: this happens on its own, without the
						// user asking for it, so it only updates the visible label.
						// The live region stays quiet until the user actually tries
						// to do something (e.g. presses Space) and needs to know
						// whether that worked.
						status_label.set_label("Ready. Press Space to play or pause.");
						*state.borrow_mut() = Some(PlayerState::Ready(session));
					}
					Err(e) => {
						show_media_load_error(&frm, &e);
						*state.borrow_mut() = None;
						frm.close(true);
					}
				}
			})
		});
	});

	thread::spawn(move || {
		while !progress.done.load(Ordering::Acquire) {
			thread::sleep(Duration::from_millis(20));
		}
		let result = build_decoded_source(progress);
		ui_call_after(move || {
			let done = ACTIVE_LOAD_DONE.with(|d| d.borrow_mut().remove(&load_id));
			if let Some(done) = done {
				done(result);
			}
		});
	});
}

fn with_session(state: &Rc<RefCell<Option<PlayerState>>>, lr: &MediaLiveRegion, f: impl FnOnce(&PlaybackSession)) {
	match state.borrow().as_ref() {
		Some(PlayerState::Ready(s)) => f(s),
		_ => lr.announce("Media is still loading"),
	}
}

fn show_media_load_error(frame: &Frame, detail: &str) {
	let msg = format!(
		"Fedra could not load this attachment: {detail}\n\nThe media may be unavailable or its format may not be \
		supported. Try opening the attachment in your browser."
	);
	let dlg = MessageDialog::builder(frame, &msg, "Media Player Error")
		.with_style(MessageDialogStyle::OK | MessageDialogStyle::IconError)
		.build();
	dlg.show_modal();
	dlg.destroy();
}

fn format_duration(duration: Duration) -> String {
	let total_secs = duration.as_secs();
	let hours = total_secs / 3600;
	let minutes = (total_secs % 3600) / 60;
	let seconds = total_secs % 60;

	let mut parts = Vec::new();

	if hours > 0 {
		parts.push(if hours == 1 { "1 hour".to_string() } else { format!("{hours} hours") });
	}

	if minutes > 0 {
		parts.push(if minutes == 1 { "1 minute".to_string() } else { format!("{minutes} minutes") });
	}

	parts.push(if seconds == 1 { "1 second".to_string() } else { format!("{seconds} seconds") });

	parts.join(", ")
}
