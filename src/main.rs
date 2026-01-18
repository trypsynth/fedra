#![windows_subsystem = "windows"]

mod auth;
mod config;
mod dialogs;
mod error;
mod mastodon;
mod network;
mod speech;
mod streaming;
mod timeline;

use std::{
	backtrace::Backtrace,
	cell::Cell,
	fs::OpenOptions,
	io::Write,
	mem,
	path::PathBuf,
	rc::Rc,
	sync::mpsc,
	time::{SystemTime, UNIX_EPOCH},
};

use url::Url;
use wxdragon::prelude::*;

use crate::{
	config::{Account, Config},
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
		self.config.accounts.first()
	}
}

enum UiCommand {
	NewPost,
	Reply { reply_all: bool },
	Favourite,
	Boost,
	Refresh,
	OpenTimeline(TimelineType),
	CloseTimeline,
	TimelineSelectionChanged(usize),
	TimelineEntrySelectionChanged(usize),
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
	let _ = webbrowser::open(authorize_url.as_str());
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
	let post_menu = Menu::builder()
		.append_item(ID_NEW_POST, "&New Post\tCtrl+N", "Create a new post")
		.append_item(ID_REPLY, "&Reply\tCtrl+R", "Reply to all mentioned users")
		.append_item(ID_REPLY_AUTHOR, "Reply to &Author\tCtrl+Shift+R", "Reply to author only")
		.append_separator()
		.append_item(ID_FAVOURITE, "&Favourite\tCtrl+Shift+F", "Favourite or unfavourite selected post")
		.append_item(ID_BOOST, "&Boost\tCtrl+Shift+B", "Boost or unboost selected post")
		.build();
	let timelines_menu = Menu::builder()
		.append_item(ID_LOCAL_TIMELINE, "&Local Timeline\tCtrl+L", "Open local timeline")
		.append_item(ID_FEDERATED_TIMELINE, "&Federated Timeline", "Open federated timeline")
		.append_separator()
		.append_item(ID_CLOSE_TIMELINE, "&Close Timeline", "Close current timeline")
		.append_separator()
		.append_item(ID_REFRESH, "&Refresh\tF5", "Refresh current timeline")
		.build();
	MenuBar::builder().append(post_menu, "&Post").append(timelines_menu, "&Timelines").build()
}

fn refresh_timeline(state: &AppState) {
	let timeline_type = match state.timeline_manager.active() {
		Some(t) => t.timeline_type.clone(),
		None => return,
	};
	match &state.network_handle {
		Some(handle) => {
			handle.send(NetworkCommand::FetchTimeline { timeline_type, limit: Some(40) });
		}
		None => {
			speech::speak("Network not available");
		}
	}
}

fn update_timeline_ui(timeline_list: &ListBox, entries: &[TimelineEntry]) {
	timeline_list.clear();
	for entry in entries.iter().rev() {
		timeline_list.append(&entry.display_text());
	}
}

fn with_suppressed_selection<T>(suppress_selection: &Cell<bool>, f: impl FnOnce() -> T) -> T {
	suppress_selection.set(true);
	let result = f();
	suppress_selection.set(false);
	result
}

fn apply_timeline_selection(timeline_list: &ListBox, timeline: &mut Timeline) {
	if timeline.entries.is_empty() {
		timeline.selected_index = None;
		return;
	}
	let selection = match timeline.selected_index {
		Some(sel) if sel < timeline.entries.len() => sel,
		_ => timeline.entries.len() - 1,
	};
	timeline.selected_index = Some(selection);
	timeline_list.set_selection(selection as u32, true);
}

fn update_active_timeline_ui(timeline_list: &ListBox, timeline: &mut Timeline, suppress_selection: &Cell<bool>) {
	with_suppressed_selection(suppress_selection, || {
		update_timeline_ui(timeline_list, &timeline.entries);
		apply_timeline_selection(timeline_list, timeline);
	});
}

