#![windows_subsystem = "windows"]

mod auth;
mod config;
mod dialogs;
mod error;
mod html;
mod live_region;
mod mastodon;
mod network;
mod streaming;
mod timeline;

use std::{
	backtrace::Backtrace,
	cell::Cell,
	fs::OpenOptions,
	io::Write,
	path::PathBuf,
	rc::Rc,
	sync::mpsc,
	time::{SystemTime, UNIX_EPOCH},
};

use url::Url;
use wxdragon::prelude::*;

use crate::{
	config::{Account, Config, SortOrder, TimestampFormat},
	mastodon::{MastodonClient, PollLimits, Status},
	network::{NetworkCommand, NetworkHandle, NetworkResponse, TimelineData},
	timeline::{Timeline, TimelineEntry, TimelineManager, TimelineType},
};

const ID_NEW_POST: i32 = 1001;
const ID_REPLY: i32 = 1002;
const ID_FAVOURITE: i32 = 1003;
const ID_BOOST: i32 = 1004;
const ID_LOCAL_TIMELINE: i32 = 1005;
const ID_FEDERATED_TIMELINE: i32 = 1006;
const ID_CLOSE_TIMELINE: i32 = 1007;
const ID_REFRESH: i32 = 1008;
const ID_REPLY_AUTHOR: i32 = 1009;
const ID_OPTIONS: i32 = 1010;
const ID_MANAGE_ACCOUNTS: i32 = 1011;
const ID_VIEW_PROFILE: i32 = 1012;
const ID_VIEW_USER_TIMELINE: i32 = 1013;
const KEY_DELETE: i32 = 127;

fn log_path() -> PathBuf {
	if let Ok(appdata) = std::env::var("APPDATA") {
		return PathBuf::from(appdata).join("Fedra").join("crash.log");
	}
	std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")).join("crash.log")
}

fn log_event(message: &str) {
	let millis = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis();
	let path = log_path();
	if let Some(parent) = path.parent() {
		let _ = std::fs::create_dir_all(parent);
	}
	if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
		let _ = writeln!(file, "[{}] {}", millis, message);
	}
}

fn reset_log() {
	let path = log_path();
	if let Some(parent) = path.parent() {
		let _ = std::fs::create_dir_all(parent);
	}
	let _ = OpenOptions::new().create(true).write(true).truncate(true).open(path);
}

fn install_panic_hook() {
	std::panic::set_hook(Box::new(|info| {
		log_event(&format!("panic: {}", info));
		let backtrace = Backtrace::force_capture();
		log_event(&format!("backtrace: {backtrace}"));
	}));
}

struct AppState {
	config: Config,
	timeline_manager: TimelineManager,
	network_handle: Option<NetworkHandle>,
	streaming_url: Option<Url>,
	access_token: Option<String>,
	max_post_chars: Option<usize>,
	poll_limits: PollLimits,
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
		}
	}

	fn active_account(&self) -> Option<&config::Account> {
		if let Some(id) = &self.config.active_account_id {
			self.config.accounts.iter().find(|a| &a.id == id)
		} else {
			self.config.accounts.first()
		}
	}

	fn active_account_mut(&mut self) -> Option<&mut config::Account> {
		if let Some(id) = self.config.active_account_id.clone() {
			self.config.accounts.iter_mut().find(|a| a.id == id)
		} else {
			self.config.accounts.first_mut()
		}
	}
}

enum UiCommand {
	NewPost,
	Reply { reply_all: bool },
	Favourite,
	Boost,
	Refresh,
	OpenTimeline(TimelineType),
	OpenUserTimeline,
	CloseTimeline,
	TimelineSelectionChanged(usize),
	TimelineEntrySelectionChanged(usize),
	ShowOptions,
	ManageAccounts,
	SwitchAccount(String),
	SwitchNextAccount,
	SwitchPrevAccount,
	RemoveAccount(String),
	ViewProfile,
}

fn setup_new_account(frame: &Frame) -> Option<Account> {
	let instance_url = dialogs::prompt_for_instance(frame)?;
	let client = match MastodonClient::new(instance_url.clone()) {
		Ok(client) => client,
		Err(err) => {
			dialogs::show_error(frame, &err);
			return None;
		}
	};
	let mut account = Account::new(instance_url.to_string());
	// Try OAuth with local listener
	if let Ok(result) = auth::oauth_with_local_listener(&client, "Fedra") {
		account.access_token = Some(result.access_token);
		account.client_id = Some(result.client_id);
		account.client_secret = Some(result.client_secret);
		return Some(account);
	}
	// Fall back to out-of-band OAuth
	if let Some(acc) = try_oob_oauth(frame, &client, &instance_url, &mut account) {
		return Some(acc);
	}
	// Fall back to manual token entry
	let token = dialogs::prompt_for_access_token(frame, &instance_url)?;
	account.access_token = Some(token);
	Some(account)
}

fn try_oob_oauth(frame: &Frame, client: &MastodonClient, instance_url: &Url, account: &mut Account) -> Option<Account> {
	let credentials = match client.register_app("Fedra", auth::OOB_REDIRECT_URI) {
		Ok(creds) => creds,
		Err(err) => {
			dialogs::show_error(frame, &err);
			return None;
		}
	};
	let authorize_url = match client.build_authorize_url(&credentials, auth::OOB_REDIRECT_URI) {
		Ok(url) => url,
		Err(err) => {
			dialogs::show_error(frame, &err);
			return None;
		}
	};
	let _ = launch_default_browser(authorize_url.as_str(), BrowserLaunchFlags::Default);
	let code = dialogs::prompt_for_oauth_code(frame, instance_url)?;
	let access_token = match client.exchange_token(&credentials, &code, auth::OOB_REDIRECT_URI) {
		Ok(token) => token,
		Err(err) => {
			dialogs::show_error(frame, &err);
			return None;
		}
	};
	account.access_token = Some(access_token);
	account.client_id = Some(credentials.client_id);
	account.client_secret = Some(credentials.client_secret);
	Some(account.clone())
}

