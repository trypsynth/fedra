#![warn(clippy::all, clippy::pedantic, clippy::nursery)]
// Still allowed: 9 functions take more than 7 arguments and 26 run past 100
// lines, nearly all of them dialog builders. Splitting those up is a refactor
// rather than a lint fix. To see the list:
//   cargo clippy --all-targets -- --force-warn clippy::too_many_arguments --force-warn clippy::too_many_lines
#![allow(clippy::too_many_arguments, clippy::too_many_lines)]
// UI tasks run on the UI thread and hold widgets across awaits, so their futures are never Send.
#![allow(clippy::future_not_send)]
#![windows_subsystem = "windows"]

mod accounts;
mod audio;
mod auth;
mod clock;
mod config;
mod html;
mod mastodon;
mod network;
mod notifications;
mod read_position;
mod responses;
mod speech;
mod streaming;
mod template;
mod text;
mod timeline;
mod ui;
mod ui_wake;

use std::{
	cell::{Cell, RefCell},
	collections::{HashMap, HashSet},
	rc::Rc,
	sync::mpsc,
	time::{Duration, Instant},
};

use logkeeper::Logkeeper;
use wx_utils::tasks::{sleep, spawn_local};
use wxdragon::prelude::*;

pub(crate) use crate::ui::ids::{
	ID_BLOCKED_USERS, ID_BOOKMARK, ID_BOOKMARKS_TIMELINE, ID_BOOST, ID_CHECK_FOR_UPDATES, ID_CLEAR_ALL_TIMELINES,
	ID_CLEAR_TIMELINE, ID_CLOSE_TIMELINE, ID_COPY_POST, ID_COPY_POST_LINK, ID_CUSTOMIZE_SHORTCUTS, ID_DELETE_POST,
	ID_DIRECT_TIMELINE, ID_EDIT_POST, ID_EDIT_PROFILE, ID_FAVORITE, ID_FAVORITES_TIMELINE, ID_FEDERATED_TIMELINE,
	ID_FIND, ID_FIND_NEXT, ID_FIND_PREV, ID_HOME_TIMELINE, ID_LOAD_MORE, ID_LOCAL_TIMELINE, ID_MANAGE_ACCOUNTS,
	ID_MANAGE_FILTERS, ID_MANAGE_LISTS, ID_MENTIONS_TIMELINE, ID_MESSAGE_REQUESTS, ID_MUTE_CONVERSATION,
	ID_MUTED_USERS, ID_NEW_POST, ID_NOTIFICATIONS_TIMELINE, ID_OPEN_INSTANCE_TIMELINE_BY_INPUT, ID_OPEN_LINKS,
	ID_OPEN_LIST, ID_OPEN_POST, ID_OPEN_USER_TIMELINE_BY_INPUT, ID_OPTIONS, ID_PIN_POST, ID_PLAY_MEDIA, ID_QUOTE,
	ID_REFRESH, ID_REPLY, ID_REPLY_AUTHOR, ID_RESET_TIMELINE_SOUND, ID_SCHEDULED_POSTS, ID_SEARCH, ID_SENT_TIMELINE,
	ID_SET_TIMELINE_SOUND, ID_TOGGLE_FOLLOW, ID_TOGGLE_PERMANENT_TIMELINE, ID_TOGGLE_TIMELINE_NOTIFICATIONS,
	ID_UI_WAKE, ID_VIEW_BOOSTS, ID_VIEW_EDIT_HISTORY, ID_VIEW_FAVORITES, ID_VIEW_HASHTAGS, ID_VIEW_HELP,
	ID_VIEW_IN_BROWSER, ID_VIEW_MENTIONS, ID_VIEW_POST, ID_VIEW_PROFILE, ID_VIEW_QUOTED_THREAD, ID_VIEW_THREAD,
	ID_VIEW_USER_TIMELINE, ID_VOTE, ID_YOUR_FOLLOWERS, ID_YOUR_FOLLOWING,
};
#[cfg(not(target_os = "macos"))]
pub(crate) use crate::ui::ids::{ID_TRAY_EXIT, ID_TRAY_TOGGLE};
use crate::{
	accounts::{apply_refresh_interval, start_add_account_flow, switch_to_account},
	config::Config,
	mastodon::{MastodonClient, PollLimits},
	network::Session,
	responses::process_stream_events,
	timeline::TimelineManager,
	ui::{
		commands::{Requests, UiCommand, UiCommandContext, UiHandles, handle_ui_command},
		menu::{PostMenuState, relabel_menus},
		timeline_list::TimelineList,
		timeline_view::{TimelineViewOptions, update_active_timeline_ui},
		window::{bind_input_handlers, build_main_window},
	},
	ui_wake::{UiCommandSender, UiWaker},
};

