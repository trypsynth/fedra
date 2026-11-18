use std::{cell::Cell, string::ToString, thread};

use url::Url;
use wxdragon::prelude::*;

use crate::{
	AppState, UiCommand, auth,
	config::Account,
	mastodon::{MastodonClient, PollLimits},
	network::Session,
	responses::{fetch_instance_info, fetch_markers, fetch_notification_policy, search, timelines},
	streaming,
	timeline::TimelineType,
	ui::{dialogs, menu::update_menu_labels, timeline_view::with_suppressed_selection},
	ui_wake::UiCommandSender,
};

pub fn update_window_title(state: &AppState, frame: &Frame) {
	let handle = state.active_account().map_or_else(|| "Unknown".to_string(), Account::full_handle);
	let timeline =
		state.timeline_manager.active().map_or_else(|| "Unknown".to_string(), |t| t.timeline_type.display_name());
	let title = crate::template::render_window_title(
		&state.config.window_title_template,
		&crate::template::WindowTitleTemplateVars { app: "Fedra".to_string(), account: handle, timeline },
	);
	frame.set_label(&title);
}

pub fn start_add_account_flow(frame: &Frame, ui_tx: &UiCommandSender, state: &mut AppState) -> bool {
	let Some(instance_url) = dialogs::prompt_for_instance(frame) else { return false };
	continue_add_account_flow(frame, ui_tx, state, instance_url);
	true
}