fn build_menu_bar() -> MenuBar {
	let file_menu = Menu::builder()
		.append_item(ID_VIEW_PROFILE, "View &Profile\tCtrl+P", "View profile of selected post's author")
		.append_item(ID_MANAGE_ACCOUNTS, "Manage &Accounts...", "Add, remove or switch accounts")
		.append_separator()
		.append_item(ID_OPTIONS, "&Options\tCtrl+,", "Configure application settings")
		.build();
	let post_menu = Menu::builder()
		.append_item(ID_NEW_POST, "&New Post\tCtrl+N", "Create a new post")
		.append_item(ID_REPLY, "&Reply\tCtrl+R", "Reply to all mentioned users")
		.append_item(ID_REPLY_AUTHOR, "Reply to &Author\tCtrl+Shift+R", "Reply to author only")
		.append_separator()
		.append_item(ID_FAVOURITE, "&Favourite\tCtrl+Shift+F", "Favourite or unfavourite selected post")
		.append_item(ID_BOOST, "&Boost\tCtrl+Shift+B", "Boost or unboost selected post")
		.build();
	let timelines_menu = Menu::builder()
		.append_item(ID_VIEW_USER_TIMELINE, "&User Timeline\tCtrl+T", "Open timeline of selected post's author")
		.append_item(ID_LOCAL_TIMELINE, "&Local Timeline\tCtrl+L", "Open local timeline")
		.append_item(ID_FEDERATED_TIMELINE, "&Federated Timeline", "Open federated timeline")
		.append_separator()
		.append_item(ID_CLOSE_TIMELINE, "&Close Timeline", "Close current timeline")
		.append_separator()
		.append_item(ID_REFRESH, "&Refresh\tF5", "Refresh current timeline")
		.build();
	MenuBar::builder()
		.append(file_menu, "&File")
		.append(post_menu, "&Post")
		.append(timelines_menu, "&Timelines")
		.build()
}

fn refresh_timeline(state: &AppState, live_region: &StaticText) {
	let timeline_type = match state.timeline_manager.active() {
		Some(t) => t.timeline_type.clone(),
		None => return,
	};
	match &state.network_handle {
		Some(handle) => {
			handle.send(NetworkCommand::FetchTimeline { timeline_type, limit: Some(40) });
		}
		None => {
			live_region::announce(live_region, "Network not available");
		}
	}
}

fn update_timeline_ui(
	timeline_list: &ListBox,
	entries: &[TimelineEntry],
	sort_order: SortOrder,
	timestamp_format: TimestampFormat,
) {
	timeline_list.clear();
	let iter: Box<dyn Iterator<Item = &TimelineEntry>> = match sort_order {
		SortOrder::NewestToOldest => Box::new(entries.iter()),
		SortOrder::OldestToNewest => Box::new(entries.iter().rev()),
	};
	for entry in iter {
		timeline_list.append(&entry.display_text(timestamp_format));
	}
}

fn with_suppressed_selection<T>(suppress_selection: &Cell<bool>, f: impl FnOnce() -> T) -> T {
	suppress_selection.set(true);
	let result = f();
	suppress_selection.set(false);
	result
}

fn list_index_to_entry_index(list_index: usize, entries_len: usize, sort_order: SortOrder) -> Option<usize> {
	if list_index >= entries_len {
		return None;
	}
	match sort_order {
		SortOrder::NewestToOldest => Some(list_index),
		SortOrder::OldestToNewest => Some(entries_len - 1 - list_index),
	}
}

fn entry_index_to_list_index(entry_index: usize, entries_len: usize, sort_order: SortOrder) -> Option<usize> {
	if entry_index >= entries_len {
		return None;
	}
	match sort_order {
		SortOrder::NewestToOldest => Some(entry_index),
		SortOrder::OldestToNewest => Some(entries_len - 1 - entry_index),
	}
}

fn sync_timeline_selection_from_list(timeline: &mut Timeline, timeline_list: &ListBox, sort_order: SortOrder) {
	let selection = timeline_list.get_selection().map(|sel| sel as usize);
	timeline.selected_index = selection;
	timeline.selected_id = selection
		.and_then(|list_index| list_index_to_entry_index(list_index, timeline.entries.len(), sort_order))
		.map(|entry_index| timeline.entries[entry_index].id().to_string());
}

fn apply_timeline_selection(timeline_list: &ListBox, timeline: &mut Timeline, sort_order: SortOrder) {
	if timeline.entries.is_empty() {
		timeline.selected_index = None;
		timeline.selected_id = None;
		return;
	}
	let entries_len = timeline.entries.len();
	let selection = timeline
		.selected_id
		.as_deref()
		.and_then(|selected_id| {
			timeline
				.entries
				.iter()
				.position(|entry| entry.id() == selected_id)
				.and_then(|entry_index| entry_index_to_list_index(entry_index, entries_len, sort_order))
		})
		.or_else(|| timeline.selected_index.filter(|&sel| sel < entries_len))
		.unwrap_or_else(|| match sort_order {
			SortOrder::NewestToOldest => 0,
			SortOrder::OldestToNewest => entries_len - 1,
		});
	timeline.selected_index = Some(selection);
	timeline.selected_id = list_index_to_entry_index(selection, entries_len, sort_order)
		.map(|entry_index| timeline.entries[entry_index].id().to_string());
	timeline_list.set_selection(selection as u32, true);
}

