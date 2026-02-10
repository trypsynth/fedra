#![warn(clippy::all, clippy::pedantic, clippy::nursery)]
#![allow(clippy::too_many_arguments, clippy::too_many_lines)] // temp
#![cfg_attr(windows, deny(warnings))]
#![windows_subsystem = "windows"]

mod accounts;
mod auth;
mod commands;
mod config;
mod html;
mod mastodon;
mod network;
mod notifications;
mod responses;
mod streaming;
mod text;
mod timeline;
mod ui;
mod ui_wake;
mod update;

use std::{
	cell::Cell,
	collections::{HashMap, HashSet},
	rc::Rc,
	sync::mpsc,
	time::{Duration, Instant},
};

use wxdragon::prelude::*;

pub(crate) use crate::ui::ids::{
	ID_BOOKMARK, ID_BOOKMARKS_TIMELINE, ID_BOOST, ID_CHECK_FOR_UPDATES, ID_CLOSE_TIMELINE, ID_COPY_POST,
	ID_DELETE_POST, ID_DIRECT_TIMELINE, ID_EDIT_POST, ID_EDIT_PROFILE, ID_FAVORITE, ID_FAVORITES_TIMELINE,
	ID_FEDERATED_TIMELINE, ID_LOAD_MORE, ID_LOCAL_TIMELINE, ID_MANAGE_ACCOUNTS, ID_NEW_POST, ID_OPEN_LINKS,
	ID_OPEN_USER_TIMELINE_BY_INPUT, ID_OPTIONS, ID_REFRESH, ID_REPLY, ID_REPLY_AUTHOR, ID_SEARCH, ID_TRAY_EXIT,
	ID_TRAY_TOGGLE, ID_UI_WAKE, ID_VIEW_BOOSTS, ID_VIEW_FAVORITES, ID_VIEW_HASHTAGS, ID_VIEW_HELP, ID_VIEW_IN_BROWSER,
	ID_VIEW_MENTIONS, ID_VIEW_PROFILE, ID_VIEW_THREAD, ID_VIEW_USER_TIMELINE, ID_VOTE, KEY_DELETE,
};
use crate::{
	accounts::{start_add_account_flow, switch_to_account},
	commands::{UiCommand, handle_ui_command},
	config::{Config, TimestampFormat},
	mastodon::{MastodonClient, PollLimits},
	network::NetworkHandle,
	responses::{process_network_responses, process_stream_events},
	timeline::TimelineManager,
	ui::{
		menu::update_menu_labels,
		timeline_view::update_active_timeline_ui,
		window::{bind_input_handlers, build_main_window},
	},
	ui_wake::{UiCommandSender, UiWaker},
};

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
	pub(crate) pending_auth_dialog: Option<Dialog>,
	pub(crate) client: Option<MastodonClient>,
	pub(crate) pending_user_lookup_action: Option<ui::dialogs::UserLookupAction>,
	pub(crate) cw_expanded: HashSet<String>,
	pub(crate) current_user_id: Option<String>,
	pub(crate) app_shell: Option<Rc<ui::app_shell::AppShell>>,
	pub(crate) media_ctrl: Option<MediaCtrl>,
	pub(crate) ui_waker: UiWaker,
	pub(crate) _instance_checker: Option<SingleInstanceChecker>,
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
			pending_auth_dialog: None,
			client: None,
			pending_user_lookup_action: None,
			cw_expanded: HashSet::new(),
			current_user_id: None,
			app_shell: None,
			media_ctrl: None,
			ui_waker,
			_instance_checker: instance_checker,
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
}

#[must_use]
pub fn get_sound_path() -> std::path::PathBuf {
	std::env::current_exe()
		.ok()
		.and_then(|path| path.parent().map(|p| p.join("sounds").join("boop.mp3")))
		.unwrap_or_else(|| std::path::PathBuf::from("sounds/boop.mp3"))
}

fn drain_ui_commands(
	ui_rx: &mpsc::Receiver<UiCommand>,
	state: &mut AppState,
	frame: &Frame,
	timelines_selector: ListBox,
	timeline_list: ListBox,
	suppress_selection: &Cell<bool>,
	live_region: StaticText,
	quick_action_keys_enabled: &Cell<bool>,
	autoload_mode: &Cell<config::AutoloadMode>,
	sort_order_cell: &Cell<config::SortOrder>,
	tray_hidden: &Cell<bool>,
	ui_tx: &UiCommandSender,
) {
	while let Ok(cmd) = ui_rx.try_recv() {
		handle_ui_command(
			cmd,
			state,
			frame,
			timelines_selector,
			timeline_list,
			suppress_selection,
			live_region,
			quick_action_keys_enabled,
			autoload_mode,
			sort_order_cell,
			tray_hidden,
			ui_tx,
		);
	}
}

