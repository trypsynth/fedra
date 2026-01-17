#![windows_subsystem = "windows"]

mod auth;
mod config;
mod dialogs;
mod error;
mod mastodon;
mod network;
mod streaming;
mod timeline;

use std::{cell::RefCell, mem, rc::Rc};

use url::Url;
use wxdragon::prelude::*;

use crate::{
	config::{Account, Config},
	mastodon::{MastodonClient, Status},
	network::{NetworkCommand, NetworkHandle, NetworkResponse},
	timeline::{Timeline, TimelineManager, TimelineType},
};

const ID_NEW_POST: i32 = 1001;
const ID_REFRESH: i32 = 1002;
const ID_LOCAL_TIMELINE: i32 = 1003;
const ID_FEDERATED_TIMELINE: i32 = 1004;
const ID_CLOSE_TIMELINE: i32 = 1005;

struct AppState {
	config: Config,
	timeline_manager: TimelineManager,
	network_handle: Option<NetworkHandle>,
	streaming_url: Option<Url>,
	access_token: Option<String>,
	max_post_chars: Option<usize>,
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
		}
	}

	fn active_account(&self) -> Option<&config::Account> {
		self.config.accounts.first()
	}
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
		.append_separator()
		.append_item(ID_REFRESH, "&Refresh\tF5", "Refresh timeline")
		.build();
	let timelines_menu = Menu::builder()
		.append_item(ID_LOCAL_TIMELINE, "&Local Timeline", "Open local timeline")
		.append_item(ID_FEDERATED_TIMELINE, "&Federated Timeline", "Open federated timeline")
		.append_separator()
		.append_item(ID_CLOSE_TIMELINE, "&Close Timeline", "Close current timeline")
		.build();
	MenuBar::builder().append(post_menu, "&Post").append(timelines_menu, "&Timelines").build()
}

fn do_new_post(frame: &Frame, state: &AppState) {
	if state.active_account().is_none() {
		dialogs::show_error_msg(frame, "No account configured.");
		return;
	}
	let post = match dialogs::prompt_for_post(frame, state.max_post_chars) {
		Some(p) => p,
		None => return,
	};
	match &state.network_handle {
		Some(handle) => {
			handle.send(NetworkCommand::PostStatus {
				content: post.content,
				visibility: post.visibility.as_api_str().to_string(),
			});
		}
		None => {
			dialogs::show_error_msg(frame, "Network not available.");
		}
	}
}

fn refresh_timeline(frame: &Frame, state: &AppState) {
	let timeline_type = match state.timeline_manager.active() {
		Some(t) => t.timeline_type.clone(),
		None => return,
	};
	match &state.network_handle {
		Some(handle) => {
			handle.send(NetworkCommand::FetchTimeline { timeline_type, limit: Some(40) });
		}
		None => {
			dialogs::show_error_msg(frame, "Network not available.");
		}
	}
}

fn update_timeline_ui(timeline_list: &ListBox, statuses: &[Status]) {
	timeline_list.clear();
	for status in statuses.iter().rev() {
		timeline_list.append(&status.timeline_display());
	}
}

fn apply_timeline_selection(timeline_list: &ListBox, timeline: &mut Timeline) {
	if timeline.statuses.is_empty() {
		timeline.selected_index = None;
		return;
	}
	let selection = match timeline.selected_index {
		Some(sel) if sel < timeline.statuses.len() => sel,
		_ => timeline.statuses.len() - 1,
	};
	timeline.selected_index = Some(selection);
	timeline_list.set_selection(selection as u32, true);
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

fn process_stream_events(state: &mut AppState, timeline_list: &ListBox) {
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
						timeline.statuses.insert(0, *status);
						if is_active {
							active_needs_update = true;
						}
					}
				}
				streaming::StreamEvent::Delete { timeline_type, id } => {
					if timeline.timeline_type == timeline_type {
						timeline.statuses.retain(|s| s.id != id);
						if is_active {
							active_needs_update = true;
						}
					}
				}
				streaming::StreamEvent::Connected(_)
				| streaming::StreamEvent::Disconnected(_)
				| streaming::StreamEvent::Error { .. } => {}
			}
		}
	}
	if active_needs_update {
		if let Some(active) = state.timeline_manager.active_mut() {
			active.selected_index = timeline_list.get_selection().map(|sel| sel as usize);
			update_timeline_ui(timeline_list, &active.statuses);
			apply_timeline_selection(timeline_list, active);
		}
	}
}

fn process_network_responses(frame: &Frame, state: &mut AppState, timeline_list: &ListBox) {
	let handle = match &state.network_handle {
		Some(h) => h,
		None => return,
	};
	let active_type = state.timeline_manager.active().map(|t| t.timeline_type.clone());
	for response in handle.drain() {
		match response {
			NetworkResponse::TimelineLoaded { timeline_type, result: Ok(statuses) } => {
				let is_active = active_type.as_ref() == Some(&timeline_type);
				if let Some(timeline) = state.timeline_manager.get_mut(&timeline_type) {
					if is_active {
						timeline.selected_index = timeline_list.get_selection().map(|sel| sel as usize);
					}
					timeline.statuses = statuses;
					if is_active {
						update_timeline_ui(timeline_list, &timeline.statuses);
						apply_timeline_selection(timeline_list, timeline);
					}
				}
			}
			NetworkResponse::TimelineLoaded { result: Err(ref err), .. } => {
				dialogs::show_error(frame, err);
			}
			NetworkResponse::PostComplete(Ok(())) => {
				dialogs::show_info(frame, "Your post has been published!", "Posted");
			}
			NetworkResponse::PostComplete(Err(ref err)) => {
				dialogs::show_error(frame, err);
			}
		}
	}
}