fn update_active_timeline_ui(
	timeline_list: &ListBox,
	timeline: &mut Timeline,
	suppress_selection: &Cell<bool>,
	sort_order: SortOrder,
	timestamp_format: TimestampFormat,
) {
	with_suppressed_selection(suppress_selection, || {
		update_timeline_ui(timeline_list, &timeline.entries, sort_order, timestamp_format);
		apply_timeline_selection(timeline_list, timeline, sort_order);
	});
}

fn handle_ui_command(
	cmd: UiCommand,
	state: &mut AppState,
	frame: &Frame,
	timelines_selector: &ListBox,
	timeline_list: &ListBox,
	suppress_selection: &Cell<bool>,
	live_region: &StaticText,
) {
	match cmd {
		UiCommand::NewPost => {
			let (has_account, max_post_chars, poll_limits, enter_to_send) = (
				state.active_account().is_some(),
				state.max_post_chars,
				state.poll_limits.clone(),
				state.config.enter_to_send,
			);
			if !has_account {
				live_region::announce(live_region, "No account configured");
				return;
			}
			log_event("new_post: open dialog");
			let post = match dialogs::prompt_for_post(frame, max_post_chars, &poll_limits, enter_to_send) {
				Some(p) => p,
				None => return,
			};
			if let Some(handle) = &state.network_handle {
				handle.send(NetworkCommand::PostStatus {
					content: post.content,
					visibility: post.visibility.as_api_str().to_string(),
					spoiler_text: post.spoiler_text,
					content_type: post.content_type,
					media: post
						.media
						.into_iter()
						.map(|item| network::MediaUpload { path: item.path, description: item.description })
						.collect(),
					poll: post.poll.map(|poll| network::PollData {
						options: poll.options,
						expires_in: poll.expires_in,
						multiple: poll.multiple,
					}),
				});
			} else {
				live_region::announce(live_region, "Network not available");
			}
		}
		UiCommand::Reply { reply_all } => {
			let (status, max_post_chars, enter_to_send) =
				(get_selected_status(state).cloned(), state.max_post_chars, state.config.enter_to_send);
			let status = match status {
				Some(s) => s,
				None => {
					live_region::announce(live_region, "No post selected");
					return;
				}
			};
			let target = status.reblog.as_ref().map(|r| r.as_ref()).unwrap_or(&status);
			let self_acct = state.active_account().and_then(|account| account.acct.as_deref());
			let reply =
				match dialogs::prompt_for_reply(frame, target, max_post_chars, reply_all, self_acct, enter_to_send) {
					Some(r) => r,
					None => return,
				};
			if let Some(handle) = &state.network_handle {
				handle.send(NetworkCommand::Reply {
					in_reply_to_id: target.id.clone(),
					content: reply.content,
					visibility: reply.visibility.as_api_str().to_string(),
					spoiler_text: reply.spoiler_text,
				});
			} else {
				live_region::announce(live_region, "Network not available");
			}
		}
		UiCommand::Favourite => {
			do_favourite(state, live_region);
		}
		UiCommand::Boost => {
			do_boost(state, live_region);
		}
		UiCommand::Refresh => {
			refresh_timeline(state, live_region);
		}
		UiCommand::OpenTimeline(timeline_type) => {
			open_timeline(state, timelines_selector, timeline_list, timeline_type, suppress_selection, live_region);
		}
		UiCommand::CloseTimeline => {
			close_timeline(state, timelines_selector, timeline_list, suppress_selection, live_region);
		}
		UiCommand::TimelineSelectionChanged(index) => {
			if index < state.timeline_manager.len() {
				if let Some(active) = state.timeline_manager.active_mut() {
					sync_timeline_selection_from_list(active, timeline_list, state.config.sort_order);
				}
				state.timeline_manager.set_active(index);
				if let Some(active) = state.timeline_manager.active_mut() {
					update_active_timeline_ui(
						timeline_list,
						active,
						suppress_selection,
						state.config.sort_order,
						state.config.timestamp_format,
					);
				}
			}
		}
		UiCommand::TimelineEntrySelectionChanged(index) => {
			if let Some(active) = state.timeline_manager.active_mut() {
				active.selected_index = Some(index);
				active.selected_id = list_index_to_entry_index(index, active.entries.len(), state.config.sort_order)
					.map(|entry_index| active.entries[entry_index].id().to_string());
			}
		}
		UiCommand::ShowOptions => {
			if let Some((enter_to_send, sort_order, timestamp_format)) = dialogs::prompt_for_options(
				frame,
				state.config.enter_to_send,
				state.config.sort_order,
				state.config.timestamp_format,
			) {
				let needs_refresh =
					state.config.sort_order != sort_order || state.config.timestamp_format != timestamp_format;
				state.config.enter_to_send = enter_to_send;
				state.config.sort_order = sort_order;
				state.config.timestamp_format = timestamp_format;
				let store = config::ConfigStore::new();
				if let Err(err) = store.save(&state.config) {
					dialogs::show_error(frame, &err);
				}
				if needs_refresh && let Some(active) = state.timeline_manager.active_mut() {
					update_active_timeline_ui(
						timeline_list,
						active,
						suppress_selection,
						state.config.sort_order,
						state.config.timestamp_format,
					);
				}
			}
		}
		UiCommand::ManageAccounts => {
			let result = dialogs::prompt_manage_accounts(
				frame,
				&state.config.accounts,
				state.active_account().map(|a| a.id.as_str()),
			);
			match result {
				dialogs::ManageAccountsResult::Add => {
					if let Some(account) = setup_new_account(frame) {
						let id = account.id.clone();
						state.config.accounts.push(account);
						let _ = config::ConfigStore::new().save(&state.config);
						handle_ui_command(
							UiCommand::SwitchAccount(id),
							state,
							frame,
							timelines_selector,
							timeline_list,
							suppress_selection,
							live_region,
						);
					}
				}
				dialogs::ManageAccountsResult::Remove(id) => {
					handle_ui_command(
						UiCommand::RemoveAccount(id),
						state,
						frame,
						timelines_selector,
						timeline_list,
						suppress_selection,
						live_region,
					);
				}
				dialogs::ManageAccountsResult::Switch(id) => {
					handle_ui_command(
						UiCommand::SwitchAccount(id),
						state,
						frame,
						timelines_selector,
						timeline_list,
						suppress_selection,
						live_region,
					);
				}
				dialogs::ManageAccountsResult::None => {}
			}
		}
		UiCommand::SwitchAccount(id) => {
			if state.config.active_account_id.as_ref() == Some(&id) {
				return;
			}
			state.config.active_account_id = Some(id);
			let _ = config::ConfigStore::new().save(&state.config);
			switch_to_account(state, frame, timelines_selector, timeline_list, suppress_selection, live_region, true);
		}
		UiCommand::SwitchNextAccount => {
			if state.config.accounts.len() <= 1 {
				return;
			}
			let current_index = state
				.config
				.active_account_id
				.as_ref()
				.and_then(|id| state.config.accounts.iter().position(|a| &a.id == id))
				.unwrap_or(0);
			let next_index = (current_index + 1) % state.config.accounts.len();
			let next_id = state.config.accounts[next_index].id.clone();
			handle_ui_command(
				UiCommand::SwitchAccount(next_id),
				state,
				frame,
				timelines_selector,
				timeline_list,
				suppress_selection,
				live_region,
			);
		}
		UiCommand::SwitchPrevAccount => {
			if state.config.accounts.len() <= 1 {
				return;
			}
			let current_index = state
				.config
				.active_account_id
				.as_ref()
				.and_then(|id| state.config.accounts.iter().position(|a| &a.id == id))
				.unwrap_or(0);
			let prev_index = (current_index + state.config.accounts.len() - 1) % state.config.accounts.len();
			let prev_id = state.config.accounts[prev_index].id.clone();
			handle_ui_command(
				UiCommand::SwitchAccount(prev_id),
				state,
				frame,
				timelines_selector,
				timeline_list,
				suppress_selection,
				live_region,
			);
		}
		UiCommand::RemoveAccount(id) => {
			let is_active = state.config.active_account_id.as_ref() == Some(&id);
			state.config.accounts.retain(|a| a.id != id);
			if is_active {
				state.config.active_account_id = state.config.accounts.first().map(|a| a.id.clone());
			}
			let _ = config::ConfigStore::new().save(&state.config);
			if is_active {
				if state.config.accounts.is_empty() {
					if let Some(account) = setup_new_account(frame) {
						let id = account.id.clone();
						state.config.accounts.push(account);
						state.config.active_account_id = Some(id);
						let _ = config::ConfigStore::new().save(&state.config);
					} else {
						frame.close(true);
						return;
					}
				}
				switch_to_account(
					state,
					frame,
					timelines_selector,
					timeline_list,
					suppress_selection,
					live_region,
					true,
				);
			}
		}
		UiCommand::ViewProfile => {
			let status = match get_selected_status(state) {
				Some(s) => s,
				None => {
					live_region::announce(live_region, "No post selected");
					return;
				}
			};
			// Use the boosted status's account if this is a boost
			let account = status.reblog.as_ref().map(|r| &r.account).unwrap_or(&status.account);
			if dialogs::show_profile(frame, account) {
				let timeline_type =
					TimelineType::User { id: account.id.clone(), name: account.display_name_or_username().to_string() };
				open_timeline(state, timelines_selector, timeline_list, timeline_type, suppress_selection, live_region);
			}
		}
		UiCommand::OpenUserTimeline => {
			let status = match get_selected_status(state) {
				Some(s) => s,
				None => {
					live_region::announce(live_region, "No post selected");
					return;
				}
			};
			let account = status.reblog.as_ref().map(|r| &r.account).unwrap_or(&status.account);
			let timeline_type =
				TimelineType::User { id: account.id.clone(), name: account.display_name_or_username().to_string() };
			open_timeline(state, timelines_selector, timeline_list, timeline_type, suppress_selection, live_region);
		}
	}
}