/// Starts signing in to `instance_url`, once it's been asked for.
pub fn continue_add_account_flow(frame: &Frame, ui_tx: &UiCommandSender, state: &mut AppState, instance_url: Url) {
	let client = match MastodonClient::new(instance_url.clone()) {
		Ok(client) => client,
		Err(err) => {
			dialogs::show_error(frame, &err);
			return;
		}
	};
	tracing::info!(instance = instance_url.host_str(), "adding an account");
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
	tracing::info!("signing in by pasting a code instead");
	if !launch_default_browser(authorize_url.as_str(), BrowserLaunchFlags::Default) {
		tracing::warn!("couldn't open the browser to sign in");
	}
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
	timeline_list: &crate::ui::timeline_list::TimelineList,
	suppress_selection: &Cell<bool>,
	should_announce: bool,
	new_account_id: Option<String>,
) {
	if let Some(new_id) = new_account_id {
		crate::read_position::sync(state);
		state.synced_markers.clear();
		state.pending_message_requests = None;
		if let Some(old_id) = state.config.active_account_id.clone() {
			for timeline in state.timeline_manager.iter_mut() {
				timeline.stream_handle = None;
				timeline.stream_connected = false;
			}
			state.account_timelines.insert(old_id.clone(), std::mem::take(&mut state.timeline_manager));
			state.account_cw_expanded.insert(old_id, std::mem::take(&mut state.cw_expanded));
		}
		state.config.active_account_id = Some(new_id);
		let _ = state.save_config();
	}
	state.session = None;
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
		tracing::warn!(account = active_id, "no usable account to switch to");
		return;
	};
	tracing::info!(account = active_id, instance = url.host_str(), "switching to account");
	state.streaming_url = Some(url.clone());
	state.access_token = Some(token.clone());
	// The previous account's limits don't apply here. Until the instance answers, these are what it said last time.
	let limits = state.active_account().and_then(|account| account.instance_limits.clone());
	state.max_post_chars = limits.as_ref().map(|l| l.max_post_chars);
	state.max_media_description_chars = limits.as_ref().map(|l| l.max_media_description_chars);
	state.poll_limits = limits.as_ref().map_or_else(PollLimits::default, |l| l.poll_limits.clone());
	state.instance_languages = limits.map(|l| l.languages).unwrap_or_default();
	if let Ok(client) = MastodonClient::new(url) {
		state.client = Some(client.clone());
		state.session = Some(Session::new(client.clone(), token.clone()));
		if let Some(requests) = state.requests() {
			fetch_instance_info(&requests);
		}
		let needs_verify = state.active_account().and_then(|a| a.acct.as_deref()).is_none()
			|| state.active_account().and_then(|a| a.display_name.as_deref()).is_none()
			|| state.active_account().and_then(|a| a.user_id.as_deref()).is_none()
			|| state
				.active_account()
				.is_some_and(|a| a.default_post_visibility.is_none() || a.default_post_language.is_none());
		if needs_verify {
			if let Ok(account) = client.verify_credentials(&token)
				&& let Some(active) = state.active_account_mut()
			{
				active.acct = Some(account.acct);
				active.display_name = Some(account.display_name);
				active.user_id = Some(account.id.clone());
				let source = account.source.unwrap_or_default();
				active.default_post_visibility = source.privacy;
				active.default_post_language = Some(source.language.unwrap_or_default());
				state.current_user_id = Some(account.id);
				let _ = state.save_config();
			}
		} else if let Some(active) = state.active_account() {
			state.current_user_id = active.user_id.clone();
		}
	}
	// Only restore the saved timelines the first time an account is shown this
	// session. Switching back to it later keeps whatever was left open.
	let first_load = state.timeline_manager.len() == 0;
	let (saved_timelines, saved_active_timeline, saved_positions) = state
		.active_account()
		.map(|a| (a.saved_timelines.clone(), a.saved_active_timeline.clone(), a.saved_positions.clone()))
		.unwrap_or_default();
	if first_load {
		// Asked for before any timeline, so the markers arrive first and restoring can use them.
		if let Some(requests) = state.requests() {
			if state.config.sync_read_position {
				fetch_markers(&requests);
			}
			fetch_notification_policy(&requests);
		}
		let mut loaded_saved = false;
		let default_timelines = state.config.default_timelines.clone();
		if !saved_timelines.is_empty() {
			for t in saved_timelines {
				if !state.config.restore_open_timelines {
					let is_default = if t == TimelineType::Home || t == TimelineType::Notifications {
						true
					} else {
						let mut found = false;
						for dt in &default_timelines {
							let dt_type = match dt {
								crate::config::DefaultTimeline::Local => TimelineType::Local,
								crate::config::DefaultTimeline::Federated => TimelineType::Federated,
								crate::config::DefaultTimeline::Direct => TimelineType::Direct,
								crate::config::DefaultTimeline::Bookmarks => TimelineType::Bookmarks,
								crate::config::DefaultTimeline::Favorites => TimelineType::Favorites,
								crate::config::DefaultTimeline::Mentions => TimelineType::Mentions,
								crate::config::DefaultTimeline::Sent => TimelineType::User {
									id: state.current_user_id.clone().unwrap_or_default(),
									name: "Sent".to_string(),
								},
							};
							if t == dt_type {
								found = true;
								break;
							}
						}
						found
					};
					if !is_default {
						continue;
					}
				}
				state.timeline_manager.open(t.clone());
				match t {
					TimelineType::Search { query, search_type } => {
						if let Some(requests) = state.requests() {
							search(&requests, query, search_type, Some(u32::from(state.config.fetch_limit)), None);
						}
					}
					_ => timelines::refresh(state, t),
				}
			}
			loaded_saved = state.config.restore_open_timelines;
		}
		if !loaded_saved {
			let mut types_to_load = vec![TimelineType::Home, TimelineType::Notifications];
			for default in &default_timelines {
				let timeline_type = match default {
					crate::config::DefaultTimeline::Local => TimelineType::Local,
					crate::config::DefaultTimeline::Federated => TimelineType::Federated,
					crate::config::DefaultTimeline::Direct => TimelineType::Direct,
					crate::config::DefaultTimeline::Bookmarks => TimelineType::Bookmarks,
					crate::config::DefaultTimeline::Favorites => TimelineType::Favorites,
					crate::config::DefaultTimeline::Mentions => TimelineType::Mentions,
					crate::config::DefaultTimeline::Sent => {
						let Some(id) = state.current_user_id.clone() else { continue };
						TimelineType::User { id, name: "Sent".to_string() }
					}
				};
				types_to_load.push(timeline_type);
			}
			for t in types_to_load {
				if state.timeline_manager.open(t.clone()) {
					timelines::refresh(state, t);
				}
			}
		}
	}
	apply_streaming_setting(state);
	if first_load {
		if let Some(index) = saved_active_timeline.and_then(|saved| state.timeline_manager.index_of(&saved)) {
			state.timeline_manager.set_active(index);
		}
		// The timelines' first pages are only requested so far, so each finds its place when its page arrives.
		for position in saved_positions.into_iter().filter(|p| p.timeline.keeps_position()) {
			state.timeline_manager.restore(&position.timeline, position.post_id);
		}
	}
	timelines_selector.clear();
	for name in state.timeline_manager.display_names() {
		timelines_selector.append(&name);
	}
	let active_index = state.timeline_manager.active_index();
	timelines_selector.set_selection(u32::try_from(active_index).unwrap(), true);
	with_suppressed_selection(suppress_selection, || {
		timeline_list.clear();
		state.redraw_active_timeline(timeline_list, suppress_selection);
	});
	if should_announce {
		let handle = state.active_account().map_or_else(|| "Unknown".to_string(), Account::full_handle);
		timeline_list.announce(&format!("Switched to {handle}"));
	}
	update_window_title(state, frame);
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
	let streaming = state.config.streaming;
	let Some(timeline) = state.timeline_manager.get_mut(timeline_type) else { return };
	timeline.stream_connected = false;
	// With streaming off, the refresh timer polls every timeline that isn't connected.
	timeline.stream_handle = if streaming {
		streaming::start_streaming(&base_url, &access_token, timeline_type.clone(), state.ui_waker.clone())
	} else {
		None
	};
}

/// Starts or stops the open timelines' streams to match the streaming setting.
pub fn apply_streaming_setting(state: &mut AppState) {
	let timeline_types: Vec<TimelineType> =
		state.timeline_manager.iter_mut().map(|t| t.timeline_type.clone()).collect();
	for timeline_type in timeline_types {
		start_streaming_for_timeline(state, &timeline_type);
	}
}

/// Restarts the refresh timer at the configured interval.
pub fn apply_refresh_interval(state: &AppState) {
	if let Some(timer) = &state.refresh_timer {
		timer.start(i32::from(state.config.refresh_minutes) * 60_000, false);
	}
}
