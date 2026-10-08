#![warn(clippy::all, clippy::pedantic, clippy::nursery)]
// Still allowed: 9 functions take more than 7 arguments and 26 run past 100
// lines, nearly all of them dialog builders. Splitting those up is a refactor
// rather than a lint fix. To see the list:
//   cargo clippy --all-targets -- --force-warn clippy::too_many_arguments --force-warn clippy::too_many_lines
#![allow(clippy::too_many_arguments, clippy::too_many_lines)]
#![windows_subsystem = "windows"]

mod accounts;
mod audio;
mod auth;
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
	cell::Cell,
	collections::{HashMap, HashSet},
	fs::File,
	io::Write,
	panic,
	rc::Rc,
	sync::mpsc,
	time::{Duration, Instant},
};

use wxdragon::prelude::*;

pub(crate) use crate::ui::ids::{
	ID_BOOKMARK, ID_BOOKMARKS_TIMELINE, ID_BOOST, ID_CHECK_FOR_UPDATES, ID_CLEAR_ALL_TIMELINES, ID_CLEAR_TIMELINE,
	ID_CLOSE_TIMELINE, ID_COPY_POST, ID_COPY_POST_LINK, ID_CUSTOMIZE_SHORTCUTS, ID_DELETE_POST, ID_DIRECT_TIMELINE,
	ID_EDIT_POST, ID_EDIT_PROFILE, ID_FAVORITE, ID_FAVORITES_TIMELINE, ID_FEDERATED_TIMELINE, ID_FIND, ID_FIND_NEXT,
	ID_FIND_PREV, ID_HOME_TIMELINE, ID_LOAD_MORE, ID_LOCAL_TIMELINE, ID_MANAGE_ACCOUNTS, ID_MANAGE_FILTERS,
	ID_MANAGE_LISTS, ID_MENTIONS_TIMELINE, ID_MESSAGE_REQUESTS, ID_NEW_POST, ID_NOTIFICATIONS_TIMELINE,
	ID_OPEN_INSTANCE_TIMELINE_BY_INPUT, ID_OPEN_LINKS, ID_OPEN_LIST, ID_OPEN_USER_TIMELINE_BY_INPUT, ID_OPTIONS,
	ID_PIN_POST, ID_PLAY_MEDIA, ID_QUOTE, ID_REFRESH, ID_REPLY, ID_REPLY_AUTHOR, ID_RESET_TIMELINE_SOUND, ID_SEARCH,
	ID_SENT_TIMELINE, ID_SET_TIMELINE_SOUND, ID_TOGGLE_FOLLOW, ID_TOGGLE_PERMANENT_TIMELINE,
	ID_TOGGLE_TIMELINE_NOTIFICATIONS, ID_TRAY_EXIT, ID_TRAY_TOGGLE, ID_UI_WAKE, ID_VIEW_BOOSTS, ID_VIEW_FAVORITES,
	ID_VIEW_HASHTAGS, ID_VIEW_HELP, ID_VIEW_IN_BROWSER, ID_VIEW_MENTIONS, ID_VIEW_POST, ID_VIEW_PROFILE,
	ID_VIEW_QUOTED_THREAD, ID_VIEW_THREAD, ID_VIEW_USER_TIMELINE, ID_VOTE,
};
use crate::{
	accounts::{apply_refresh_interval, start_add_account_flow, switch_to_account},
	config::Config,
	mastodon::{MastodonClient, PollLimits},
	network::NetworkHandle,
	responses::{NetworkResponseContext, process_network_responses, process_stream_events},
	timeline::TimelineManager,
	ui::{
		commands::{UiCommand, UiCommandContext, handle_ui_command},
		menu::update_menu_labels,
		timeline_view::{TimelineViewOptions, update_active_timeline_ui},
		window::{bind_input_handlers, build_main_window},
	},
	ui_wake::{UiCommandSender, UiWaker},
};

#[derive(Copy, Clone, Default)]
#[allow(clippy::struct_excessive_bools)]
pub struct ContextMenuState {
	pub favourited: bool,
	pub reblogged: bool,
	pub bookmarked: bool,
	pub pinned: bool,
	pub is_direct: bool,
	pub is_own: bool,
	pub has_boosts: bool,
	pub has_favorites: bool,
	pub quick_action_keys: bool,
}

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