fn switch_to_account(
	state: &mut AppState,
	frame: &Frame,
	timelines_selector: &ListBox,
	timeline_list: &ListBox,
	suppress_selection: &Cell<bool>,
	live_region: &StaticText,
	should_announce: bool,
) {
	for timeline in state.timeline_manager.iter_mut() {
		timeline.stream_handle = None;
	}
	state.network_handle = None;
	state.timeline_manager = TimelineManager::new();

	let (url, token) = match state.active_account().and_then(|a| {
		let url = Url::parse(&a.instance).ok()?;
		let token = a.access_token.clone()?;
		Some((url, token))
	}) {
		Some(val) => val,
		None => return,
	};

	state.streaming_url = Some(url.clone());
	state.access_token = Some(token.clone());
	state.network_handle = network::start_network(url.clone(), token.clone()).ok();

	if let Ok(client) = MastodonClient::new(url.clone()) {
		if let Ok(info) = client.get_instance_info() {
			state.max_post_chars = Some(info.max_post_chars);
			state.poll_limits = info.poll_limits;
		}
		if (state.active_account().and_then(|a| a.acct.as_deref()).is_none()
			|| state.active_account().and_then(|a| a.display_name.as_deref()).is_none())
			&& let Ok(account) = client.verify_credentials(&token)
			&& let Some(active) = state.active_account_mut()
		{
			active.acct = Some(account.acct);
			active.display_name = Some(account.display_name);
			let _ = config::ConfigStore::new().save(&state.config);
		}
	}

	state.timeline_manager.open(TimelineType::Home);
	state.timeline_manager.open(TimelineType::Notifications);
	state.timeline_manager.open(TimelineType::Local);

	if let Some(handle) = &state.network_handle {
		handle.send(NetworkCommand::FetchTimeline { timeline_type: TimelineType::Home, limit: Some(40) });
		handle.send(NetworkCommand::FetchTimeline { timeline_type: TimelineType::Notifications, limit: Some(40) });
		handle.send(NetworkCommand::FetchTimeline { timeline_type: TimelineType::Local, limit: Some(40) });
	}

	start_streaming_for_timeline(state, &TimelineType::Home);
	start_streaming_for_timeline(state, &TimelineType::Notifications);
	start_streaming_for_timeline(state, &TimelineType::Local);

	timelines_selector.clear();
	for name in state.timeline_manager.display_names() {
		timelines_selector.append(&name);
	}
	timelines_selector.set_selection(0_u32, true);

	with_suppressed_selection(suppress_selection, || {
		timeline_list.clear();
	});

	let (handle, title) = if let Some(account) = state.active_account() {
		let host =
			Url::parse(&account.instance).ok().and_then(|u| u.host_str().map(|s| s.to_string())).unwrap_or_default();
		let username = account.acct.as_deref().unwrap_or("?");
		let h = if username.contains('@') { format!("@{}", username) } else { format!("@{}@{}", username, host) };
		(h.clone(), format!("Fedra - {}", h))
	} else {
		("Unknown".to_string(), "Fedra".to_string())
	};

	if should_announce {
		live_region::announce(live_region, &format!("Switched to {}", handle));
	}
	frame.set_label(&title);
}

