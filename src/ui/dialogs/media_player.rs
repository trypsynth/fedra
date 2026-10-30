use std::{
	cell::{Cell, RefCell},
	collections::HashMap,
	io::{Read, Write},
	path::{Path, PathBuf},
	rc::Rc,
	sync::{
		Arc,
		atomic::{AtomicBool, AtomicUsize, Ordering},
	},
	thread::{self, LocalKey},
	time::{Duration, Instant},
};

use rodio::Source;
use url::Url;
use wx_utils::{
	format_duration_seconds,
	progress::{Ended, run_with_progress},
};
use wxdragon::prelude::*;

use crate::audio;
#[cfg(target_os = "macos")]
use crate::ui::keys;

// Background threads never touch wxWidgets handles or Rc-based state directly, since those aren't
// `Send`. They only send plain `Send` data to the main thread via `wxdragon::call_after`, and the
// callbacks that DO close over widgets/Rc state are registered here, keyed by task id, so they're
// only ever stored and invoked on the main thread.
type LoadCallbacks<T> = RefCell<HashMap<usize, Box<dyn FnMut(Load<T>)>>>;

thread_local! {
	static MEDIA_LOADS: LoadCallbacks<DecodedSource> = RefCell::new(HashMap::new());
	static IMAGE_LOADS: LoadCallbacks<DecodedImage> = RefCell::new(HashMap::new());
}

/// What a background load reports to the window waiting on it.
enum Load<T> {
	Downloading {
		downloaded: u64,
		total: u64,
	},
	/// The download itself finished, but decoding hasn't caught up yet (for
	/// some formats, working out an accurate duration means scanning a good
	/// chunk of the file). Without this, the status label would sit on
	/// whatever download percentage it last showed, which is rarely a clean
	/// 100%, making it look stuck or wrong right when the file is actually
	/// fully there.
	Finishing,
	Done(Result<T, String>),
}

const PROGRESS_INTERVAL: Duration = Duration::from_millis(250);

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

#[cfg(windows)]
mod announcer {
	use std::{cell::RefCell, rc::Rc};

	use accesskit::{ActionHandler, ActionRequest, ActivationHandler, Node, NodeId, Role, TreeInfo, TreeUpdate};
	use accesskit_windows::SubclassingAdapter;
	use windows::Win32::Foundation::HWND;
	use wxdragon::prelude::*;

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
	pub(super) struct MediaLiveRegion {
		adapter: Rc<RefCell<SubclassingAdapter>>,
		last_announcement: Rc<RefCell<Option<String>>>,
	}

	impl MediaLiveRegion {
		pub(super) fn new(frame: &Frame) -> Self {
			let hwnd = HWND(frame.get_handle().cast());
			let last_announcement = Rc::new(RefCell::new(None::<String>));
			let adapter = SubclassingAdapter::new(hwnd, MediaActivationHandler, MediaActionHandler);
			Self { adapter: Rc::new(RefCell::new(adapter)), last_announcement }
		}