fn handle_ui_command(
	cmd: UiCommand,
	state: &mut AppState,
	frame: &Frame,
	timelines_selector: &ListBox,
	timeline_list: &ListBox,
	suppress_selection: &Cell<bool>,
) {
	match cmd {
		UiCommand::NewPost => {
			let (has_account, max_post_chars, poll_limits) =
				(state.active_account().is_some(), state.max_post_chars, state.poll_limits.clone());
			if !has_account {
				speech::speak("No account configured");
				return;
			}
			log_event("new_post: open dialog");
			let post = match dialogs::prompt_for_post(frame, max_post_chars, &poll_limits) {
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
				speech::speak("Network not available");
			}
		}
		UiCommand::Reply { reply_all } => {
			let (status, max_post_chars) = (get_selected_status(state).cloned(), state.max_post_chars);
			let status = match status {
				Some(s) => s,
				None => {
					speech::speak("No post selected");
					return;
				}
			};
			let target = status.reblog.as_ref().map(|r| r.as_ref()).unwrap_or(&status);
			let reply = match dialogs::prompt_for_reply(frame, target, max_post_chars, reply_all) {
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
				speech::speak("Network not available");
			}
		}
		UiCommand::Favourite => {
			do_favourite(state);
		}
		UiCommand::Boost => {
			do_boost(state);
		}
		UiCommand::Refresh => {
			refresh_timeline(state);
		}
		UiCommand::OpenTimeline(timeline_type) => {
			open_timeline(state, timelines_selector, timeline_list, timeline_type, suppress_selection);
		}
		UiCommand::CloseTimeline => {
			close_timeline(state, timelines_selector, timeline_list, suppress_selection);
		}
		UiCommand::TimelineSelectionChanged(index) => {
			if index < state.timeline_manager.len() {
				if let Some(active) = state.timeline_manager.active_mut() {
					active.selected_index = timeline_list.get_selection().map(|sel| sel as usize);
				}
				state.timeline_manager.set_active(index);
				if let Some(active) = state.timeline_manager.active_mut() {
					update_active_timeline_ui(timeline_list, active, suppress_selection);
				}
			}
		}
		UiCommand::TimelineEntrySelectionChanged(index) => {
			if let Some(active) = state.timeline_manager.active_mut() {
				active.selected_index = Some(index);
			}
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
) {
	while let Ok(cmd) = ui_rx.try_recv() {
		handle_ui_command(cmd, state, frame, timelines_selector, timeline_list, suppress_selection);
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
	if active_needs_update {
		if let Some(active) = state.timeline_manager.active_mut() {
			active.selected_index = timeline_list.get_selection().map(|sel| sel as usize);
			update_active_timeline_ui(timeline_list, active, suppress_selection);
		}
	}
}

fn process_network_responses(
	frame: &Frame,
	state: &mut AppState,
	timeline_list: &ListBox,
	suppress_selection: &Cell<bool>,
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
						timeline.selected_index = timeline_list.get_selection().map(|sel| sel as usize);
					}
					timeline.entries = match data {
						TimelineData::Statuses(statuses) => statuses.into_iter().map(TimelineEntry::Status).collect(),
						TimelineData::Notifications(notifications) => {
							notifications.into_iter().map(TimelineEntry::Notification).collect()
						}
					};
					if is_active {
						update_active_timeline_ui(timeline_list, timeline, suppress_selection);
					}
				}
			}
			NetworkResponse::TimelineLoaded { result: Err(ref err), .. } => {
				speech::speak(&format!("Failed to load timeline: {}", error::user_message(err)));
			}
			NetworkResponse::PostComplete(Ok(())) => {
				speech::speak("Posted");
			}
			NetworkResponse::PostComplete(Err(ref err)) => {
				speech::speak(&format!("Failed to post: {}", error::user_message(err)));
			}
			NetworkResponse::Favourited { status_id, result: Ok(status) } => {
				update_status_in_timelines(state, &status_id, |s| {
					s.favourited = status.favourited;
					s.favourites_count = status.favourites_count;
				});
				speech::speak("Favourited");
			}
			NetworkResponse::Favourited { result: Err(ref err), .. } => {
				speech::speak(&format!("Failed to favourite: {}", error::user_message(err)));
			}
			NetworkResponse::Unfavourited { status_id, result: Ok(status) } => {
				update_status_in_timelines(state, &status_id, |s| {
					s.favourited = status.favourited;
					s.favourites_count = status.favourites_count;
				});
				speech::speak("Unfavourited");
			}
			NetworkResponse::Unfavourited { result: Err(ref err), .. } => {
				speech::speak(&format!("Failed to unfavourite: {}", error::user_message(err)));
			}
			NetworkResponse::Boosted { status_id, result: Ok(status) } => {
				// The returned status is the reblog wrapper, get the inner status
				if let Some(inner) = &status.reblog {
					update_status_in_timelines(state, &status_id, |s| {
						s.reblogged = inner.reblogged;
						s.reblogs_count = inner.reblogs_count;
					});
				}
				speech::speak("Boosted");
			}
			NetworkResponse::Boosted { result: Err(ref err), .. } => {
				speech::speak(&format!("Failed to boost: {}", error::user_message(err)));
			}
			NetworkResponse::Unboosted { status_id, result: Ok(status) } => {
				update_status_in_timelines(state, &status_id, |s| {
					s.reblogged = status.reblogged;
					s.reblogs_count = status.reblogs_count;
				});
				speech::speak("Unboosted");
			}
			NetworkResponse::Unboosted { result: Err(ref err), .. } => {
				speech::speak(&format!("Failed to unboost: {}", error::user_message(err)));
			}
			NetworkResponse::Replied(Ok(())) => {
				speech::speak("Reply sent");
			}
			NetworkResponse::Replied(Err(ref err)) => {
				speech::speak(&format!("Failed to reply: {}", error::user_message(err)));
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
				if let Some(ref mut reblog) = status.reblog {
					if reblog.id == status_id {
						updater(reblog);
					}
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
) {
	if !state.timeline_manager.open(timeline_type.clone()) {
		speech::speak("Timeline already open");
		return;
	}
	selector.append(timeline_type.display_name());
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
	// Timeline displays statuses in reverse order (newest at top)
	let reverse_index = timeline.entries.len().checked_sub(1)?.checked_sub(index)?;
	timeline.entries.get(reverse_index)?.as_status()
}

fn do_favourite(state: &AppState) {
	let status = match get_selected_status(state) {
		Some(s) => s,
		None => {
			speech::speak("No post selected");
			return;
		}
	};
	let handle = match &state.network_handle {
		Some(h) => h,
		None => {
			speech::speak("Network not available");
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

fn do_boost(state: &AppState) {
	let status = match get_selected_status(state) {
		Some(s) => s,
		None => {
			speech::speak("No post selected");
			return;
		}
	};
	let handle = match &state.network_handle {
		Some(h) => h,
		None => {
			speech::speak("Network not available");
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

fn close_timeline(state: &mut AppState, selector: &ListBox, timeline_list: &ListBox, suppress_selection: &Cell<bool>) {
	let active_type = match state.timeline_manager.active() {
		Some(t) => t.timeline_type.clone(),
		None => return,
	};
	if !active_type.is_closeable() {
		speech::speak(&format!("Cannot close the {} timeline", active_type.display_name()));
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
		update_active_timeline_ui(timeline_list, active, suppress_selection);
	}
}

fn main() {
	let _ = wxdragon::main(|_| {
		install_panic_hook();
		log_event("app_start");
		speech::init();
		let frame = Frame::builder().with_title("Fedra").with_size(Size::new(800, 600)).build();
		wxdragon::app::set_top_window(&frame);
		let menu_bar = build_menu_bar();
		frame.set_menu_bar(menu_bar);
		let panel = Panel::builder(&frame).build();
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
		let store = config::ConfigStore::new();
		let mut config = store.load();
		if config.accounts.is_empty() {
			match setup_new_account(&frame) {
				Some(account) => {
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
		state.timeline_manager.open(TimelineType::Home);
		state.timeline_manager.open(TimelineType::Notifications);
		state.timeline_manager.open(TimelineType::Local);
		let network_info = state.active_account().and_then(|account| {
			let url = Url::parse(&account.instance).ok()?;
			let token = account.access_token.clone()?;
			Some((url, token))
		});
		if let Some((url, token)) = network_info.clone() {
			if let Ok(client) = MastodonClient::new(url.clone()) {
				if let Ok(info) = client.get_instance_info() {
					state.max_post_chars = Some(info.max_post_chars);
					state.poll_limits = info.poll_limits;
				}
			}
			state.streaming_url = Some(url.clone());
			state.access_token = Some(token.clone());
			state.network_handle = network::start_network(url, token).ok();
		}
		if let Some(handle) = &state.network_handle {
			handle.send(NetworkCommand::FetchTimeline { timeline_type: TimelineType::Home, limit: Some(40) });
			handle.send(NetworkCommand::FetchTimeline { timeline_type: TimelineType::Notifications, limit: Some(40) });
			handle.send(NetworkCommand::FetchTimeline { timeline_type: TimelineType::Local, limit: Some(40) });
		}
		start_streaming_for_timeline(&mut state, &TimelineType::Home);
		start_streaming_for_timeline(&mut state, &TimelineType::Notifications);
		start_streaming_for_timeline(&mut state, &TimelineType::Local);
		timelines_selector.clear();
		for name in state.timeline_manager.display_names() {
			timelines_selector.append(&name);
		}
		timelines_selector.set_selection(0_u32, true);
		let (ui_tx, ui_rx) = mpsc::channel();
		let is_shutting_down = Rc::new(Cell::new(false));
		let suppress_selection = Rc::new(Cell::new(false));
		let timer_busy = Rc::new(Cell::new(false));
		let timer = Timer::new(&frame);
		let shutdown_timer = is_shutting_down.clone();
		let suppress_timer = suppress_selection.clone();
		let busy_timer = timer_busy.clone();
		let frame_timer = frame;
		let timelines_selector_timer = timelines_selector;
		let timeline_list_timer = timeline_list;
		let mut state = state;
		timer.on_tick(move |_| {
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
			);
			process_stream_events(&mut state, &timeline_list_timer, &suppress_timer);
			process_network_responses(&frame_timer, &mut state, &timeline_list_timer, &suppress_timer);
			busy_timer.set(false);
		});
		timer.start(100, false); // Check every 100ms
		// Keep timer alive, it would be dropped at end of scope otherwise
		mem::forget(timer);
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
			if let Some(index) = event.get_selection() {
				if index >= 0 {
					let _ = ui_tx_selector.send(UiCommand::TimelineSelectionChanged(index as usize));
				}
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
		timeline_list_state.on_selection_changed(move |event| {
			if shutdown_list.get() {
				return;
			}
			if suppress_list.get() {
				return;
			}
			if let Some(selection) = event.get_selection() {
				if selection >= 0 {
					let _ = ui_tx_list.send(UiCommand::TimelineEntrySelectionChanged(selection as usize));
				}
			}
		});
		let ui_tx_menu = ui_tx.clone();
		let shutdown_menu = is_shutting_down.clone();
		frame.on_menu_selected(move |event| match event.get_id() {
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
		frame.on_close(move |event| {
			if !shutdown_close.get() {
				log_event("app_close_requested");
				shutdown_close.set(true);
			}
			event.skip(true);
		});
		frame.show(true);
		frame.centre();
	});
}