fn drain_ui_commands(
	ui_rx: &mpsc::Receiver<UiCommand>,
	state: &mut AppState,
	frame: &Frame,
	timelines_selector: &ListBox,
	timeline_list: &ListBox,
	suppress_selection: &Cell<bool>,
	live_region: &StaticText,
) {
	while let Ok(cmd) = ui_rx.try_recv() {
		handle_ui_command(cmd, state, frame, timelines_selector, timeline_list, suppress_selection, live_region);
	}
}

fn start_streaming_for_timeline(state: &mut AppState, timeline_type: &TimelineType) {
	let base_url = match &state.streaming_url {
		Some(url) => url.clone(),
		None => return,
	};
	let access_token = match &state.access_token {
		Some(t) => t.clone(),
		None => return,
	};
	let timeline = match state.timeline_manager.get_mut(timeline_type) {
		Some(t) => t,
		None => return,
	};
	if timeline.stream_handle.is_some() {
		return;
	}
	timeline.stream_handle = streaming::start_streaming(base_url, access_token, timeline_type.clone());
}

fn process_stream_events(state: &mut AppState, timeline_list: &ListBox, suppress_selection: &Cell<bool>) {
	let active_type = state.timeline_manager.active().map(|t| t.timeline_type.clone());
	let mut active_needs_update = false;
	for timeline in state.timeline_manager.iter_mut() {
		let handle = match &timeline.stream_handle {
			Some(h) => h,
			None => continue,
		};
		let events = handle.drain();
		let is_active = active_type.as_ref() == Some(&timeline.timeline_type);
		if is_active {
			sync_timeline_selection_from_list(timeline, timeline_list, state.config.sort_order);
		}
		for event in events {
			match event {
				streaming::StreamEvent::Update { timeline_type, status } => {
					if timeline.timeline_type == timeline_type {
						timeline.entries.insert(0, TimelineEntry::Status(*status));
						if is_active {
							active_needs_update = true;
						}
					}
				}
				streaming::StreamEvent::Delete { timeline_type, id } => {
					if timeline.timeline_type == timeline_type {
						timeline.entries.retain(|entry| entry.as_status().map(|s| s.id != id).unwrap_or(true));
						if is_active {
							active_needs_update = true;
						}
					}
				}
				streaming::StreamEvent::Notification { timeline_type, notification } => {
					if timeline.timeline_type == timeline_type {
						timeline.entries.insert(0, TimelineEntry::Notification(*notification));
						if is_active {
							active_needs_update = true;
						}
					}
				}
				streaming::StreamEvent::Connected(timeline_type) => {
					let _ = timeline_type;
				}
				streaming::StreamEvent::Disconnected(timeline_type) => {
					let _ = timeline_type;
				}
				streaming::StreamEvent::Error { timeline_type, message } => {
					let _ = (timeline_type, message);
				}
			}
		}
	}
	if active_needs_update && let Some(active) = state.timeline_manager.active_mut() {
		update_active_timeline_ui(
			timeline_list,
			active,
			suppress_selection,
			state.config.sort_order,
			state.config.timestamp_format,
		);
	}
}