		pub(super) fn announce(&self, text: &str) {
			let mut new_text = text.to_string();
			let mut last = self.last_announcement.borrow_mut();
			if let Some(old) = last.as_ref()
				&& *old == new_text
			{
				new_text.push('\u{00A0}');
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
}

#[cfg(windows)]
use announcer::MediaLiveRegion;

#[cfg(not(windows))]
#[derive(Clone)]
struct MediaLiveRegion;

#[cfg(not(windows))]
impl MediaLiveRegion {
	const fn new(_frame: &Frame) -> Self {
		Self
	}

	#[allow(clippy::unused_self, reason = "a method, like the Windows version")]
	fn announce(&self, text: &str) {
		crate::speech::speak(text);
	}
}

/// A media file downloaded to a temp file, deleted once nothing needs it.
/// Playback only starts once the whole file has arrived: decoding or seeking
/// a file that's still being written to made fast seeking freeze the player
/// (a seek past what had downloaded so far blocked rodio's whole audio
/// thread waiting for more bytes), so nothing touches the file for playback
/// until it's complete.
struct TempMedia {
	path: PathBuf,
	cancel: AtomicBool,
}

impl Drop for TempMedia {
	fn drop(&mut self) {
		let _ = std::fs::remove_file(&self.path);
	}
}

/// A temp file path that won't collide with another attachment being
/// downloaded at the same time (e.g. an image viewer and a media player open
/// together).
fn unique_temp_path() -> PathBuf {
	std::env::temp_dir().join(format!(
		"fedra-media-{}-{}.tmp",
		std::process::id(),
		std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or_default(),
	))
}

/// Downloads `url` to `dest`, reporting the bytes so far and the total (0 if unknown) after each
/// chunk. Fails once `cancel` is set, so nothing goes on to use the partial file.
fn download(url: &str, dest: &Path, cancel: &AtomicBool, mut report: impl FnMut(u64, u64)) -> anyhow::Result<()> {
	let client = reqwest::blocking::Client::builder().user_agent("Fedra/0.1").build()?;
	let mut resp = client.get(url).send()?.error_for_status()?;
	let total = resp.content_length().unwrap_or(0);
	let mut file = std::fs::File::create(dest)?;
	let mut buf = vec![0u8; 65536];
	let mut downloaded = 0u64;
	loop {
		if cancel.load(Ordering::Acquire) {
			anyhow::bail!("Download cancelled");
		}
		let n = resp.read(&mut buf)?;
		if n == 0 {
			return Ok(());
		}
		file.write_all(&buf[..n])?;
		downloaded += n as u64;
		report(downloaded, total);
	}
}

/// Downloads `url` to a temp file and decodes it with `decode` on one background thread, passing
/// progress and then the result to `on_update` on the UI thread. Closing `frame` cancels it.
fn start_load<T: Send + 'static>(
	loads: &'static LocalKey<LoadCallbacks<T>>,
	frame: &Frame,
	url: String,
	decode: fn(Arc<TempMedia>) -> Result<T, String>,
	on_update: impl FnMut(Load<T>) + 'static,
) {
	let media = Arc::new(TempMedia { path: unique_temp_path(), cancel: AtomicBool::new(false) });
	let cancel_media = Arc::clone(&media);
	frame.on_destroy(move |_| cancel_media.cancel.store(true, Ordering::Release));
	let id = NEXT_TASK_ID.fetch_add(1, Ordering::Relaxed);
	loads.with_borrow_mut(|callbacks| callbacks.insert(id, Box::new(on_update)));
	let send = move |update: Load<T>| {
		ui_call_after(move || {
			// Taken out while it runs, since showing an error runs a nested event loop.
			let done = matches!(update, Load::Done(_));
			let Some(mut callback) = loads.with_borrow_mut(|callbacks| callbacks.remove(&id)) else { return };
			callback(update);
			if !done {
				loads.with_borrow_mut(|callbacks| callbacks.insert(id, callback));
			}
		});
	};
	thread::spawn(move || {
		let mut last_report = Instant::now();
		let downloaded = download(&url, &media.path, &media.cancel, |downloaded, total| {
			if last_report.elapsed() >= PROGRESS_INTERVAL {
				last_report = Instant::now();
				send(Load::Downloading { downloaded, total });
			}
		});
		let result = match downloaded {
			Ok(()) => {
				send(Load::Finishing);
				decode(media)
			}
			Err(e) => Err(e.to_string()),
		};
		send(Load::Done(result));
	});
}

/// The status label text for a load's progress.
fn loading_label<T>(what: &str, update: &Load<T>) -> String {
	match update {
		Load::Downloading { downloaded, total } if *total > 0 => {
			let percent = (downloaded.saturating_mul(100) / total).min(100);
			format!("Loading {what}... {percent}%")
		}
		Load::Finishing => format!("Loading {what}... almost ready"),
		_ => format!("Loading {what}..."),
	}
}

/// The result of successfully decoding a fully-downloaded media file on a
/// background thread. Everything in here is `Send` so it can cross back to
/// the UI thread via `call_after`; the `AudioOutput`/`Player` are only ever
/// created on the UI thread afterward.
struct DecodedSource {
	decoder: rodio::Decoder<std::io::BufReader<std::fs::File>>,
	total_duration: Option<Duration>,
	media: Arc<TempMedia>,
}

fn build_decoded_source(media: Arc<TempMedia>) -> Result<DecodedSource, String> {
	let file = std::fs::File::open(&media.path).map_err(|e| e.to_string())?;
	let decoder = rodio::Decoder::try_from(file).map_err(|_| "Could not play that file".to_string())?;
	let total_duration = decoder.total_duration();
	Ok(DecodedSource { decoder, total_duration, media })
}

/// A rodio player set up to play a fully-downloaded media attachment.
struct PlaybackSession {
	// Kept alive for as long as the session lives; dropping it stops playback.
	_output: audio::AudioOutput,
	player: rodio::Player,
	total_duration: Option<Duration>,
	media: Arc<TempMedia>,
}

/// Either still loading the attachment, or fully set up and ready to play.
enum PlayerState {
	Loading,
	Ready(Box<PlaybackSession>),
}

/// Prompts for a save location and downloads `url` there, showing progress
/// and a completion dialog parented on `frame`. Used by the "Download this
/// media file" command in both the player and the image viewer.
fn download_to_user_file(frame: &Frame, url: &str) {
	let default_file = Url::parse(url).map_or_else(
		|_| "media".to_string(),
		|u| {
			u.path_segments()
				.and_then(|mut segments| segments.next_back())
				.filter(|s| !s.is_empty())
				.unwrap_or("media")
				.to_string()
		},
	);
	let dialog = FileDialog::builder(frame)
		.with_message("Save Media As")
		.with_default_file(&default_file)
		.with_style(FileDialogStyle::Save | FileDialogStyle::OverwritePrompt)
		.build();
	if dialog.show_modal() != ID_OK {
		return;
	}
	let Some(path) = dialog.get_path().map(PathBuf::from) else { return };
	let url = url.to_string();
	let target = *frame;
	let work_path = path.clone();
	run_with_progress(
		frame,
		"Downloading Media",
		"Downloading media...",
		move |progress| {
			download(&url, &work_path, progress.cancel_flag(), |done, total| progress.set(done, Some(total)))
				.map_err(|e| e.to_string())
		},
		move |result, ended| {
			if ended == Ended::Cancelled {
				let _ = std::fs::remove_file(&path);
				return;
			}
			if !target.is_valid() {
				return;
			}
			let (msg, style) = match result {
				Ok(()) => ("Download complete.".to_string(), MessageDialogStyle::IconInformation),
				Err(e) => (format!("Failed to download media: {e}"), MessageDialogStyle::IconError),
			};
			MessageDialog::builder(&target, &msg, "Fedra")
				.with_style(MessageDialogStyle::OK | style)
				.build()
				.show_modal();
			target.set_focus();
		},
	);
}

pub fn show_media_player(parent: &impl WxWidget, url: String, kind: &str, _access_token: Option<String>) {
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
	const ID_SEEK_START: i32 = 10011;
	const ID_SEEK_END: i32 = 10012;
	if kind.eq_ignore_ascii_case("image") {
		show_image_viewer(parent, url);
		return;
	}
	let frame = Frame::builder().with_parent(parent).with_title("Media Player").with_size(Size::new(480, 200)).build();
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
	#[cfg(target_os = "macos")]
	crate::ui::mac_shortcuts::install_menu_shortcuts(
		frame,
		&[
			(keys::SPACE, ID_PLAY_PAUSE),
			(keys::LEFT, ID_SEEK_BACK),
			(keys::RIGHT, ID_SEEK_FWD),
			(keys::UP, ID_VOL_UP),
			(keys::DOWN, ID_VOL_DOWN),
			('E' as i32, ID_ELAPSED),
			('R' as i32, ID_REMAINING),
			('T' as i32, ID_TOTAL),
			('D' as i32, ID_DOWNLOAD),
			(keys::ESCAPE, ID_CLOSE),
		],
	);
	let state: Rc<RefCell<Option<PlayerState>>> = Rc::new(RefCell::new(Some(PlayerState::Loading)));
	// Set when Space is pressed before the media is ready, so playback starts
	// on its own once loading finishes instead of the press being dropped and
	// the user having to guess when to try again.
	let play_when_ready = Rc::new(Cell::new(false));
	start_load(&MEDIA_LOADS, &frame, url.clone(), build_decoded_source, {
		let state = state.clone();
		let play_when_ready = play_when_ready.clone();
		let frm = frame;
		move |update| {
			if !frm.is_valid() {
				return;
			}
			let Load::Done(result) = update else {
				status_label.set_label(&loading_label("media", &update));
				return;
			};
			let outcome = result.and_then(|decoded| {
				audio::AudioOutput::open()
					.map(|output| (output, decoded))
					.map_err(|e| format!("Could not open audio output: {e}"))
			});
			match outcome {
				Ok((output, decoded)) => {
					let player = rodio::Player::connect_new(output.mixer());
					// Wait for the user to press play rather than starting immediately,
					// unless they already pressed it while loading.
					player.pause();
					player.append(decoded.decoder);
					if play_when_ready.get() {
						player.play();
					}
					let session = PlaybackSession {
						_output: output,
						player,
						total_duration: decoded.total_duration,
						media: decoded.media,
					};
					// Deliberately silent: this happens on its own, without the
					// user asking for it, so it only updates the visible label.
					// The live region stays quiet until the user actually tries
					// to do something (e.g. presses Space) and needs to know
					// whether that worked.
					status_label.set_label("Ready. Press Space to play or pause.");
					*state.borrow_mut() = Some(PlayerState::Ready(Box::new(session)));
				}
				Err(e) => {
					show_media_load_error(&frm, &e);
					*state.borrow_mut() = None;
					frm.close(true);
				}
			}
		}
	});
	frame.on_menu_selected({
		let frm = frame;
		move |event| match event.get_id() {
			ID_PLAY_PAUSE => {
				if matches!(state.borrow().as_ref(), Some(PlayerState::Loading)) {
					let queued = !play_when_ready.get();
					play_when_ready.set(queued);
					lr.announce(if queued { "Queued" } else { "Unqueued" });
					return;
				}
				with_session(&state, &lr, |s| {
					if s.player.empty() {
						// Reached the end: start over from the beginning
						// rather than doing nothing.
						match std::fs::File::open(&s.media.path).ok().and_then(|f| rodio::Decoder::try_from(f).ok()) {
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
			ID_SEEK_START => {
				with_session(&state, &lr, |s| {
					let _ = s.player.try_seek(Duration::ZERO);
				});
			}
			ID_SEEK_END => {
				with_session(&state, &lr, |s| match s.total_duration {
					Some(total) => {
						let _ = s.player.try_seek(total);
					}
					None => lr.announce("Total time is unknown"),
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
				download_to_user_file(&frm, &url);
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
}

/// The result of successfully decoding a fully-downloaded image attachment
/// on a background thread. Plain pixel data, so it's `Send` and can cross
/// back to the UI thread via `call_after`; the `wxdragon::Bitmap` is only
/// ever created on the UI thread afterward (`Bitmap` isn't `Send`).
struct DecodedImage {
	rgba: Vec<u8>,
	width: u32,
	height: u32,
}

#[allow(clippy::needless_pass_by_value, reason = "matches the decoder signature start_load takes")]
fn decode_image(media: Arc<TempMedia>) -> Result<DecodedImage, String> {
	let bytes = std::fs::read(&media.path).map_err(|e| e.to_string())?;
	let rgba = image::load_from_memory(&bytes).map_err(|e| format!("Could not decode image: {e}"))?.to_rgba8();
	let (width, height) = rgba.dimensions();
	Ok(DecodedImage { rgba: rgba.into_raw(), width, height })
}

/// Downloads and displays a static image attachment. There's nothing to
/// play, so this is a much smaller cousin of [`show_media_player`]: just a
/// download, a decode, and a picture, plus the same "save a copy" command.
fn show_image_viewer(parent: &impl WxWidget, url: String) {
	const ID_DOWNLOAD: i32 = 10006;
	const ID_CLOSE: i32 = 10007;
	let frame = Frame::builder().with_parent(parent).with_title("Media Viewer").with_size(Size::new(480, 200)).build();
	let panel = Panel::builder(&frame).build();
	let status_label = StaticText::builder(&panel).with_label("Loading image...").build();
	let panel_sizer = BoxSizer::builder(Orientation::Vertical).build();
	panel_sizer.add(&status_label, 1, SizerFlag::Expand | SizerFlag::All, 10);
	panel.set_sizer(panel_sizer, true);
	let frame_sizer = BoxSizer::builder(Orientation::Vertical).build();
	frame_sizer.add(&panel, 1, SizerFlag::Expand, 0);
	frame.set_sizer(frame_sizer, true);
	let menu = Menu::builder()
		.append_item(ID_DOWNLOAD, "Download\tD", "Download this image file")
		.append_separator()
		.append_item(ID_CLOSE, "Close\tEscape", "Close media viewer")
		.build();
	let menu_bar = MenuBar::builder().append(menu, "&Image").build();
	frame.set_menu_bar(menu_bar);
	#[cfg(target_os = "macos")]
	crate::ui::mac_shortcuts::install_menu_shortcuts(frame, &[('D' as i32, ID_DOWNLOAD), (keys::ESCAPE, ID_CLOSE)]);
	frame.on_menu_selected({
		let frm = frame;
		let url = url.clone();
		move |event| match event.get_id() {
			ID_DOWNLOAD => {
				download_to_user_file(&frm, &url);
			}
			ID_CLOSE => {
				frm.close(true);
			}
			_ => {}
		}
	});
	frame.show(true);
	// See show_media_player: focusing the frame, not the label, avoids the
	// screen reader re-speaking every status text change on its own.
	frame.set_focus();
	start_load(&IMAGE_LOADS, &frame, url, decode_image, {
		let frm = frame;
		move |update| {
			if !frm.is_valid() {
				return;
			}
			let Load::Done(result) = update else {
				status_label.set_label(&loading_label("image", &update));
				return;
			};
			match result {
				Ok(decoded) => {
					if let Some(bitmap) = Bitmap::from_rgba(&decoded.rgba, decoded.width, decoded.height) {
						// Cap the window to something reasonable; AspectFit
						// keeps larger images from being stretched or cropped.
						const MAX_DIM: i32 = 1000;
						let w = i32::try_from(decoded.width).unwrap_or(MAX_DIM).min(MAX_DIM);
						let h = i32::try_from(decoded.height).unwrap_or(MAX_DIM).min(MAX_DIM);
						status_label.destroy();
						let bitmap_view = StaticBitmap::builder(&panel)
							.with_bitmap(Some(bitmap))
							.with_scale_mode(Some(ScaleMode::AspectFit))
							.with_size(Size::new(w, h))
							.build();
						panel_sizer.add(&bitmap_view, 1, SizerFlag::Expand | SizerFlag::All, 0);
						panel.layout();
						frm.set_client_size(Size::new(w, h));
						frm.centre();
					} else {
						show_media_load_error(&frm, "Could not display this image.");
						frm.close(true);
					}
				}
				Err(e) => {
					show_media_load_error(&frm, &e);
					frm.close(true);
				}
			}
		}
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
}

fn format_duration(duration: Duration) -> String {
	format_duration_seconds(duration.as_secs())
}