pub(crate) enum PostOperation {
	NewPost,
	Reply { in_reply_to_id: String },
	Edit { status_id: String },
	Quote { quoted_status_id: String },
}

pub(crate) struct PendingPost {
	pub config: ui::dialogs::ComposeDialogConfig,
	pub operation: PostOperation,
	pub last_result: ui::dialogs::PostResult,
}

/// Shared by the wake handler and the UI tasks that need it. Borrow it only between awaits, and never
/// across a modal dialog: a modal's event loop keeps running tasks, and one that borrows the state
/// while it's still borrowed further up the stack would panic.
pub(crate) type SharedState = Rc<RefCell<AppState>>;

/// How long the wake handler waits before trying again when the state is already borrowed.
pub(crate) const STATE_BUSY_RETRY: Duration = Duration::from_millis(50);

pub(crate) struct AppState {
	pub(crate) config: Config,
	pub(crate) timeline_manager: TimelineManager,
	pub(crate) account_timelines: HashMap<String, TimelineManager>,
	pub(crate) account_cw_expanded: HashMap<String, HashSet<String>>,
	pub(crate) streaming_url: Option<url::Url>,
	pub(crate) access_token: Option<String>,
	pub(crate) max_post_chars: Option<usize>,
	pub(crate) max_media_description_chars: Option<usize>,
	pub(crate) poll_limits: PollLimits,
	pub(crate) hashtag_dialog: Option<ui::dialogs::HashtagDialog>,
	pub(crate) profile_dialog: Option<ui::dialogs::ProfileDialog>,
	pub(crate) follow_list_dialogs: HashMap<ui::dialogs::FollowList, ui::dialogs::FollowListDialog>,
	pub(crate) manage_lists_dialog: Option<ui::dialogs::ManageListsDialog>,
	pub(crate) manage_list_members_dialog: Option<ui::dialogs::ManageListMembersDialog>,
	pub(crate) pending_auth_dialog: Option<Dialog>,
	pub(crate) client: Option<MastodonClient>,
	pub(crate) session: Option<Session>,
	/// Set once the main window's handles exist, before the first account is shown.
	pub(crate) handles: Option<UiHandles>,
	pub(crate) cw_expanded: HashSet<String>,
	pub(crate) current_user_id: Option<String>,
	pub(crate) app_shell: Option<Rc<ui::app_shell::AppShell>>,
	pub(crate) post_menu_state: Rc<Cell<PostMenuState>>,
	pub(crate) notification_sound: Option<audio::AudioOutput>,
	pub(crate) refresh_timer: Option<Rc<Timer<Frame>>>,
	pub(crate) ui_waker: UiWaker,
	pub(crate) pending_thread_continuation: bool,
	pub(crate) synced_markers: std::collections::HashMap<&'static str, String>,
	pub(crate) pending_message_requests: Option<u64>,
	pub(crate) pending_post: Option<PendingPost>,
	pub(crate) pending_add_to_list_user: Option<String>,
}

impl AppState {
	fn new(config: Config, ui_waker: UiWaker) -> Self {
		Self {
			config,
			timeline_manager: TimelineManager::new(),
			account_timelines: HashMap::new(),
			account_cw_expanded: HashMap::new(),
			streaming_url: None,
			access_token: None,
			max_post_chars: None,
			max_media_description_chars: None,
			poll_limits: PollLimits::default(),
			hashtag_dialog: None,
			profile_dialog: None,
			follow_list_dialogs: HashMap::new(),
			manage_lists_dialog: None,
			manage_list_members_dialog: None,
			pending_auth_dialog: None,
			client: None,
			session: None,
			handles: None,
			cw_expanded: HashSet::new(),
			current_user_id: None,
			app_shell: None,
			post_menu_state: Rc::new(Cell::new(PostMenuState::default())),
			notification_sound: None,
			refresh_timer: None,
			ui_waker,
			pending_thread_continuation: false,
			synced_markers: std::collections::HashMap::new(),
			pending_message_requests: None,
			pending_post: None,
			pending_add_to_list_user: None,
		}
	}

	pub(crate) fn session(&self) -> Option<Session> {
		self.session.clone()
	}