fn process_network_responses(
	frame: &Frame,
	state: &mut AppState,
	timeline_list: &ListBox,
	suppress_selection: &Cell<bool>,
	live_region: &StaticText,
) {
	let handle = match &state.network_handle {
		Some(h) => h,
		None => return,
	};
	let active_type = state.timeline_manager.active().map(|t| t.timeline_type.clone());
	for response in handle.drain() {
		match response {
			NetworkResponse::TimelineLoaded { timeline_type, result: Ok(data) } => {
				let is_active = active_type.as_ref() == Some(&timeline_type);
				if let Some(timeline) = state.timeline_manager.get_mut(&timeline_type) {
					if is_active {
						sync_timeline_selection_from_list(timeline, timeline_list, state.config.sort_order);
					}
					timeline.entries = match data {
						TimelineData::Statuses(statuses) => statuses.into_iter().map(TimelineEntry::Status).collect(),
						TimelineData::Notifications(notifications) => {
							notifications.into_iter().map(TimelineEntry::Notification).collect()
						}
					};
					if is_active {
						update_active_timeline_ui(
							timeline_list,
							timeline,
							suppress_selection,
							state.config.sort_order,
							state.config.timestamp_format,
						);
					}
				}
			}
			NetworkResponse::TimelineLoaded { result: Err(ref err), .. } => {
				live_region::announce(live_region, &format!("Failed to load timeline: {}", error::user_message(err)));
			}
			NetworkResponse::PostComplete(Ok(())) => {
				live_region::announce(live_region, "Posted");
			}
			NetworkResponse::PostComplete(Err(ref err)) => {
				live_region::announce(live_region, &format!("Failed to post: {}", error::user_message(err)));
			}
			NetworkResponse::Favourited { status_id, result: Ok(status) } => {
				update_status_in_timelines(state, &status_id, |s| {
					s.favourited = status.favourited;
					s.favourites_count = status.favourites_count;
				});
				live_region::announce(live_region, "Favourited");
			}
			NetworkResponse::Favourited { result: Err(ref err), .. } => {
				live_region::announce(live_region, &format!("Failed to favourite: {}", error::user_message(err)));
			}
			NetworkResponse::Unfavourited { status_id, result: Ok(status) } => {
				update_status_in_timelines(state, &status_id, |s| {
					s.favourited = status.favourited;
					s.favourites_count = status.favourites_count;
				});
				live_region::announce(live_region, "Unfavourited");
			}
			NetworkResponse::Unfavourited { result: Err(ref err), .. } => {
				live_region::announce(live_region, &format!("Failed to unfavourite: {}", error::user_message(err)));
			}
			NetworkResponse::Boosted { status_id, result: Ok(status) } => {
				// The returned status is the reblog wrapper, get the inner status
				if let Some(inner) = &status.reblog {
					update_status_in_timelines(state, &status_id, |s| {
						s.reblogged = inner.reblogged;
						s.reblogs_count = inner.reblogs_count;
					});
				}
				live_region::announce(live_region, "Boosted");
			}
			NetworkResponse::Boosted { result: Err(ref err), .. } => {
				live_region::announce(live_region, &format!("Failed to boost: {}", error::user_message(err)));
			}
			NetworkResponse::Unboosted { status_id, result: Ok(status) } => {
				update_status_in_timelines(state, &status_id, |s| {
					s.reblogged = status.reblogged;
					s.reblogs_count = status.reblogs_count;
				});
				live_region::announce(live_region, "Unboosted");
			}
			NetworkResponse::Unboosted { result: Err(ref err), .. } => {
				live_region::announce(live_region, &format!("Failed to unboost: {}", error::user_message(err)));
			}
			NetworkResponse::Replied(Ok(())) => {
				live_region::announce(live_region, "Reply sent");
			}
			NetworkResponse::Replied(Err(ref err)) => {
				live_region::announce(live_region, &format!("Failed to reply: {}", error::user_message(err)));
			}
		}
	}
	let _ = frame;
}

fn update_status_in_timelines<F>(state: &mut AppState, status_id: &str, updater: F)
where
	F: Fn(&mut Status),
{
	for timeline in state.timeline_manager.iter_mut() {
		for entry in &mut timeline.entries {
			if let Some(status) = entry.as_status_mut() {
				// Check the status itself
				if status.id == status_id {
					updater(status);
				}
				// Check if it's a reblog of the target
				if let Some(ref mut reblog) = status.reblog
					&& reblog.id == status_id
				{
					updater(reblog);
				}
			}
		}
	}
}

fn open_timeline(
	state: &mut AppState,
	selector: &ListBox,
	timeline_list: &ListBox,
	timeline_type: TimelineType,
	suppress_selection: &Cell<bool>,
	live_region: &StaticText,
) {
	if !state.timeline_manager.open(timeline_type.clone()) {
		live_region::announce(live_region, "Timeline already open");
		return;
	}
	selector.append(&timeline_type.display_name());
	let new_index = state.timeline_manager.len() - 1;
	state.timeline_manager.set_active(new_index);
	with_suppressed_selection(suppress_selection, || {
		selector.set_selection(new_index as u32, true);
	});
	if let Some(handle) = &state.network_handle {
		handle.send(NetworkCommand::FetchTimeline { timeline_type: timeline_type.clone(), limit: Some(40) });
	}
	start_streaming_for_timeline(state, &timeline_type);
	with_suppressed_selection(suppress_selection, || {
		timeline_list.clear();
	});
}

fn get_selected_status(state: &AppState) -> Option<&Status> {
	let timeline = state.timeline_manager.active()?;
	let index = timeline.selected_index?;

	let final_index = match state.config.sort_order {
		crate::config::SortOrder::NewestToOldest => index,
		crate::config::SortOrder::OldestToNewest => timeline.entries.len().checked_sub(1)?.checked_sub(index)?,
	};

	timeline.entries.get(final_index)?.as_status()
}

