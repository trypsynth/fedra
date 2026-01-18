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

use std::{cell::RefCell, mem, rc::Rc};

use url::Url;
use wxdragon::prelude::*;

use crate::{
	config::{Account, Config},
	mastodon::{MastodonClient, PollLimits, Status},
	network::{NetworkCommand, NetworkHandle, NetworkResponse},
	timeline::{Timeline, TimelineManager, TimelineType},
};

const ID_NEW_POST: i32 = 1001;
const ID_REPLY: i32 = 1002;
const ID_FAVOURITE: i32 = 1003;
const ID_BOOST: i32 = 1004;
const ID_LOCAL_TIMELINE: i32 = 1005;
const ID_FEDERATED_TIMELINE: i32 = 1006;
const ID_CLOSE_TIMELINE: i32 = 1007;
const ID_REFRESH: i32 = 1008;
const KEY_DELETE: i32 = 127;

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
		.append_item(ID_REPLY, "&Reply\tCtrl+R", "Reply to selected post")
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

fn do_new_post(frame: &Frame, state: &AppState) {
	if state.active_account().is_none() {
		speech::speak("No account configured");
		return;
	}
	let post = match dialogs::prompt_for_post(frame, state.max_post_chars, &state.poll_limits) {
		Some(p) => p,
		None => return,
	};
	match &state.network_handle {
		Some(handle) => {
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
		}
		None => {
			speech::speak("Network not available");
		}
	}
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
		for status in &mut timeline.statuses {
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

fn open_timeline(
	state: &mut AppState,
	selector: &ListBox,
	timeline_list: &ListBox,
	timeline_type: TimelineType,
) {
	if !state.timeline_manager.open(timeline_type.clone()) {
		speech::speak("Timeline already open");
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

fn get_selected_status(state: &AppState) -> Option<&Status> {
	let timeline = state.timeline_manager.active()?;
	let index = timeline.selected_index?;
	// Timeline displays statuses in reverse order (newest at top)
	let reverse_index = timeline.statuses.len().checked_sub(1)?.checked_sub(index)?;
	timeline.statuses.get(reverse_index)
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

fn do_reply(frame: &Frame, state: &AppState) {
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
	// Get the actual status to reply to (unwrap reblog if present)
	let target = status.reblog.as_ref().map(|r| r.as_ref()).unwrap_or(status);
	let reply = match dialogs::prompt_for_reply(frame, target, state.max_post_chars) {
		Some(r) => r,
		None => return,
	};
	handle.send(NetworkCommand::Reply {
		in_reply_to_id: target.id.clone(),
		content: reply.content,
		visibility: reply.visibility.as_api_str().to_string(),
		spoiler_text: reply.spoiler_text,
	});
}

fn close_timeline(state: &mut AppState, selector: &ListBox, timeline_list: &ListBox) {
	let active_type = match state.timeline_manager.active() {
		Some(t) => t.timeline_type.clone(),
		None => return,
	};
	if !active_type.is_closeable() {
		speech::speak("Cannot close the Home timeline");
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
		let state_delete = state.clone();
		let timelines_selector_delete = timelines_selector;
		let timeline_list_delete = timeline_list_selector;
		timelines_selector_delete.on_key_down(move |event| {
			if let WindowEventData::Keyboard(ref key_event) = event {
				if key_event.get_key_code() == Some(KEY_DELETE) {
					close_timeline(&mut state_delete.borrow_mut(), &timelines_selector_delete, &timeline_list_delete);
					event.skip(false);
				} else {
					event.skip(true);
				}
			} else {
				event.skip(true);
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
			ID_REPLY => {
				do_reply(&frame, &state_menu.borrow());
			}
			ID_FAVOURITE => {
				do_favourite(&state_menu.borrow());
			}
			ID_BOOST => {
				do_boost(&state_menu.borrow());
			}
			ID_REFRESH => {
				refresh_timeline(&state_menu.borrow());
			}
			ID_LOCAL_TIMELINE => {
				open_timeline(&mut state_menu.borrow_mut(), &timelines_selector_menu, &timeline_list_menu, TimelineType::Local);
			}
			ID_FEDERATED_TIMELINE => {
				open_timeline(
					&mut state_menu.borrow_mut(),
					&timelines_selector_menu,
					&timeline_list_menu,
					TimelineType::Federated,
				);
			}
			ID_CLOSE_TIMELINE => {
				close_timeline(&mut state_menu.borrow_mut(), &timelines_selector_menu, &timeline_list_menu);
			}
			_ => {}
		});
		frame.show(true);
		frame.centre();
	});
}