	/// What to make the active account's requests with, once there is one.
	pub(crate) fn requests(&self) -> Option<Requests> {
		Some(Requests::new(self.handles.clone()?, self.session.clone()?))
	}

	pub(crate) fn active_account(&self) -> Option<&config::Account> {
		self.config
			.active_account_id
			.as_ref()
			.map_or_else(|| self.config.accounts.first(), |id| self.config.accounts.iter().find(|a| &a.id == id))
	}

	pub(crate) fn active_account_mut(&mut self) -> Option<&mut config::Account> {
		if let Some(id) = self.config.active_account_id.clone() {
			self.config.accounts.iter_mut().find(|a| a.id == id)
		} else {
			self.config.accounts.first_mut()
		}
	}

	/// Copies each account's open timelines into the config, then writes it.
	///
	/// Every config save goes through here so the open timelines on disk are
	/// never stale: they used to be written only on a clean exit, so a crash
	/// or an update lost them, and any other save in between wrote out an
	/// empty list.
	pub(crate) fn save_config(&mut self) -> anyhow::Result<()> {
		let active_id = self.active_account().map(|a| a.id.clone());
		for account in &mut self.config.accounts {
			let manager = if active_id.as_ref() == Some(&account.id) {
				Some(&self.timeline_manager)
			} else {
				self.account_timelines.get(&account.id)
			};
			// An empty manager means this account's timelines haven't been
			// loaded yet (mid account switch, or during startup), not that
			// the user closed them all, which isn't possible.
			let Some(manager) = manager.filter(|m| m.len() > 0) else { continue };
			account.saved_timelines = manager.open_timeline_types();
			account.saved_active_timeline = manager.active().map(|t| t.timeline_type.clone());
			account.saved_positions = manager
				.timelines()
				.iter()
				.filter(|t| t.timeline_type.keeps_position())
				.filter_map(|t| {
					// Until a timeline finds its place, what's selected in it is only where loading left it.
					let post_id = t.restore.as_ref().map(|r| r.post_id.clone()).or_else(|| t.selected_id.clone())?;
					Some(config::SavedPosition { timeline: t.timeline_type.clone(), post_id })
				})
				.collect();
		}
		config::ConfigStore::new()
			.save(&self.config)
			.inspect_err(|e| tracing::error!(error = format!("{e:#}"), "couldn't save config"))
	}

	pub(crate) fn timeline_view_options_for(&self, timeline_type: &timeline::TimelineType) -> TimelineViewOptions {
		TimelineViewOptions::from_config(&self.config, timeline_type)
	}

	pub(crate) fn redraw_active_timeline(&mut self, timeline_list: &TimelineList, suppress_selection: &Cell<bool>) {
		let Some(active) = self.timeline_manager.active() else { return };
		let options = self.timeline_view_options_for(&active.timeline_type);
		if let Some(active) = self.timeline_manager.active_mut() {
			update_active_timeline_ui(timeline_list, active, suppress_selection, &options, &self.cw_expanded);
		}
	}
}

/// Where the readme ships: next to the executable, or in `Contents/Resources` when Fedra
/// runs from a macOS app bundle.
#[must_use]
pub fn resource_dir() -> std::path::PathBuf {
	config::home().resource_dir()
}

/// Makes an unbundled binary, such as one started by `cargo run`, a regular app with a menu bar
/// and focus rather than a background process of the terminal. Must run before wx starts.
#[cfg(target_os = "macos")]
fn promote_unbundled_to_regular_app() {
	use objc::{class, msg_send, runtime::Object, sel, sel_impl};
	if config::home().kind() == homeport::InstallKind::MacBundle {
		return;
	}
	// NSApplicationActivationPolicyRegular = 0.
	unsafe {
		let ns_app: *mut Object = msg_send![class!(NSApplication), sharedApplication];
		let _: () = msg_send![ns_app, setActivationPolicy: 0_isize];
		let _: () = msg_send![ns_app, activateIgnoringOtherApps: true];
	}
}

/// Passed by the Run registry value the installer adds when "Run at startup" is checked.
const STARTUP_ARG: &str = "--startup";

fn drain_ui_commands(ui_rx: &mpsc::Receiver<UiCommand>, ctx: &mut UiCommandContext<'_>) {
	while let Ok(cmd) = ui_rx.try_recv() {
		handle_ui_command(cmd, ctx);
	}
}