fn do_favourite(state: &AppState, live_region: &StaticText) {
	let status = match get_selected_status(state) {
		Some(s) => s,
		None => {
			live_region::announce(live_region, "No post selected");
			return;
		}
	};
	let handle = match &state.network_handle {
		Some(h) => h,
		None => {
			live_region::announce(live_region, "Network not available");
			return;
		}
	};
	// Get the actual status to interact with (unwrap reblog if present)
	let target = status.reblog.as_ref().map(|r| r.as_ref()).unwrap_or(status);
	let status_id = target.id.clone();
	if target.favourited {
		handle.send(NetworkCommand::Unfavourite { status_id });
	} else {
		handle.send(NetworkCommand::Favourite { status_id });
	}
}

fn do_boost(state: &AppState, live_region: &StaticText) {
	let status = match get_selected_status(state) {
		Some(s) => s,
		None => {
			live_region::announce(live_region, "No post selected");
			return;
		}
	};
	let handle = match &state.network_handle {
		Some(h) => h,
		None => {
			live_region::announce(live_region, "Network not available");
			return;
		}
	};
	// Get the actual status to interact with (unwrap reblog if present)
	let target = status.reblog.as_ref().map(|r| r.as_ref()).unwrap_or(status);
	let status_id = target.id.clone();
	if target.reblogged {
		handle.send(NetworkCommand::Unboost { status_id });
	} else {
		handle.send(NetworkCommand::Boost { status_id });
	}
}

fn close_timeline(
	state: &mut AppState,
	selector: &ListBox,
	timeline_list: &ListBox,
	suppress_selection: &Cell<bool>,
	live_region: &StaticText,
) {
	let active_type = match state.timeline_manager.active() {
		Some(t) => t.timeline_type.clone(),
		None => return,
	};
	if !active_type.is_closeable() {
		live_region::announce(live_region, &format!("Cannot close the {} timeline", active_type.display_name()));
		return;
	}
	if !state.timeline_manager.close(&active_type) {
		return;
	}
	selector.clear();
	for name in state.timeline_manager.display_names() {
		selector.append(&name);
	}
	let active_index = state.timeline_manager.active_index();
	with_suppressed_selection(suppress_selection, || {
		selector.set_selection(active_index as u32, true);
	});
	if let Some(active) = state.timeline_manager.active_mut() {
		update_active_timeline_ui(
			timeline_list,
			active,
			suppress_selection,
			state.config.sort_order,
			state.config.timestamp_format,
		);
	}
}