fn main() {
	let _ = wxdragon::main(|_| {
		let _ = set_appearance(Appearance::System);
		let instance_checker = SingleInstanceChecker::new("Fedra.SingleInstance", None);
		if let Some(checker) = instance_checker.as_ref() {
			if checker.is_another_running() {
				let frame = Frame::builder().with_title("Fedra").with_size(Size::new(1, 1)).build();
				let dialog = MessageDialog::builder(&frame, "Fedra is already running.", "Error")
					.with_style(MessageDialogStyle::OK | MessageDialogStyle::IconError)
					.build();
				dialog.show_modal();
				frame.close(true);
				return;
			}
		}
		let window_parts = build_main_window();
		let frame = window_parts.frame;
		let timelines_selector = window_parts.timelines_selector;
		let timeline_list = window_parts.timeline_list;
		let live_region_label = window_parts.live_region_label;
		let (ui_tx_raw, ui_rx) = mpsc::channel();
		let is_shutting_down = Rc::new(Cell::new(false));
		let suppress_selection = Rc::new(Cell::new(false));
		let wake_busy = Rc::new(Cell::new(false));
		let wake_reschedule = Rc::new(Cell::new(false));
		let tray_hidden = Rc::new(Cell::new(false));
		let store = config::ConfigStore::new();
		let config = store.load();
		let ui_alive = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
		let ui_waker = UiWaker::new(frame, ui_alive.clone());
		let ui_tx = UiCommandSender::new(ui_tx_raw, ui_waker.clone());
		let quick_action_keys_enabled = Rc::new(Cell::new(config.quick_action_keys));
		let autoload_mode = Rc::new(Cell::new(config.autoload));
		let sort_order_cell = Rc::new(Cell::new(config.sort_order));
		let mut state = AppState::new(config, ui_waker.clone(), instance_checker);
		let mc = MediaCtrl::builder(&frame).with_size(Size::new(0, 0)).build();
		let sound_path = get_sound_path();
		if sound_path.exists() {
			mc.load(&sound_path.to_string_lossy());
		}
		state.media_ctrl = Some(mc);
		if state.config.accounts.is_empty() && !start_add_account_flow(&frame, &ui_tx, &mut state) {
			frame.close(true);
			return;
		}
		if let Some(mb) = frame.get_menu_bar() {
			update_menu_labels(&mb, &state);
		}
		switch_to_account(
			&mut state,
			&frame,
			timelines_selector,
			timeline_list,
			&suppress_selection,
			live_region_label,
			false,
			None,
		);
		let app_shell = Rc::new(ui::app_shell::install_app_shell(&frame, ui_tx.clone()));
		app_shell.clone().attach_destroy(&frame);
		state.app_shell = Some(app_shell);

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
		let live_region_wake = live_region_label;
		let mut state = state;
		let ui_waker_handler = ui_waker.clone();
		let quick_action_keys_drain = quick_action_keys_enabled.clone();
		let autoload_drain = autoload_mode.clone();
		let sort_order_drain = sort_order_cell.clone();
		let tray_hidden_drain = tray_hidden;
		let ui_tx_timer = ui_tx.clone();
		let mut last_ui_refresh = Instant::now();
		frame.bind_with_id_internal(EventType::MENU, ID_UI_WAKE, move |_| {
			if shutdown_wake.get() {
				return;
			}
			if busy_wake.get() {
				reschedule_wake.set(true);
				ui_waker_handler.reset();
				return;
			}
			busy_wake.set(true);
			ui_waker_handler.reset();
			drain_ui_commands(
				&ui_rx,
				&mut state,
				&frame_wake,
				timelines_selector_wake,
				timeline_list_wake,
				&suppress_wake,
				live_region_wake,
				&quick_action_keys_drain,
				&autoload_drain,
				&sort_order_drain,
				&tray_hidden_drain,
				&ui_tx_timer,
			);
			process_stream_events(&mut state, timeline_list_wake, &suppress_wake, &frame_wake);
			process_network_responses(
				&frame_wake,
				&mut state,
				timelines_selector_wake,
				timeline_list_wake,
				&suppress_wake,
				live_region_wake,
				&quick_action_keys_drain,
				&autoload_drain,
				&sort_order_drain,
				&tray_hidden_drain,
				&ui_tx_timer,
			);
			if last_ui_refresh.elapsed() >= Duration::from_secs(60) {
				if state.config.timestamp_format == TimestampFormat::Relative
					&& let Some(active) = state.timeline_manager.active_mut()
				{
					update_active_timeline_ui(
						timeline_list_wake,
						active,
						&suppress_wake,
						state.config.sort_order,
						state.config.timestamp_format,
						state.config.content_warning_display,
						&state.cw_expanded,
						state.config.preserve_thread_order,
					);
				}
				last_ui_refresh = Instant::now();
			}

			busy_wake.set(false);
			if reschedule_wake.replace(false) {
				ui_waker_handler.wake();
			}
		});

		let refresh_timer = Rc::new(Timer::new(&frame));
		let refresh_waker = ui_waker;
		refresh_timer.on_tick(move |_| {
			refresh_waker.wake();
		});
		refresh_timer.start(60_000, false);
		let refresh_timer_keepalive = refresh_timer;
		let ui_alive_destroy = ui_alive;
		frame.on_destroy(move |_| {
			ui_alive_destroy.store(false, std::sync::atomic::Ordering::SeqCst);
			refresh_timer_keepalive.stop();
		});

		bind_input_handlers(
			&window_parts,
			ui_tx.clone(),
			is_shutting_down,
			suppress_selection.clone(),
			quick_action_keys_enabled,
			autoload_mode,
			sort_order_cell,
		);
		frame.show(true);
		frame.centre();
	});
}