pub(crate) struct AppState {
	pub(crate) config: Config,
	pub(crate) timeline_manager: TimelineManager,
	pub(crate) account_timelines: HashMap<String, TimelineManager>,
	pub(crate) account_cw_expanded: HashMap<String, HashSet<String>>,
	pub(crate) network_handle: Option<NetworkHandle>,
	pub(crate) streaming_url: Option<url::Url>,
	pub(crate) access_token: Option<String>,
	pub(crate) max_post_chars: Option<usize>,
	pub(crate) poll_limits: PollLimits,
	pub(crate) hashtag_dialog: Option<ui::dialogs::HashtagDialog>,
	pub(crate) profile_dialog: Option<ui::dialogs::ProfileDialog>,
	pub(crate) followers_dialog: Option<ui::dialogs::FollowListDialog>,
	pub(crate) following_dialog: Option<ui::dialogs::FollowListDialog>,
	pub(crate) manage_lists_dialog: Option<ui::dialogs::ManageListsDialog>,
	pub(crate) manage_list_members_dialog: Option<ui::dialogs::ManageListMembersDialog>,
	pub(crate) pending_auth_dialog: Option<Dialog>,
	pub(crate) client: Option<MastodonClient>,
	pub(crate) pending_user_lookup_action: Option<ui::dialogs::UserLookupAction>,
	pub(crate) cw_expanded: HashSet<String>,
	pub(crate) current_user_id: Option<String>,
	pub(crate) app_shell: Option<Rc<ui::app_shell::AppShell>>,
	pub(crate) context_menu_state: Rc<Cell<ContextMenuState>>,
	pub(crate) notification_sound: Option<audio::AudioOutput>,
	pub(crate) refresh_timer: Option<Rc<Timer<Frame>>>,
	pub(crate) ui_waker: UiWaker,
	pub(crate) _instance_checker: Option<SingleInstanceChecker>,
	pub(crate) pending_thread_continuation: bool,
	pub(crate) pending_restore_post_id: Option<(crate::timeline::TimelineType, String)>,
	pub(crate) restore_pages: u8,
	pub(crate) synced_markers: std::collections::HashMap<&'static str, String>,
	pub(crate) pending_message_requests: Option<u64>,
	pub(crate) pending_post: Option<PendingPost>,
	pub(crate) pending_add_to_list_user: Option<String>,
}

