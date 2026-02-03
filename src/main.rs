#![windows_subsystem = "windows"]

mod accounts;
mod auth;
mod commands;
mod config;
mod html;
mod live_region;
mod mastodon;
mod network;
mod responses;
mod streaming;
mod timeline;
mod ui;

use std::{
	cell::Cell,
	collections::HashSet,
	rc::Rc,
	sync::mpsc,
	time::{Duration, Instant},
};

use wxdragon::prelude::*;

pub(crate) use crate::ui::ids::*;
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
};

pub(crate) struct AppState {
	pub(crate) config: Config,
	pub(crate) timeline_manager: TimelineManager,
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
}

impl AppState {
	fn new(config: Config) -> Self {
		Self {
			config,
			timeline_manager: TimelineManager::new(),
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
		}
	}

	pub(crate) fn active_account(&self) -> Option<&config::Account> {
		if let Some(id) = &self.config.active_account_id {
			self.config.accounts.iter().find(|a| &a.id == id)
		} else {
			self.config.accounts.first()
		}
	}

	pub(crate) fn active_account_mut(&mut self) -> Option<&mut config::Account> {
		if let Some(id) = self.config.active_account_id.clone() {
			self.config.accounts.iter_mut().find(|a| a.id == id)
		} else {
			self.config.accounts.first_mut()
		}
	}
}

fn drain_ui_commands(
	ui_rx: &mpsc::Receiver<UiCommand>,
	state: &mut AppState,
	frame: &Frame,
	timelines_selector: &ListBox,
	timeline_list: &ListBox,
	suppress_selection: &Cell<bool>,
	live_region: &StaticText,
	quick_action_keys_enabled: &Cell<bool>,
	autoload_mode: &Cell<config::AutoloadMode>,
	sort_order_cell: &Cell<config::SortOrder>,
	tray_hidden: &Cell<bool>,
	ui_tx: &mpsc::Sender<UiCommand>,
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
		let window_parts = build_main_window();
		let frame = window_parts.frame;
		let timelines_selector = window_parts.timelines_selector;
		let timeline_list = window_parts.timeline_list;
		let live_region_label = window_parts.live_region_label;
		let (ui_tx, ui_rx) = mpsc::channel();
		let is_shutting_down = Rc::new(Cell::new(false));
		let suppress_selection = Rc::new(Cell::new(false));
		let timer_busy = Rc::new(Cell::new(false));
		let tray_hidden = Rc::new(Cell::new(false));
		let store = config::ConfigStore::new();
		let config = store.load();
		let quick_action_keys_enabled = Rc::new(Cell::new(config.quick_action_keys));
		let autoload_mode = Rc::new(Cell::new(config.autoload));
		let sort_order_cell = Rc::new(Cell::new(config.sort_order));
		let mut state = AppState::new(config);

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
			&timelines_selector,
			&timeline_list,
			&suppress_selection,
			&live_region_label,
			false,
		);
		let app_shell = ui::app_shell::install_app_shell(&frame, ui_tx.clone());
		let timer = Rc::new(Timer::new(&frame));
		let shutdown_timer = is_shutting_down.clone();
		let suppress_timer = suppress_selection.clone();
		let busy_timer = timer_busy.clone();
		let frame_timer = frame;
		let timelines_selector_timer = timelines_selector;
		let timeline_list_timer = timeline_list;
		let live_region_timer = live_region_label;
		let mut state = state;
		let timer_tick = timer.clone();
		let quick_action_keys_drain = quick_action_keys_enabled.clone();
		let autoload_drain = autoload_mode.clone();
		let sort_order_drain = sort_order_cell.clone();
		let tray_hidden_drain = tray_hidden.clone();
		let ui_tx_timer = ui_tx.clone();
		let mut last_ui_refresh = Instant::now();
		timer_tick.on_tick(move |_| {
			if shutdown_timer.get() {
				return;
			}
			if busy_timer.get() {
				return;
			}
			busy_timer.set(true);
			drain_ui_commands(
				&ui_rx,
				&mut state,
				&frame_timer,
				&timelines_selector_timer,
				&timeline_list_timer,
				&suppress_timer,
				&live_region_timer,
				&quick_action_keys_drain,
				&autoload_drain,
				&sort_order_drain,
				&tray_hidden_drain,
				&ui_tx_timer,
			);
			process_stream_events(&mut state, &timeline_list_timer, &suppress_timer, &frame_timer);
			process_network_responses(
				&frame_timer,
				&mut state,
				&timelines_selector_timer,
				&timeline_list_timer,
				&suppress_timer,
				&live_region_timer,
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
						&timeline_list_timer,
						active,
						&suppress_timer,
						state.config.sort_order,
						state.config.timestamp_format,
						state.config.content_warning_display,
						&state.cw_expanded,
					);
				}
				last_ui_refresh = Instant::now();
			}

			busy_timer.set(false);
		});
		timer.start(100, false);
		bind_input_handlers(
			&window_parts,
			ui_tx.clone(),
			is_shutting_down.clone(),
			suppress_selection.clone(),
			quick_action_keys_enabled.clone(),
			autoload_mode.clone(),
			sort_order_cell.clone(),
			timer.clone(),
		);
		app_shell.attach_destroy(&frame);
		frame.show(true);
		frame.centre();
	});
}
