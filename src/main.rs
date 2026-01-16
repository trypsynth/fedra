#![windows_subsystem = "windows"]

mod auth;
mod config;
mod dialogs;
mod error;
mod mastodon;
mod streaming;

use std::{cell::RefCell, rc::Rc};

use url::Url;
use wxdragon::prelude::*;

use crate::{
	config::{Account, Config},
	mastodon::{MastodonClient, Status},
	streaming::StreamHandle,
};

const ID_NEW_POST: i32 = 1001;
const ID_REFRESH: i32 = 1002;

struct AppState {
	config: Config,
	statuses: Vec<Status>,
	stream_handle: Option<StreamHandle>,
	client: Option<MastodonClient>,
}

impl AppState {
	fn new(config: Config) -> Self {
		Self { config, statuses: Vec::new(), stream_handle: None, client: None }
	}

	fn active_account(&self) -> Option<&config::Account> {
		self.config.accounts.first()
	}

	fn access_token(&self) -> Option<&str> {
		self.active_account()?.access_token.as_deref()
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

fn try_oob_oauth(
	frame: &Frame,
	client: &MastodonClient,
	instance_url: &Url,
	account: &mut Account,
) -> Option<Account> {
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
	MenuBar::builder().append(post_menu, "&Post").build()
}

fn do_new_post(frame: &Frame, state: &AppState) {
	let account = match state.active_account() {
		Some(acc) => acc,
		None => {
			dialogs::show_error_msg(frame, "No account configured.");
			return;
		}
	};
	let access_token = match &account.access_token {
		Some(token) => token,
		None => {
			dialogs::show_error_msg(frame, "Account has no access token.");
			return;
		}
	};
	let content = match dialogs::prompt_for_post(frame) {
		Some(c) => c,
		None => return,
	};
	let instance_url = match Url::parse(&account.instance) {
		Ok(url) => url,
		Err(_) => {
			dialogs::show_error_msg(frame, "Invalid instance URL in account.");
			return;
		}
	};
	let client = match mastodon::MastodonClient::new(instance_url) {
		Ok(c) => c,
		Err(err) => {
			dialogs::show_error(frame, &err);
			return;
		}
	};
	match client.post_status(access_token, &content) {
		Ok(()) => {
			dialogs::show_info(frame, "Your post has been published!", "Posted");
		}
		Err(err) => {
			dialogs::show_error(frame, &err);
		}
	}
}

fn refresh_timeline(frame: &Frame, state: &mut AppState, timeline_list: &ListBox) {
	let client = match &state.client {
		Some(c) => c,
		None => {
			dialogs::show_error_msg(frame, "No client available.");
			return;
		}
	};
	let access_token = match state.access_token() {
		Some(t) => t.to_string(),
		None => {
			dialogs::show_error_msg(frame, "No access token available.");
			return;
		}
	};
	match client.get_home_timeline(&access_token, Some(40)) {
		Ok(statuses) => {
			state.statuses = statuses;
			update_timeline_ui(timeline_list, &state.statuses);
		}
		Err(err) => {
			dialogs::show_error(frame, &err);
		}
	}
}

fn update_timeline_ui(timeline_list: &ListBox, statuses: &[Status]) {
	timeline_list.clear();
	for status in statuses {
		timeline_list.append(&status.summary());
	}
}

fn start_streaming(state: &mut AppState) {
	let client = match &state.client {
		Some(c) => c,
		None => return,
	};
	let access_token = match state.access_token() {
		Some(t) => t,
		None => return,
	};
	let streaming_url = match client.streaming_url(access_token) {
		Ok(url) => url,
		Err(_) => return,
	};
	state.stream_handle = Some(streaming::start_streaming(streaming_url));
}

fn process_stream_events(state: &mut AppState, timeline_list: &ListBox) {
	let handle = match &state.stream_handle {
		Some(h) => h,
		None => return,
	};
	let events = handle.drain();
	let mut needs_update = false;
	for event in events {
		match event {
			streaming::StreamEvent::Update(status) => {
				state.statuses.insert(0, *status);
				needs_update = true;
			}
			streaming::StreamEvent::Delete(id) => {
				state.statuses.retain(|s| s.id != id);
				needs_update = true;
			}
			streaming::StreamEvent::Connected => {
				// Could show status indicator
			}
			streaming::StreamEvent::Disconnected => {
				// Could show status indicator
			}
			streaming::StreamEvent::Error(_msg) => {
				// Streaming failed, could fall back to polling
			}
		}
	}
	if needs_update {
		update_timeline_ui(timeline_list, &state.statuses);
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
		let timelines_selector = ListBox::builder(&panel)
			.with_choices(vec!["Home".to_string(), "Local".to_string(), "Federated".to_string()])
			.build();
		timelines_selector.set_selection(0, true);
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
		if let Some(account) = state.active_account()
			&& let Ok(url) = Url::parse(&account.instance)
		{
			state.client = mastodon::MastodonClient::new(url).ok();
		}
		refresh_timeline(&frame, &mut state, &timeline_list);
		start_streaming(&mut state);
		let state = Rc::new(RefCell::new(state));
		let timer = Timer::new(&frame);
		let state_timer = state.clone();
		timer.on_tick(move |_| {
			process_stream_events(&mut state_timer.borrow_mut(), &timeline_list);
		});
		timer.start(100, false); // Check every 100ms
		let state_menu = state.clone();
		frame.on_menu_selected(move |event| match event.get_id() {
			ID_NEW_POST => {
				do_new_post(&frame, &state_menu.borrow());
			}
			ID_REFRESH => {
				refresh_timeline(&frame, &mut state_menu.borrow_mut(), &timeline_list);
			}
			_ => {}
		});
		frame.show(true);
		frame.centre();
	});
}