impl AppState {
	fn new(config: Config, ui_waker: UiWaker, instance_checker: Option<SingleInstanceChecker>) -> Self {
		Self {
			config,
			timeline_manager: TimelineManager::new(),
			account_timelines: HashMap::new(),
			account_cw_expanded: HashMap::new(),
			network_handle: None,
			streaming_url: None,
			access_token: None,
			max_post_chars: None,
			poll_limits: PollLimits::default(),
			hashtag_dialog: None,
			profile_dialog: None,
			followers_dialog: None,
			following_dialog: None,
			manage_lists_dialog: None,
			manage_list_members_dialog: None,
			pending_auth_dialog: None,
			client: None,
			pending_user_lookup_action: None,
			cw_expanded: HashSet::new(),
			current_user_id: None,
			app_shell: None,
			context_menu_state: Rc::new(Cell::new(ContextMenuState::default())),
			notification_sound: None,
			refresh_timer: None,
			ui_waker,
			_instance_checker: instance_checker,
			pending_thread_continuation: false,
			pending_restore_post_id: None,
			restore_pages: 0,
			synced_markers: std::collections::HashMap::new(),
			pending_message_requests: None,
			pending_post: None,
			pending_add_to_list_user: None,
		}
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
			account.saved_selected_post_id = manager.active().and_then(|t| t.selected_id.clone());
		}
		config::ConfigStore::new().save(&self.config)
	}

	pub(crate) fn timeline_view_options_for(&self, timeline_type: &timeline::TimelineType) -> TimelineViewOptions {
		TimelineViewOptions::from_config(&self.config, timeline_type)
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
	let in_bundle = std::env::current_exe().is_ok_and(|exe| exe.to_string_lossy().contains(".app/Contents/MacOS/"));
	if in_bundle {
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
	let _ = rustls::crypto::ring::default_provider().install_default();
	panic::set_hook(Box::new(|info| {
		let log_path = config::config_dir().join("crash.log");
		if let Ok(mut file) = File::create(&log_path) {
			let _ = writeln!(file, "Fedra crashed at {}", chrono::Local::now());
			let _ = writeln!(file, "{info}");
			if let Some(location) = info.location() {
				let _ = writeln!(file, "  at {}:{}:{}", location.file(), location.line(), location.column());
			}
		}
	}));
	#[cfg(target_os = "macos")]
	promote_unbundled_to_regular_app();
	let _ = wxdragon::main(|app| {
		// macOS reopens the running app through on_reopen_app instead of starting another instance.
		#[cfg(target_os = "macos")]
		let instance_checker: Option<SingleInstanceChecker> = None;
		#[cfg(not(target_os = "macos"))]
		let instance_checker = SingleInstanceChecker::new("Fedra.SingleInstance", None);
		if let Some(checker) = instance_checker.as_ref()
			&& checker.is_another_running()
		{
			let frame = Frame::builder().with_title("Fedra").with_size(Size::new(1, 1)).build();
			let dialog = MessageDialog::builder(&frame, "Fedra is already running.", "Error")
				.with_style(MessageDialogStyle::OK | MessageDialogStyle::IconError)
				.build();
			dialog.show_modal();
			frame.close(true);
			return;
		}
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
		let shortcuts_cell = Rc::new(std::cell::RefCell::new(config.shortcuts.clone()));
		let mut state = AppState::new(config, ui_waker.clone(), instance_checker);
		match audio::AudioOutput::open() {
			Ok(output) => state.notification_sound = Some(output),
			Err(err) => eprintln!("Failed to open audio output for notification sound: {err}"),
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
			update_menu_labels(&mb, &state);
		}
		switch_to_account(&mut state, &frame, timelines_selector, &timeline_list, &suppress_selection, false, None);
		let app_shell = Rc::new(ui::app_shell::install_app_shell(&frame, ui_tx.clone()));
		let app_shell_close = app_shell.clone();
		state.app_shell = Some(app_shell);
		ui::commands::register_hotkeys(&state, &ui_tx, &timeline_list);
		if state.config.check_for_updates_on_startup {
			crate::ui::update_check::run_update_check(frame, true);
		}
		let shutdown_wake = is_shutting_down.clone();
		let suppress_wake = suppress_selection.clone();
		let busy_wake = wake_busy;
		let reschedule_wake = wake_reschedule;
		let frame_wake = frame;
		let timelines_selector_wake = timelines_selector;
		let timeline_list_wake = timeline_list;
		let mut state = state;
		let context_menu_state_for_handlers = state.context_menu_state.clone();
		let ui_waker_handler = ui_waker.clone();
		let quick_action_keys_drain = quick_action_keys_enabled.clone();
		let autoload_drain = autoload_mode.clone();
		let sort_order_drain = sort_order_cell.clone();
		let tray_hidden_drain = tray_hidden;
		let shortcuts_drain = shortcuts_cell.clone();
		let ui_tx_timer = ui_tx.clone();
		let mut last_ui_refresh = Instant::now();
		frame.bind_with_id_internal(EventType::MENU, ID_UI_WAKE, move |_| {
			let is_shutting_down = shutdown_wake.get();
			if busy_wake.get() {
				reschedule_wake.set(true);
				return;
			}
			busy_wake.set(true);
			ui_waker_handler.reset();
			{
				let mut ui_ctx = UiCommandContext {
					state: &mut state,
					frame: &frame_wake,
					timelines_selector: timelines_selector_wake,
					timeline_list: timeline_list_wake.clone(),
					suppress_selection: &suppress_wake,
					live_region: &timeline_list_wake,
					quick_action_keys_enabled: &quick_action_keys_drain,
					autoload_mode: &autoload_drain,
					sort_order_cell: &sort_order_drain,
					tray_hidden: &tray_hidden_drain,
					shortcuts_cell: &shortcuts_drain,
					ui_tx: &ui_tx_timer,
				};
				drain_ui_commands(&ui_rx, &mut ui_ctx);
			}
			if is_shutting_down {
				busy_wake.set(false);
				return;
			}
			process_stream_events(&mut state, &timeline_list_wake, &suppress_wake, &frame_wake);
			{
				let mut network_ctx = NetworkResponseContext {
					frame: &frame_wake,
					state: &mut state,
					timelines_selector: timelines_selector_wake,
					timeline_list: timeline_list_wake.clone(),
					suppress_selection: &suppress_wake,
					live_region: &timeline_list_wake,
					quick_action_keys_enabled: &quick_action_keys_drain,
					autoload_mode: &autoload_drain,
					sort_order_cell: &sort_order_drain,
					tray_hidden: &tray_hidden_drain,
					shortcuts_cell: &shortcuts_drain,
					ui_tx: &ui_tx_timer,
				};
				process_network_responses(&mut network_ctx);
			}
			if last_ui_refresh.elapsed() >= Duration::from_secs(60) {
				let view_options =
					state.timeline_manager.active().map(|a| state.timeline_view_options_for(&a.timeline_type));
				let active_index = state.timeline_manager.active_index();
				if let Some(view_options) = view_options
					&& let Some(active) = state.timeline_manager.active_mut()
				{
					update_active_timeline_ui(
						&timeline_list_wake,
						active,
						&suppress_wake,
						&view_options,
						&state.cw_expanded,
						active_index,
					);
				}
				last_ui_refresh = Instant::now();
			}
			busy_wake.set(false);
			ui_waker_handler.reset();
			if reschedule_wake.replace(false) {
				ui_waker_handler.wake();
			}
		});
		let ui_alive_destroy = ui_alive;
		frame.on_destroy(move |_| {
			ui_alive_destroy.store(false, std::sync::atomic::Ordering::SeqCst);
			refresh_timer_keepalive.stop();
		});
		bind_input_handlers(
			&window_parts,
			ui_tx.clone(),
			is_shutting_down.clone(),
			suppress_selection.clone(),
			&quick_action_keys_enabled,
			autoload_mode,
			sort_order_cell,
			context_menu_state_for_handlers,
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
		if !start_hidden {
			frame.show(true);
			// macOS otherwise leaves keyboard focus nowhere Tab can move on from.
			if cfg!(target_os = "macos") {
				window_parts.timeline_list.set_focus();
			}
		}
	});
}
