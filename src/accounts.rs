use std::{cell::Cell, string::ToString, sync::mpsc, thread};

use url::Url;
use wxdragon::prelude::*;

use crate::{
	AppState, UiCommand, auth,
	config::{Account, ConfigStore},
	live_region,
	mastodon::MastodonClient,
	network::{self, NetworkCommand},
	streaming,
	timeline::{TimelineManager, TimelineType},
	ui::{dialogs, menu::update_menu_labels, timeline_view::with_suppressed_selection},
};

pub fn start_add_account_flow(frame: &Frame, ui_tx: &mpsc::Sender<UiCommand>, state: &mut AppState) -> bool {
	let Some(instance_url) = dialogs::prompt_for_instance(frame) else { return false };
	let client = match MastodonClient::new(instance_url.clone()) {
		Ok(client) => client,
		Err(err) => {
			dialogs::show_error(frame, &err);
			return true;
		}
	};
	let instance_url_clone = instance_url;
	let ui_tx_thread = ui_tx.clone();
	thread::spawn(move || {
		let result = auth::oauth_with_local_listener(&client, "Fedra").map_err(|e| e.to_string());
		let _ = ui_tx_thread.send(UiCommand::OAuthResult { result, instance_url: instance_url_clone });
	});
	let dialog = Dialog::builder(frame, "Authentication").with_size(300, 150).build();
	let panel = Panel::builder(&dialog).build();
	let sizer = BoxSizer::builder(Orientation::Vertical).build();
	let label = StaticText::builder(&panel)
		.with_label("Waiting for authentication in browser...\nPlease complete the login process.")
		.build();
	let cancel_button = Button::builder(&panel).with_label("Cancel").build();
	let ui_tx_cancel = ui_tx.clone();
	cancel_button.on_click(move |_| {
		let _ = ui_tx_cancel.send(UiCommand::CancelAuth);
	});
	sizer.add(&label, 1, SizerFlag::Expand | SizerFlag::All, 20);
	sizer.add(&cancel_button, 0, SizerFlag::AlignRight | SizerFlag::All, 10);
	panel.set_sizer(sizer, true);
	dialog.show(true);
	state.pending_auth_dialog = Some(dialog);
	true
}

pub fn try_oob_oauth(
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

pub fn switch_to_account(
	state: &mut AppState,
	frame: &Frame,
	timelines_selector: ListBox,
	timeline_list: ListBox,
	suppress_selection: &Cell<bool>,
	live_region: StaticText,
	should_announce: bool,
) {
	for timeline in state.timeline_manager.iter_mut() {
		timeline.stream_handle = None;
	}
	state.network_handle = None;
	state.timeline_manager = TimelineManager::new();
	state.cw_expanded.clear();
	let Some((url, token)) = state.active_account().and_then(|a| {
		let url = Url::parse(&a.instance).ok()?;
		let token = a.access_token.clone()?;
		Some((url, token))
	}) else {
		return;
	};
	state.streaming_url = Some(url.clone());
	state.access_token = Some(token.clone());
	state.network_handle = network::start_network(url.clone(), token.clone()).ok();
	if let Ok(client) = MastodonClient::new(url) {
		state.client = Some(client.clone());
		if let Ok(info) = client.get_instance_info() {
			state.max_post_chars = Some(info.max_post_chars);
			state.poll_limits = info.poll_limits;
		}
		let needs_verify = state.active_account().and_then(|a| a.acct.as_deref()).is_none()
			|| state.active_account().and_then(|a| a.display_name.as_deref()).is_none()
			|| state.active_account().and_then(|a| a.user_id.as_deref()).is_none();
		if needs_verify {
			if let Ok(account) = client.verify_credentials(&token)
				&& let Some(active) = state.active_account_mut()
			{
				active.acct = Some(account.acct);
				active.display_name = Some(account.display_name);
				active.user_id = Some(account.id.clone());
				state.current_user_id = Some(account.id);
				let _ = ConfigStore::new().save(&state.config);
			}
		} else if let Some(active) = state.active_account() {
			state.current_user_id = active.user_id.clone();
		}
	}
	state.timeline_manager.open(TimelineType::Home);
	state.timeline_manager.open(TimelineType::Notifications);
	state.timeline_manager.open(TimelineType::Local);
	if let Some(handle) = &state.network_handle {
		handle.send(NetworkCommand::FetchTimeline { timeline_type: TimelineType::Home, limit: Some(40), max_id: None });
		handle.send(NetworkCommand::FetchTimeline {
			timeline_type: TimelineType::Notifications,
			limit: Some(40),
			max_id: None,
		});
		handle.send(NetworkCommand::FetchTimeline {
			timeline_type: TimelineType::Local,
			limit: Some(40),
			max_id: None,
		});
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
	let (handle, title) = state.active_account().map_or_else(
		|| ("Unknown".to_string(), "Fedra".to_string()),
		|account| {
			let host = Url::parse(&account.instance)
				.ok()
				.and_then(|u| u.host_str().map(ToString::to_string))
				.unwrap_or_default();
			let username = account.acct.as_deref().unwrap_or("?");
			let h = if username.contains('@') { format!("@{username}") } else { format!("@{username}@{host}") };
			(h.clone(), format!("Fedra - {h}"))
		},
	);
	if should_announce {
		live_region::announce(&live_region, &format!("Switched to {handle}"));
	}
	frame.set_label(&title);
	if let Some(mb) = frame.get_menu_bar() {
		update_menu_labels(&mb, state);
	}
}

pub fn start_streaming_for_timeline(state: &mut AppState, timeline_type: &TimelineType) {
	let base_url = match &state.streaming_url {
		Some(url) => url.clone(),
		None => return,
	};
	let access_token = match &state.access_token {
		Some(t) => t.clone(),
		None => return,
	};
	let Some(timeline) = state.timeline_manager.get_mut(timeline_type) else { return };
	timeline.stream_handle = streaming::start_streaming(base_url, access_token, timeline_type.clone());
}