fn open_timeline(
	state: &mut AppState,
	frame: &Frame,
	selector: &ListBox,
	timeline_list: &ListBox,
	timeline_type: TimelineType,
) {
	if !state.timeline_manager.open(timeline_type.clone()) {
		dialogs::show_error_msg(frame, "That timeline is already open.");
		return;
	}
	selector.append(timeline_type.display_name());
	let new_index = state.timeline_manager.len() - 1;
	state.timeline_manager.set_active(new_index);
	selector.set_selection(new_index as u32, true);
	if let Some(handle) = &state.network_handle {
		handle.send(NetworkCommand::FetchTimeline { timeline_type: timeline_type.clone(), limit: Some(40) });
	}
	start_streaming_for_timeline(state, &timeline_type);
	timeline_list.clear();
}

fn close_timeline(state: &mut AppState, frame: &Frame, selector: &ListBox, timeline_list: &ListBox) {
	let active_type = match state.timeline_manager.active() {
		Some(t) => t.timeline_type.clone(),
		None => return,
	};
	if !active_type.is_closeable() {
		dialogs::show_error_msg(frame, "Cannot close the Home timeline.");
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
	selector.set_selection(active_index as u32, true);
	if let Some(active) = state.timeline_manager.active_mut() {
		update_timeline_ui(timeline_list, &active.statuses);
		apply_timeline_selection(timeline_list, active);
	}
}

fn main() {
	let _ = wxdragon::main(|_| {
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
				}
			}
			state.streaming_url = Some(url.clone());
			state.access_token = Some(token.clone());
			state.network_handle = network::start_network(url, token).ok();
		}
		if let Some(handle) = &state.network_handle {
			handle.send(NetworkCommand::FetchTimeline { timeline_type: TimelineType::Home, limit: Some(40) });
			handle.send(NetworkCommand::FetchTimeline { timeline_type: TimelineType::Local, limit: Some(40) });
		}
		start_streaming_for_timeline(&mut state, &TimelineType::Home);
		start_streaming_for_timeline(&mut state, &TimelineType::Local);
		timelines_selector.clear();
		for name in state.timeline_manager.display_names() {
			timelines_selector.append(&name);
		}
		timelines_selector.set_selection(0_u32, true);
		let state = Rc::new(RefCell::new(state));
		let timer = Timer::new(&frame);
		let state_timer = state.clone();
		let frame_timer = frame;
		timer.on_tick(move |_| {
			let mut state = state_timer.borrow_mut();
			process_stream_events(&mut state, &timeline_list);
			process_network_responses(&frame_timer, &mut state, &timeline_list);
		});
		timer.start(100, false); // Check every 100ms
		// Keep timer alive, it would be dropped at end of scope otherwise
		mem::forget(timer);
		let state_selector = state.clone();
		let timeline_list_selector = timeline_list;
		timelines_selector.on_selection_changed(move |event| {
			if let Some(index) = event.get_selection() {
				if index >= 0 {
					let mut state = state_selector.borrow_mut();
					if let Some(active) = state.timeline_manager.active_mut() {
						active.selected_index = timeline_list_selector.get_selection().map(|sel| sel as usize);
					}
					state.timeline_manager.set_active(index as usize);
					if let Some(active) = state.timeline_manager.active_mut() {
						update_timeline_ui(&timeline_list_selector, &active.statuses);
						apply_timeline_selection(&timeline_list_selector, active);
					}
				}
			}
		});
		let state_timeline_list = state.clone();
		let timeline_list_state = timeline_list_selector;
		timeline_list_state.on_selection_changed(move |event| {
			if let Some(selection) = event.get_selection() {
				if selection >= 0 {
					let mut state = state_timeline_list.borrow_mut();
					if let Some(active) = state.timeline_manager.active_mut() {
						active.selected_index = Some(selection as usize);
					}
				}
			}
		});
		let state_menu = state.clone();
		let timelines_selector_menu = timelines_selector;
		let timeline_list_menu = timeline_list_selector;
		frame.on_menu_selected(move |event| match event.get_id() {
			ID_NEW_POST => {
				do_new_post(&frame, &state_menu.borrow());
			}
			ID_REFRESH => {
				refresh_timeline(&frame, &state_menu.borrow());
			}
			ID_LOCAL_TIMELINE => {
				open_timeline(
					&mut state_menu.borrow_mut(),
					&frame,
					&timelines_selector_menu,
					&timeline_list_menu,
					TimelineType::Local,
				);
			}
			ID_FEDERATED_TIMELINE => {
				open_timeline(
					&mut state_menu.borrow_mut(),
					&frame,
					&timelines_selector_menu,
					&timeline_list_menu,
					TimelineType::Federated,
				);
			}
			ID_CLOSE_TIMELINE => {
				close_timeline(&mut state_menu.borrow_mut(), &frame, &timelines_selector_menu, &timeline_list_menu);
			}
			_ => {}
		});
		frame.show(true);
		frame.centre();
	});
}