fn main() {
	// Before logging starts, so a second copy exits without starting a new log and pushing the running copy's aside.
	// macOS reopens the running app through on_reopen_app instead of starting another copy.
	#[cfg(not(target_os = "macos"))]
	let door = match doorbell::ring("fedra", &[] as &[&str]) {
		Ok(doorbell::Ring::Delivered) => return,
		Ok(doorbell::Ring::First(door)) => Ok(door),
		Err(e) => Err(e),
	};
	let _log_guard =
		Logkeeper::new(config::config_dir(), "fedra").with_crates(["fedra"]).init(config::log_level_from_disk());
	tracing::info!(version = env!("CARGO_PKG_VERSION"), commit = env!("FEDRA_COMMIT_HASH"), "starting");
	#[cfg(not(target_os = "macos"))]
	let door = door.inspect_err(|e| tracing::warn!(error = %e, "couldn't check for a running copy")).ok();
	let _ = rustls::crypto::ring::default_provider().install_default();
	#[cfg(target_os = "macos")]
	promote_unbundled_to_regular_app();
	let _ = wxdragon::main(|app| {
		let window_parts = build_main_window();
		let frame = window_parts.frame;
		speech::init(&frame);
		let timelines_selector = window_parts.timelines_selector;
		let timeline_list = window_parts.timeline_list.clone();
		let (ui_tx_raw, ui_rx) = mpsc::channel();
		let is_shutting_down = Rc::new(Cell::new(false));
		let suppress_selection = Rc::new(Cell::new(false));
		let wake_busy = Rc::new(Cell::new(false));
		let wake_reschedule = Rc::new(Cell::new(false));
		let store = config::ConfigStore::new();
		let config = store.load();
		ui::spell_check::configure(config.spell_check, config.spell_check_announce, &config.spell_language);
		let launched_at_startup = std::env::args().any(|arg| arg == STARTUP_ARG);
		let start_hidden = launched_at_startup && config.saved_window_hidden && !config.accounts.is_empty();
		let tray_hidden = Rc::new(Cell::new(start_hidden));
		let ui_alive = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
		let ui_waker = UiWaker::new(frame, ui_alive.clone());
		let ui_tx = UiCommandSender::new(ui_tx_raw, ui_waker.clone());
		let ui_tx_reopen = ui_tx.clone();
		app.on_reopen_app(move || {
			let _ = ui_tx_reopen.send(UiCommand::ShowWindow);
		});
		let quick_action_keys_enabled = Rc::new(Cell::new(config.quick_action_keys));
		let autoload_mode = Rc::new(Cell::new(config.autoload));
		let sort_order_cell = Rc::new(Cell::new(config.sort_order));
		let shortcuts_cell = Rc::new(RefCell::new(config.shortcuts.clone()));
		let mut state = AppState::new(config, ui_waker.clone());
		match audio::AudioOutput::open() {
			Ok(output) => state.notification_sound = Some(output),
			Err(err) => tracing::warn!(error = %err, "couldn't open audio output, so notifications have no sound"),
		}
		let refresh_timer = Rc::new(Timer::new(&frame));
		let ui_tx_timer_poll = ui_tx.clone();
		refresh_timer.on_tick(move |_| {
			let _ = ui_tx_timer_poll.send(UiCommand::PollStreamable);
		});
		let refresh_timer_keepalive = refresh_timer.clone();
		state.refresh_timer = Some(refresh_timer);
		apply_refresh_interval(&state);
		if state.config.accounts.is_empty() && !start_add_account_flow(&frame, &ui_tx, &mut state) {
			frame.close(true);
			return;
		}
		if let Some(mb) = frame.get_menu_bar() {
			relabel_menus(&mb, &state);
		}
		let timeline_list_wake = timeline_list.clone();
		let state: SharedState = Rc::new(RefCell::new(state));
		let handles = UiHandles {
			state: Rc::downgrade(&state),
			frame,
			timelines_selector,
			timeline_list: timeline_list.clone(),
			suppress_selection: suppress_selection.clone(),
			quick_action_keys_enabled: quick_action_keys_enabled.clone(),
			autoload_mode: autoload_mode.clone(),
			sort_order_cell: sort_order_cell.clone(),
			tray_hidden,
			shortcuts_cell: shortcuts_cell.clone(),
			ui_tx: ui_tx.clone(),
			is_shutting_down: is_shutting_down.clone(),
		};
		// Nothing else can be using the state yet: the wake handler isn't bound and no task has run.
		let (app_shell_close, start_maximized, post_menu_state_for_handlers) = {
			let mut state = state.borrow_mut();
			state.handles = Some(handles.clone());
			switch_to_account(&mut state, &frame, timelines_selector, &timeline_list, &suppress_selection, false, None);
			let app_shell = Rc::new(ui::app_shell::install_app_shell(&frame, ui_tx.clone()));
			let app_shell_close = app_shell.clone();
			state.app_shell = Some(app_shell);
			ui::commands::register_hotkeys(&state, &ui_tx, &timeline_list);
			if state.config.check_for_updates_on_startup {
				crate::ui::update_check::run_update_check(frame, true, ui_tx.clone());
			}
			(app_shell_close, state.config.start_maximized, state.post_menu_state.clone())
		};
		let shutdown_wake = is_shutting_down.clone();
		let suppress_wake = suppress_selection.clone();
		let busy_wake = wake_busy;
		let reschedule_wake = wake_reschedule;
		let ui_waker_handler = ui_waker.clone();
		let mut last_ui_refresh = Instant::now();
		frame.bind_with_id_internal(EventType::MENU, ID_UI_WAKE, move |_| {
			let is_shutting_down = shutdown_wake.get();
			if busy_wake.get() {
				reschedule_wake.set(true);
				return;
			}
			// Only reachable if something holds the state across a modal dialog. Coming back to the work
			// later beats panicking, which would take the whole app down.
			let Ok(mut state) = state.try_borrow_mut() else {
				ui_waker_handler.reset();
				let ui_waker = ui_waker_handler.clone();
				spawn_local(async move {
					sleep(STATE_BUSY_RETRY).await;
					ui_waker.wake();
				})
				.detach();
				return;
			};
			busy_wake.set(true);
			ui_waker_handler.reset();
			let mut ctx = handles.context(&mut state);
			drain_ui_commands(&ui_rx, &mut ctx);
			if is_shutting_down {
				busy_wake.set(false);
				return;
			}
			process_stream_events(ctx.state, &timeline_list_wake, &suppress_wake, &frame);
			if last_ui_refresh.elapsed() >= Duration::from_secs(60) {
				ctx.state.redraw_active_timeline(&timeline_list_wake, &suppress_wake);
				last_ui_refresh = Instant::now();
			}
			busy_wake.set(false);
			ui_waker_handler.reset();
			if reschedule_wake.replace(false) {
				ui_waker_handler.wake();
			}
		});
		// A copy started before now waits at the door until the wake handler above can hear it.
		#[cfg(not(target_os = "macos"))]
		if let Some(door) = door {
			let ui_tx_door = ui_tx.clone();
			door.listen(move |_| {
				tracing::info!("another copy was started, so showing the window");
				let _ = ui_tx_door.send(UiCommand::ShowWindow);
			});
		}
		let ui_alive_destroy = ui_alive;
		frame.on_destroy(move |_| {
			ui_alive_destroy.store(false, std::sync::atomic::Ordering::SeqCst);
			refresh_timer_keepalive.stop();
		});
		bind_input_handlers(
			&window_parts,
			ui_tx.clone(),
			is_shutting_down.clone(),
			suppress_selection,
			&quick_action_keys_enabled,
			autoload_mode,
			sort_order_cell,
			post_menu_state_for_handlers,
			shortcuts_cell,
		);
		let shutdown_close = is_shutting_down;
		let frame_close = frame;
		let ui_tx_close = ui_tx.clone();
		let ui_waker_close = ui_waker;
		frame.on_close(move |event| {
			if shutdown_close.get() {
				event.skip(true); // Actually close
			} else {
				shutdown_close.set(true);
				let _ = ui_tx_close.send(UiCommand::AppClosing);
				ui_waker_close.wake();
				// Hide the window and clean up the tray icon before destruction begins,
				// so the screen reader doesn't announce the window during teardown.
				frame_close.show(false);
				app_shell_close.cleanup();
				event.skip(false); // Wait for AppClosing command to be processed
			}
		});
		frame.centre();
		if start_maximized {
			frame.maximize(true);
		}
		if !start_hidden {
			frame.show(true);
			ship_shape::ui::bring_to_front_after_update(&frame);
			// macOS otherwise leaves keyboard focus nowhere Tab can move on from.
			if cfg!(target_os = "macos") {
				window_parts.timeline_list.set_focus();
			}
		}
		tracing::info!(hidden = start_hidden, "started");
	});
	tracing::info!("exiting");
}
