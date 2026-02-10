use std::{cell::Cell, string::ToString, thread};

use url::Url;
use wxdragon::prelude::*;

use crate::{
	AppState, UiCommand, auth,
	config::{Account, ConfigStore},
	mastodon::MastodonClient,
	network::{self, NetworkCommand},
	streaming,
	timeline::TimelineType,
	ui::{
		dialogs,
		menu::update_menu_labels,
		timeline_view::{update_active_timeline_ui, with_suppressed_selection},
	},
	ui_wake::UiCommandSender,
};

pub fn start_add_account_flow(frame: &Frame, ui_tx: &UiCommandSender, state: &mut AppState) -> bool {
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
	new_account_id: Option<String>,
) {
	if let Some(new_id) = new_account_id {
		if let Some(old_id) = state.config.active_account_id.clone() {
			for timeline in state.timeline_manager.iter_mut() {
				timeline.stream_handle = None;
			}
			state.account_timelines.insert(old_id.clone(), std::mem::take(&mut state.timeline_manager));
			state.account_cw_expanded.insert(old_id, std::mem::take(&mut state.cw_expanded));
		}
		state.config.active_account_id = Some(new_id);
		let _ = ConfigStore::new().save(&state.config);
	}

	state.network_handle = None;
	let active_id =
		state.config.active_account_id.clone().or_else(|| state.config.accounts.first().map(|a| a.id.clone()));

	if let Some(id) = &active_id {
		if let Some(mgr) = state.account_timelines.remove(id) {
			state.timeline_manager = mgr;
		}
		if let Some(cw) = state.account_cw_expanded.remove(id) {
			state.cw_expanded = cw;
		}
	}

	let Some((url, token)) = state.active_account().and_then(|a| {
		let url = Url::parse(&a.instance).ok()?;
		let token = a.access_token.clone()?;
		Some((url, token))
	}) else {
		return;
	};
	state.streaming_url = Some(url.clone());
	state.access_token = Some(token.clone());
	state.network_handle = network::start_network(url.clone(), token.clone(), state.ui_waker.clone()).ok();
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

	if state.timeline_manager.len() == 0 {
		state.timeline_manager.open(TimelineType::Home);
		state.timeline_manager.open(TimelineType::Notifications);
		let default_timelines = state.config.default_timelines.clone();
		for default in &default_timelines {
			let timeline_type = match default {
				crate::config::DefaultTimeline::Local => TimelineType::Local,
				crate::config::DefaultTimeline::Federated => TimelineType::Federated,
				crate::config::DefaultTimeline::Direct => TimelineType::Direct,
				crate::config::DefaultTimeline::Bookmarks => TimelineType::Bookmarks,
				crate::config::DefaultTimeline::Favorites => TimelineType::Favorites,
			};
			state.timeline_manager.open(timeline_type);
		}

		if let Some(handle) = &state.network_handle {
			handle.send(NetworkCommand::FetchTimeline {
				timeline_type: TimelineType::Home,
				limit: Some(40),
				max_id: None,
			});
			handle.send(NetworkCommand::FetchTimeline {
				timeline_type: TimelineType::Notifications,
				limit: Some(40),
				max_id: None,
			});
			for default in &default_timelines {
				let timeline_type = match default {
					crate::config::DefaultTimeline::Local => TimelineType::Local,
					crate::config::DefaultTimeline::Federated => TimelineType::Federated,
					crate::config::DefaultTimeline::Direct => TimelineType::Direct,
					crate::config::DefaultTimeline::Bookmarks => TimelineType::Bookmarks,
					crate::config::DefaultTimeline::Favorites => TimelineType::Favorites,
				};
				handle.send(NetworkCommand::FetchTimeline { timeline_type, limit: Some(40), max_id: None });
			}
		}
	}

	let timeline_types: Vec<TimelineType> =
		state.timeline_manager.iter_mut().map(|t| t.timeline_type.clone()).collect();
	for tt in timeline_types {
		start_streaming_for_timeline(state, &tt);
	}

	timelines_selector.clear();
	for name in state.timeline_manager.display_names() {
		timelines_selector.append(&name);
	}
	let active_index = state.timeline_manager.active_index();
	timelines_selector.set_selection(u32::try_from(active_index).unwrap(), true);

	with_suppressed_selection(suppress_selection, || {
		timeline_list.clear();
		if let Some(active) = state.timeline_manager.active_mut() {
			update_active_timeline_ui(
				timeline_list,
				active,
				suppress_selection,
				state.config.sort_order,
				state.config.timestamp_format,
				state.config.content_warning_display,
				&state.cw_expanded,
				state.config.preserve_thread_order,
			);
		}
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
		live_region::announce(live_region, &format!("Switched to {handle}"));
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
	timeline.stream_handle =
		streaming::start_streaming(&base_url, &access_token, timeline_type.clone(), state.ui_waker.clone());
}