fn main() {
	let _ = wxdragon::main(|_| {
		reset_log();
		install_panic_hook();
		log_event("app_start");
		let frame = Frame::builder().with_title("Fedra").with_size(Size::new(800, 600)).build();
		wxdragon::app::set_top_window(&frame);
		let menu_bar = build_menu_bar();
		frame.set_menu_bar(menu_bar);
		let panel = Panel::builder(&frame).build();
		// live region
		let live_region_label = StaticText::builder(&panel).with_size(Size::new(1, 1)).build();
		live_region_label.show(false);
		live_region::set_live_region(&live_region_label);
		let sizer = BoxSizer::builder(Orientation::Horizontal).build();
		let timelines_label = StaticText::builder(&panel).with_label("Timelines").build();
		let timelines_selector = ListBox::builder(&panel).with_choices(vec!["Home".to_string()]).build();
		timelines_selector.set_selection(0_u32, true);
		let timeline_list = ListBox::builder(&panel).build();
		let timelines_sizer = BoxSizer::builder(Orientation::Vertical).build();
		timelines_sizer.add(&timelines_label, 0, SizerFlag::All, 8);
		timelines_sizer.add(
			&timelines_selector,
			1,
			SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Bottom,
			8,
		);
		sizer.add_sizer(&timelines_sizer, 1, SizerFlag::Expand, 0);
		sizer.add(&timeline_list, 3, SizerFlag::Expand | SizerFlag::All, 8);
		panel.set_sizer(sizer, true);
		let frame_sizer = BoxSizer::builder(Orientation::Vertical).build();
		frame_sizer.add(&panel, 1, SizerFlag::Expand | SizerFlag::All, 0);
		frame.set_sizer(frame_sizer, true);

		let (ui_tx, ui_rx) = mpsc::channel();
		let is_shutting_down = Rc::new(Cell::new(false));
		let suppress_selection = Rc::new(Cell::new(false));
		let timer_busy = Rc::new(Cell::new(false));

		let store = config::ConfigStore::new();
		let mut config = store.load();
		if config.accounts.is_empty() {
			match setup_new_account(&frame) {
				Some(account) => {
					config.active_account_id = Some(account.id.clone());
					config.accounts.push(account);
					let _ = store.save(&config);
				}
				None => {
					frame.close(true);
					return;
				}
			}
		}

		let mut state = AppState::new(config);

		switch_to_account(
			&mut state,
			&frame,
			&timelines_selector,
			&timeline_list,
			&suppress_selection,
			&live_region_label,
			false,
		);
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
			);
			process_stream_events(&mut state, &timeline_list_timer, &suppress_timer);
			process_network_responses(
				&frame_timer,
				&mut state,
				&timeline_list_timer,
				&suppress_timer,
				&live_region_timer,
			);
			busy_timer.set(false);
		});
		timer.start(100, false);
		let ui_tx_selector = ui_tx.clone();
		let shutdown_selector = is_shutting_down.clone();
		let suppress_selector = suppress_selection.clone();
		timelines_selector.on_selection_changed(move |event| {
			if shutdown_selector.get() {
				return;
			}
			if suppress_selector.get() {
				return;
			}
			if let Some(index) = event.get_selection()
				&& index >= 0
			{
				let _ = ui_tx_selector.send(UiCommand::TimelineSelectionChanged(index as usize));
			}
		});
		let ui_tx_delete = ui_tx.clone();
		let shutdown_delete = is_shutting_down.clone();
		let timelines_selector_delete = timelines_selector;
		timelines_selector_delete.on_key_down(move |event| {
			if shutdown_delete.get() {
				return;
			}
			if let WindowEventData::Keyboard(ref key_event) = event {
				if key_event.control_down() {
					match key_event.get_key_code() {
						Some(91) => {
							let _ = ui_tx_delete.send(UiCommand::SwitchPrevAccount);
							event.skip(false);
							return;
						}
						Some(93) => {
							let _ = ui_tx_delete.send(UiCommand::SwitchNextAccount);
							event.skip(false);
							return;
						}
						_ => {}
					}
				}
				if key_event.get_key_code() == Some(KEY_DELETE) {
					let _ = ui_tx_delete.send(UiCommand::CloseTimeline);
					event.skip(false);
				} else {
					event.skip(true);
				}
			} else {
				event.skip(true);
			}
		});
		let ui_tx_list = ui_tx.clone();
		let shutdown_list = is_shutting_down.clone();
		let suppress_list = suppress_selection.clone();
		let timeline_list_state = timeline_list;
		let ui_tx_list_key = ui_tx.clone();
		let shutdown_list_key = is_shutting_down.clone();
		timeline_list_state.on_key_down(move |event| {
			if shutdown_list_key.get() {
				return;
			}
			if let WindowEventData::Keyboard(ref key_event) = event
				&& key_event.control_down()
			{
				match key_event.get_key_code() {
					Some(91) => {
						let _ = ui_tx_list_key.send(UiCommand::SwitchPrevAccount);
						event.skip(false);
						return;
					}
					Some(93) => {
						let _ = ui_tx_list_key.send(UiCommand::SwitchNextAccount);
						event.skip(false);
						return;
					}
					_ => {}
				}
			}
			event.skip(true);
		});

		timeline_list_state.on_selection_changed(move |event| {
			if shutdown_list.get() {
				return;
			}
			if suppress_list.get() {
				return;
			}
			if let Some(selection) = event.get_selection()
				&& selection >= 0
			{
				let _ = ui_tx_list.send(UiCommand::TimelineEntrySelectionChanged(selection as usize));
			}
		});
		let ui_tx_menu = ui_tx.clone();
		let shutdown_menu = is_shutting_down.clone();
		frame.on_key_down(move |event| {
			if shutdown_menu.get() {
				return;
			}
			if let WindowEventData::Keyboard(ref key_event) = event {
				if key_event.control_down() {
					match key_event.get_key_code() {
						Some(91) => {
							let _ = ui_tx_menu.send(UiCommand::SwitchPrevAccount);
						}
						Some(93) => {
							let _ = ui_tx_menu.send(UiCommand::SwitchNextAccount);
						}
						_ => event.skip(true),
					}
				} else {
					event.skip(true);
				}
			} else {
				event.skip(true);
			}
		});

		let ui_tx_menu = ui_tx.clone();
		let shutdown_menu = is_shutting_down.clone();
		frame.on_menu_selected(move |event| match event.get_id() {
			ID_VIEW_PROFILE => {
				if shutdown_menu.get() {
					return;
				}
				let _ = ui_tx_menu.send(UiCommand::ViewProfile);
			}
			ID_OPTIONS => {
				if shutdown_menu.get() {
					return;
				}
				let _ = ui_tx_menu.send(UiCommand::ShowOptions);
			}
			ID_MANAGE_ACCOUNTS => {
				if shutdown_menu.get() {
					return;
				}
				let _ = ui_tx_menu.send(UiCommand::ManageAccounts);
			}
			ID_NEW_POST => {
				if shutdown_menu.get() {
					return;
				}
				let _ = ui_tx_menu.send(UiCommand::NewPost);
			}
			ID_REPLY => {
				if shutdown_menu.get() {
					return;
				}
				let _ = ui_tx_menu.send(UiCommand::Reply { reply_all: true });
			}
			ID_REPLY_AUTHOR => {
				if shutdown_menu.get() {
					return;
				}
				let _ = ui_tx_menu.send(UiCommand::Reply { reply_all: false });
			}
			ID_FAVOURITE => {
				if shutdown_menu.get() {
					return;
				}
				let _ = ui_tx_menu.send(UiCommand::Favourite);
			}
			ID_BOOST => {
				if shutdown_menu.get() {
					return;
				}
				let _ = ui_tx_menu.send(UiCommand::Boost);
			}
			ID_REFRESH => {
				if shutdown_menu.get() {
					return;
				}
				let _ = ui_tx_menu.send(UiCommand::Refresh);
			}
			ID_VIEW_USER_TIMELINE => {
				if shutdown_menu.get() {
					return;
				}
				let _ = ui_tx_menu.send(UiCommand::OpenUserTimeline);
			}
			ID_LOCAL_TIMELINE => {
				if shutdown_menu.get() {
					return;
				}
				let _ = ui_tx_menu.send(UiCommand::OpenTimeline(TimelineType::Local));
			}
			ID_FEDERATED_TIMELINE => {
				if shutdown_menu.get() {
					return;
				}
				let _ = ui_tx_menu.send(UiCommand::OpenTimeline(TimelineType::Federated));
			}
			ID_CLOSE_TIMELINE => {
				if shutdown_menu.get() {
					return;
				}
				let _ = ui_tx_menu.send(UiCommand::CloseTimeline);
			}
			_ => {}
		});
		let shutdown_close = is_shutting_down.clone();
		let timer_close = timer.clone();
		frame.on_close(move |event| {
			if !shutdown_close.get() {
				log_event("app_close_requested");
				shutdown_close.set(true);
				timer_close.stop();
			}
			event.skip(true);
		});
		frame.show(true);
		frame.centre();
	});
}
