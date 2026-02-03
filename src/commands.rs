use std::{
	cell::Cell,
	sync::mpsc,
	time::{Duration, Instant},
};

use url::Url;
use wxdragon::prelude::*;

use crate::{
	AppState,
	accounts::{start_add_account_flow, start_streaming_for_timeline, switch_to_account, try_oob_oauth},
	auth,
	config::{self, Account, AutoloadMode, ContentWarningDisplay, SortOrder},
	html, live_region,
	mastodon::{MastodonClient, Status},
	network::{self, NetworkCommand},
	timeline::{TimelineEntry, TimelineType},
	ui::{
		app_shell, dialogs,
		menu::update_menu_labels,
		timeline_view::{
			list_index_to_entry_index, sync_timeline_selection_from_list, update_active_timeline_ui,
			with_suppressed_selection,
		},
	},
};

/// Commands that can be triggered by UI events.
pub(crate) enum UiCommand {
	NewPost,
	Reply { reply_all: bool },
	DeletePost,
	EditPost,
	Favorite,
	Bookmark,
	Boost,
	Refresh,
	OpenTimeline(TimelineType),
	OpenUserTimeline,
	OpenUserTimelineByInput,
	CloseTimeline,
	TimelineSelectionChanged(usize),
	TimelineEntrySelectionChanged(usize),
	ShowOptions,
	ManageAccounts,
	SwitchAccount(String),
	SwitchNextAccount,
	SwitchPrevAccount,
	SwitchNextTimeline,
	SwitchPrevTimeline,
	RemoveAccount(String),
	ViewProfile,
	ViewMentions,
	ViewHashtags,
	HashtagDialogClosed,
	ProfileDialogClosed,
	OpenLinks,
	ViewInBrowser,
	ViewThread,
	Vote,
	LoadMore,
	ToggleContentWarning,
	ToggleWindowVisibility,
	SetQuickActionKeysEnabled(bool),
	GoBack,
	SwitchTimelineByIndex(usize),
	OAuthResult { result: Result<auth::OAuthResult, String>, instance_url: Url },
	CancelAuth,
	CloseAndNavigateBack,
	EditProfile,
	ViewHelp,
}

/// Refreshes the current timeline by re-fetching from the network.
pub(crate) fn refresh_timeline(state: &AppState, live_region: &StaticText) {
	let timeline_type = match state.timeline_manager.active() {
		Some(t) => t.timeline_type.clone(),
		None => return,
	};
	match &state.network_handle {
		Some(handle) => {
			handle.send(NetworkCommand::FetchTimeline { timeline_type, limit: Some(40), max_id: None });
		}
		None => {
			live_region::announce(live_region, "Network not available");
		}
	}
}

/// Handles a UI command, updating state and UI as needed.
#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_ui_command(
	cmd: UiCommand,
	state: &mut AppState,
	frame: &Frame,
	timelines_selector: &ListBox,
	timeline_list: &ListBox,
	suppress_selection: &Cell<bool>,
	live_region: &StaticText,
	quick_action_keys_enabled: &Cell<bool>,
	autoload_mode: &Cell<AutoloadMode>,
	sort_order_cell: &Cell<SortOrder>,
	tray_hidden: &Cell<bool>,
	ui_tx: &mpsc::Sender<UiCommand>,
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
			let reply = match dialogs::prompt_for_reply(
				frame,
				target,
				max_post_chars,
				&state.poll_limits,
				reply_all,
				self_acct,
				enter_to_send,
			) {
				Some(r) => r,
				None => return,
			};
			if let Some(handle) = &state.network_handle {
				handle.send(NetworkCommand::Reply {
					in_reply_to_id: target.id.clone(),
					content: reply.content,
					visibility: reply.visibility.as_api_str().to_string(),
					spoiler_text: reply.spoiler_text,
					content_type: reply.content_type,
					media: reply
						.media
						.into_iter()
						.map(|item| network::MediaUpload { path: item.path, description: item.description })
						.collect(),
					poll: reply.poll.map(|poll| network::PollData {
						options: poll.options,
						expires_in: poll.expires_in,
						multiple: poll.multiple,
					}),
				});
			} else {
				live_region::announce(live_region, "Network not available");
			}
		}
		UiCommand::DeletePost => {
			let status = match get_selected_status(state) {
				Some(s) => s,
				None => {
					live_region::announce(live_region, "No post selected");
					return;
				}
			};
			let target = status.reblog.as_ref().map(|r| r.as_ref()).unwrap_or(status);
			if let Some(current_user) = &state.current_user_id {
				if &target.account.id != current_user {
					live_region::announce(live_region, "You can only delete your own posts");
					return;
				}
			} else {
				live_region::announce(live_region, "Cannot verify ownership");
				return;
			}

			let confirm = MessageDialog::builder(frame, "Are you sure you want to delete this post?", "Delete Post")
				.with_style(MessageDialogStyle::YesNo | MessageDialogStyle::IconWarning)
				.build();
			if confirm.show_modal() == ID_YES {
				if let Some(handle) = &state.network_handle {
					handle.send(NetworkCommand::DeleteStatus { status_id: target.id.clone() });
				} else {
					live_region::announce(live_region, "Network not available");
				}
			}
		}
		UiCommand::EditPost => {
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
			if let Some(current_user) = &state.current_user_id {
				if &target.account.id != current_user {
					live_region::announce(live_region, "You can only edit your own posts");
					return;
				}
			} else {
				live_region::announce(live_region, "Cannot verify ownership");
				return;
			}

			let edit = match dialogs::prompt_for_edit(frame, target, max_post_chars, &state.poll_limits, enter_to_send)
			{
				Some(r) => r,
				None => return,
			};

			if let Some(handle) = &state.network_handle {
				let media = edit
					.media
					.into_iter()
					.map(|item| {
						if item.is_existing {
							network::EditMedia::Existing(item.path)
						} else {
							network::EditMedia::New(network::MediaUpload {
								path: item.path,
								description: item.description,
							})
						}
					})
					.collect();

				handle.send(NetworkCommand::EditStatus {
					status_id: target.id.clone(),
					content: edit.content,
					spoiler_text: edit.spoiler_text,
					media,
					poll: edit.poll.map(|poll| network::PollData {
						options: poll.options,
						expires_in: poll.expires_in,
						multiple: poll.multiple,
					}),
				});
			} else {
				live_region::announce(live_region, "Network not available");
			}
		}
		UiCommand::Favorite => {
			do_favorite(state, live_region);
		}
		UiCommand::Bookmark => {
			do_bookmark(state, live_region);
		}
		UiCommand::Boost => {
			do_boost(state, live_region);
		}
		UiCommand::Refresh => {
			refresh_timeline(state, live_region);
		}
		UiCommand::OpenTimeline(timeline_type) => {
			open_timeline(
				state,
				timelines_selector,
				timeline_list,
				timeline_type,
				suppress_selection,
				live_region,
				frame,
			);
		}
		UiCommand::CloseTimeline => {
			close_timeline(state, timelines_selector, timeline_list, suppress_selection, live_region, false);
		}
		UiCommand::CloseAndNavigateBack => {
			close_timeline(state, timelines_selector, timeline_list, suppress_selection, live_region, true);
		}
		UiCommand::LoadMore => {
			if let Some(active) = state.timeline_manager.active_mut()
				&& !active.entries.is_empty()
				&& !active.loading_more
				&& active.timeline_type.supports_paging()
			{
				let now = Instant::now();
				let can_load = match active.last_load_attempt {
					Some(last) => now.duration_since(last) > Duration::from_secs(1),
					None => true,
				};

				if can_load && let Some(last) = active.entries.last() {
					let max_id = last.id().to_string();
					active.loading_more = true;
					active.last_load_attempt = Some(now);
					if let Some(handle) = &state.network_handle {
						handle.send(NetworkCommand::FetchTimeline {
							timeline_type: active.timeline_type.clone(),
							limit: Some(state.config.fetch_limit as u32),
							max_id: Some(max_id),
						});
					}
				}
			}
		}
		UiCommand::ToggleContentWarning => {
			if state.config.content_warning_display != ContentWarningDisplay::WarningOnly {
				return;
			}
			let active = match state.timeline_manager.active_mut() {
				Some(t) => t,
				None => return,
			};
			let list_index = match active.selected_index {
				Some(index) => index,
				None => {
					live_region::announce(live_region, "No post selected");
					return;
				}
			};
			let entry_index = match list_index_to_entry_index(list_index, active.entries.len(), state.config.sort_order)
			{
				Some(index) => index,
				None => return,
			};
			let entry = match active.entries.get(entry_index) {
				Some(entry) => entry,
				None => return,
			};
			let status = match entry.as_status() {
				Some(status) => status,
				None => {
					live_region::announce(live_region, "No post selected");
					return;
				}
			};
			let target = status.reblog.as_ref().map(|r| r.as_ref()).unwrap_or(status);
			if target.spoiler_text.trim().is_empty() {
				live_region::announce(live_region, "No content warning");
				return;
			}
			let entry_id = entry.id();
			let expanded = state.cw_expanded.contains(entry_id);
			if expanded {
				state.cw_expanded.remove(entry_id);
			} else {
				state.cw_expanded.insert(entry_id.to_string());
			}
			let is_expanded = state.cw_expanded.contains(entry_id);
			let text =
				entry.display_text(state.config.timestamp_format, state.config.content_warning_display, is_expanded);
			timeline_list.set_string(list_index as u32, &text);
		}
		UiCommand::ToggleWindowVisibility => {
			app_shell::toggle_window_visibility(frame, tray_hidden);
		}
		UiCommand::SetQuickActionKeysEnabled(enabled) => {
			state.config.quick_action_keys = enabled;
			quick_action_keys_enabled.set(enabled);
			let _ = config::ConfigStore::new().save(&state.config);
			let msg = if enabled { "Quick keys enabled" } else { "Quick keys disabled" };
			live_region::announce(live_region, msg);
		}
		UiCommand::GoBack => {
			if let Some(active) = state.timeline_manager.active_mut() {
				sync_timeline_selection_from_list(active, timeline_list, state.config.sort_order);
			}

			if state.timeline_manager.go_back() {
				let index = state.timeline_manager.active_index();
				with_suppressed_selection(suppress_selection, || {
					timelines_selector.set_selection(index as u32, true);
				});

				if let Some(active) = state.timeline_manager.active_mut() {
					update_active_timeline_ui(
						timeline_list,
						active,
						suppress_selection,
						state.config.sort_order,
						state.config.timestamp_format,
						state.config.content_warning_display,
						&state.cw_expanded,
					);
				}
				if let Some(mb) = frame.get_menu_bar() {
					update_menu_labels(&mb, state);
				}
			} else {
				live_region::announce(live_region, "No previous timeline");
			}
		}
		UiCommand::SwitchTimelineByIndex(index) => {
			if index < state.timeline_manager.len() {
				handle_ui_command(
					UiCommand::TimelineSelectionChanged(index),
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
			} else {
				live_region::announce(live_region, "No timeline at this position");
			}
		}
		UiCommand::TimelineSelectionChanged(index) => {
			if index < state.timeline_manager.len() {
				if let Some(active) = state.timeline_manager.active_mut() {
					sync_timeline_selection_from_list(active, timeline_list, state.config.sort_order);
				}
				state.timeline_manager.set_active(index);
				let current_selection = timelines_selector.get_selection().map(|s| s as usize);
				if current_selection != Some(index) {
					with_suppressed_selection(suppress_selection, || {
						timelines_selector.set_selection(index as u32, true);
					});
				}
				if let Some(active) = state.timeline_manager.active_mut() {
					update_active_timeline_ui(
						timeline_list,
						active,
						suppress_selection,
						state.config.sort_order,
						state.config.timestamp_format,
						state.config.content_warning_display,
						&state.cw_expanded,
					);
				}
				if let Some(mb) = frame.get_menu_bar() {
					update_menu_labels(&mb, state);
				}
			}
		}
		UiCommand::TimelineEntrySelectionChanged(index) => {
			if let Some(active) = state.timeline_manager.active_mut() {
				active.selected_index = Some(index);
				active.selected_id = list_index_to_entry_index(index, active.entries.len(), state.config.sort_order)
					.map(|entry_index| active.entries[entry_index].id().to_string());
			}
			if let Some(mb) = frame.get_menu_bar() {
				update_menu_labels(&mb, state);
			}
		}
		UiCommand::ShowOptions => {
			if let Some((
				enter_to_send,
				always_show_link_dialog,
				quick_action_keys,
				autoload,
				fetch_limit,
				content_warning_display,
				sort_order,
				timestamp_format,
			)) = dialogs::prompt_for_options(
				frame,
				state.config.enter_to_send,
				state.config.always_show_link_dialog,
				state.config.quick_action_keys,
				state.config.autoload,
				state.config.fetch_limit,
				state.config.content_warning_display,
				state.config.sort_order,
				state.config.timestamp_format,
			) {
				let needs_refresh = state.config.sort_order != sort_order
					|| state.config.timestamp_format != timestamp_format
					|| state.config.content_warning_display != content_warning_display;
				state.config.enter_to_send = enter_to_send;
				state.config.always_show_link_dialog = always_show_link_dialog;
				state.config.quick_action_keys = quick_action_keys;
				state.config.autoload = autoload;
				state.config.fetch_limit = fetch_limit;
				state.config.content_warning_display = content_warning_display;
				if state.config.content_warning_display != ContentWarningDisplay::WarningOnly {
					state.cw_expanded.clear();
				}
				quick_action_keys_enabled.set(quick_action_keys);
				autoload_mode.set(autoload);
				sort_order_cell.set(sort_order);
				if let Some(mb) = frame.get_menu_bar() {
					update_menu_labels(&mb, state);
				}
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
						state.config.content_warning_display,
						&state.cw_expanded,
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
					let _ = start_add_account_flow(frame, ui_tx, state);
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
						quick_action_keys_enabled,
						autoload_mode,
						sort_order_cell,
						tray_hidden,
						ui_tx,
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
						quick_action_keys_enabled,
						autoload_mode,
						sort_order_cell,
						tray_hidden,
						ui_tx,
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
				quick_action_keys_enabled,
				autoload_mode,
				sort_order_cell,
				tray_hidden,
				ui_tx,
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
				quick_action_keys_enabled,
				autoload_mode,
				sort_order_cell,
				tray_hidden,
				ui_tx,
			);
		}
		UiCommand::SwitchNextTimeline => {
			if state.timeline_manager.len() <= 1 {
				return;
			}
			let current = state.timeline_manager.active_index();
			let next = (current + 1) % state.timeline_manager.len();
			if let Some(name) = state.timeline_manager.display_names().get(next) {
				live_region::announce(live_region, name);
			}
			handle_ui_command(
				UiCommand::TimelineSelectionChanged(next),
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
		UiCommand::SwitchPrevTimeline => {
			if state.timeline_manager.len() <= 1 {
				return;
			}
			let current = state.timeline_manager.active_index();
			let prev = (current + state.timeline_manager.len() - 1) % state.timeline_manager.len();
			if let Some(name) = state.timeline_manager.display_names().get(prev) {
				live_region::announce(live_region, name);
			}
			handle_ui_command(
				UiCommand::TimelineSelectionChanged(prev),
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
		UiCommand::RemoveAccount(id) => {
			let is_active = state.config.active_account_id.as_ref() == Some(&id);
			state.config.accounts.retain(|a| a.id != id);
			if is_active {
				state.config.active_account_id = state.config.accounts.first().map(|a| a.id.clone());
			}
			let _ = config::ConfigStore::new().save(&state.config);
			if is_active {
				if state.config.accounts.is_empty() {
					if !start_add_account_flow(frame, ui_tx, state) {
						frame.close(true);
						return;
					}
					// If flow started, we return and wait for OAuthResult
					return;
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
		UiCommand::OAuthResult { result, instance_url } => {
			if let Some(dialog) = state.pending_auth_dialog.take() {
				dialog.destroy();
			}
			// frame.enable(true) is not needed as we don't disable it anymore
			frame.raise();

			let mut account = Account::new(instance_url.to_string());
			let client = match MastodonClient::new(instance_url.clone()) {
				Ok(c) => c,
				Err(e) => {
					dialogs::show_error(frame, &anyhow::anyhow!(e));
					if state.config.accounts.is_empty() {
						frame.close(true);
					}
					return;
				}
			};

			let success = match result {
				Ok(res) => {
					account.access_token = Some(res.access_token);
					account.client_id = Some(res.client_id);
					account.client_secret = Some(res.client_secret);
					true
				}
				Err(_) => {
					// Fallback to OOB
					if let Some(acc) = try_oob_oauth(frame, &client, &instance_url, &mut account) {
						account = acc;
						true
					} else {
						// Fallback to Manual
						if let Some(token) = dialogs::prompt_for_access_token(frame, &instance_url) {
							account.access_token = Some(token);
							true
						} else {
							false
						}
					}
				}
			};

			if success {
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
					quick_action_keys_enabled,
					autoload_mode,
					sort_order_cell,
					tray_hidden,
					ui_tx,
				);
			} else if state.config.accounts.is_empty() {
				frame.close(true);
			}
		}
		UiCommand::CancelAuth => {
			if let Some(dialog) = state.pending_auth_dialog.take() {
				dialog.destroy();
			}
			if state.config.accounts.is_empty() {
				frame.close(true);
			}
		}
		UiCommand::ViewProfile => {
			let entry = match get_selected_entry(state) {
				Some(e) => e,
				None => {
					live_region::announce(live_region, "No item selected");
					return;
				}
			};
			let (account, action) = match entry {
				TimelineEntry::Status(status) => {
					if let Some(reblog) = &status.reblog {
						let booster = &status.account;
						let author = &reblog.account;
						let accounts = [booster, author];
						let labels = [
							format!("{} (booster)", booster.display_name_or_username()),
							format!("{} (author)", author.display_name_or_username()),
						];
						let label_refs: Vec<&str> = labels.iter().map(|s| s.as_str()).collect();
						match dialogs::prompt_for_account_selection(frame, &accounts, &label_refs) {
							Some((acc, act)) => (acc, act),
							None => return,
						}
					} else {
						(status.account.clone(), dialogs::UserLookupAction::Profile)
					}
				}
				TimelineEntry::Notification(notification) => {
					(notification.account.clone(), dialogs::UserLookupAction::Profile)
				}
			};
			match action {
				dialogs::UserLookupAction::Profile => {
					if let Some(net) = &state.network_handle {
						net.send(NetworkCommand::FetchRelationship { account_id: account.id.clone() });
						net.send(NetworkCommand::FetchAccount { account_id: account.id.clone() });
						let net_tx = net.command_tx.clone();
						let ui_tx_timeline = ui_tx.clone();
						let timeline_type = TimelineType::User {
							id: account.id.clone(),
							name: account.display_name_or_username().to_string(),
						};
						let ui_tx_close = ui_tx.clone();
						let dlg = dialogs::ProfileDialog::new(
							frame,
							account.clone(),
							net_tx,
							move || {
								let _ = ui_tx_timeline.send(UiCommand::OpenTimeline(timeline_type.clone()));
							},
							move || {
								let _ = ui_tx_close.send(UiCommand::ProfileDialogClosed);
							},
						);
						dlg.show();
						state.profile_dialog = Some(dlg);
					} else {
						live_region::announce(live_region, "Network not available");
					}
				}
				dialogs::UserLookupAction::Timeline => {
					let timeline_type = TimelineType::User {
						id: account.id.clone(),
						name: account.display_name_or_username().to_string(),
					};
					open_timeline(
						state,
						timelines_selector,
						timeline_list,
						timeline_type,
						suppress_selection,
						live_region,
						frame,
					);
				}
			}
		}
		UiCommand::OpenUserTimeline => {
			let entry = match get_selected_entry(state) {
				Some(e) => e,
				None => {
					live_region::announce(live_region, "No item selected");
					return;
				}
			};
			let (account, action) = match entry {
				TimelineEntry::Status(status) => {
					if let Some(reblog) = &status.reblog {
						let booster = &status.account;
						let author = &reblog.account;
						let accounts = [booster, author];
						let labels = [
							format!("{} (booster)", booster.display_name_or_username()),
							format!("{} (author)", author.display_name_or_username()),
						];
						let label_refs: Vec<&str> = labels.iter().map(|s| s.as_str()).collect();
						match dialogs::prompt_for_account_selection(frame, &accounts, &label_refs) {
							Some((acc, act)) => (acc, act),
							None => return,
						}
					} else {
						(status.account.clone(), dialogs::UserLookupAction::Timeline)
					}
				}
				TimelineEntry::Notification(notification) => {
					(notification.account.clone(), dialogs::UserLookupAction::Timeline)
				}
			};
			match action {
				dialogs::UserLookupAction::Profile => {
					if let Some(net) = &state.network_handle {
						net.send(NetworkCommand::FetchRelationship { account_id: account.id.clone() });
						net.send(NetworkCommand::FetchAccount { account_id: account.id.clone() });
						let net_tx = net.command_tx.clone();
						let ui_tx_timeline = ui_tx.clone();
						let timeline_type = TimelineType::User {
							id: account.id.clone(),
							name: account.display_name_or_username().to_string(),
						};
						let ui_tx_close = ui_tx.clone();
						let dlg = dialogs::ProfileDialog::new(
							frame,
							account.clone(),
							net_tx,
							move || {
								let _ = ui_tx_timeline.send(UiCommand::OpenTimeline(timeline_type.clone()));
							},
							move || {
								let _ = ui_tx_close.send(UiCommand::ProfileDialogClosed);
							},
						);
						dlg.show();
						state.profile_dialog = Some(dlg);
					} else {
						live_region::announce(live_region, "Network not available");
					}
				}
				dialogs::UserLookupAction::Timeline => {
					let timeline_type = TimelineType::User {
						id: account.id.clone(),
						name: account.display_name_or_username().to_string(),
					};
					open_timeline(
						state,
						timelines_selector,
						timeline_list,
						timeline_type,
						suppress_selection,
						live_region,
						frame,
					);
				}
			}
		}
		UiCommand::OpenUserTimelineByInput => {
			if let Some((input, action)) = dialogs::prompt_for_user_lookup(frame) {
				let handle: String = input.chars().filter(|c| !c.is_whitespace()).collect();
				if let Some(network) = &state.network_handle {
					state.pending_user_lookup_action = Some(action);
					network.send(NetworkCommand::LookupAccount { handle });
				} else {
					live_region::announce(live_region, "Network not available");
				}
			}
		}
		UiCommand::ViewMentions => {
			let status = match get_selected_status(state) {
				Some(s) => s,
				None => {
					live_region::announce(live_region, "No post selected");
					return;
				}
			};
			let target = status.reblog.as_ref().map(|r| r.as_ref()).unwrap_or(status);
			if target.mentions.is_empty() {
				live_region::announce(live_region, "No mentions in this post");
				return;
			}
			if let Some((mention, action)) = dialogs::prompt_for_mentions(frame, &target.mentions) {
				let mut account = None;
				if let (Some(client), Some(token)) = (&state.client, &state.access_token) {
					match client.get_account(token, &mention.id) {
						Ok(full) => account = Some(full),
						Err(err) => {
							dialogs::show_error(frame, &err);
						}
					}
				}
				let account = account.unwrap_or_else(|| crate::mastodon::Account {
					id: mention.id.clone(),
					username: mention.username.clone(),
					acct: mention.acct.clone(),
					display_name: String::new(),
					url: mention.url,
					note: String::new(),
					followers_count: 0,
					following_count: 0,
					statuses_count: 0,
					fields: Vec::new(),
					created_at: String::new(),
					locked: false,
					bot: false,
					discoverable: None,
					source: None,
				});

				match action {
					dialogs::UserLookupAction::Profile => {
						if let Some(net) = &state.network_handle {
							net.send(NetworkCommand::FetchRelationship { account_id: account.id.clone() });
							let net_tx = net.command_tx.clone();
							let ui_tx_timeline = ui_tx.clone();
							let timeline_type = TimelineType::User {
								id: account.id.clone(),
								name: account.display_name_or_username().to_string(),
							};
							let ui_tx_close = ui_tx.clone();

							let dlg = dialogs::ProfileDialog::new(
								frame,
								account.clone(),
								net_tx,
								move || {
									let _ = ui_tx_timeline.send(UiCommand::OpenTimeline(timeline_type.clone()));
								},
								move || {
									let _ = ui_tx_close.send(UiCommand::ProfileDialogClosed);
								},
							);
							dlg.show();
							state.profile_dialog = Some(dlg);
						} else {
							live_region::announce(live_region, "Network not available");
						}
					}
					dialogs::UserLookupAction::Timeline => {
						let timeline_type = TimelineType::User {
							id: account.id.clone(),
							name: account.display_name_or_username().to_string(),
						};
						handle_ui_command(
							UiCommand::OpenTimeline(timeline_type),
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
			}
		}
		UiCommand::ViewHashtags => {
			let status = match get_selected_status(state) {
				Some(s) => s,
				None => {
					live_region::announce(live_region, "No post selected");
					return;
				}
			};
			let target = status.reblog.as_ref().map(|r| r.as_ref()).unwrap_or(status);
			if target.tags.is_empty() {
				live_region::announce(live_region, "No hashtags in this post");
				return;
			}
			let names: Vec<String> = target.tags.iter().map(|t| t.name.clone()).collect();
			if let Some(handle) = &state.network_handle {
				handle.send(NetworkCommand::FetchTagsInfo { names });
			} else {
				live_region::announce(live_region, "Network not available");
			}
		}
		UiCommand::HashtagDialogClosed => {
			state.hashtag_dialog = None;
		}
		UiCommand::ProfileDialogClosed => {
			state.profile_dialog = None;
		}
		UiCommand::OpenLinks => {
			let status = match get_selected_status(state) {
				Some(s) => s,
				None => return,
			};
			let target = status.reblog.as_ref().map(|r| r.as_ref()).unwrap_or(status);
			let links = html::extract_links(&target.content);
			if links.is_empty() {
				live_region::announce(live_region, "No links in this post");
				return;
			}
			let url_to_open = if links.len() == 1 && !state.config.always_show_link_dialog {
				Some(links[0].url.clone())
			} else {
				dialogs::prompt_for_link_selection(frame, &links)
			};
			if let Some(url) = url_to_open {
				live_region::announce(live_region, "Opening link");
				let _ = launch_default_browser(&url, BrowserLaunchFlags::Default);
			}
		}
		UiCommand::ViewInBrowser => {
			let status = match get_selected_status(state) {
				Some(s) => s,
				None => {
					live_region::announce(live_region, "No post selected");
					return;
				}
			};
			let target = status.reblog.as_ref().map(|r| r.as_ref()).unwrap_or(status);
			if let Some(url) = &target.url {
				live_region::announce(live_region, "Opening post in browser");
				let _ = launch_default_browser(url, BrowserLaunchFlags::Default);
			} else {
				live_region::announce(live_region, "Post URL not available");
			}
		}
		UiCommand::ViewThread => {
			let target = {
				let status = match get_selected_status(state) {
					Some(s) => s,
					None => {
						live_region::announce(live_region, "No post selected");
						return;
					}
				};
				let target = status.reblog.as_ref().map(|r| r.as_ref()).unwrap_or(status);
				target.clone()
			};
			let name = format!("Thread: {}", target.account.display_name_or_username());
			let timeline_type = TimelineType::Thread { id: target.id.clone(), name };
			open_timeline(
				state,
				timelines_selector,
				timeline_list,
				timeline_type.clone(),
				suppress_selection,
				live_region,
				frame,
			);
			let handle = match &state.network_handle {
				Some(h) => h,
				None => {
					live_region::announce(live_region, "Network not available");
					return;
				}
			};
			handle.send(NetworkCommand::FetchThread { timeline_type, focus: target });
		}
		UiCommand::Vote => {
			let status = match get_selected_status(state) {
				Some(s) => s,
				None => {
					live_region::announce(live_region, "No post selected");
					return;
				}
			};
			let target = status.reblog.as_ref().map(|r| r.as_ref()).unwrap_or(status);
			let poll = match &target.poll {
				Some(p) => p,
				None => {
					live_region::announce(live_region, "No poll in this post");
					return;
				}
			};
			let post_text = target.display_text();
			if let Some(choices) = dialogs::prompt_for_vote(frame, poll, &post_text) {
				if let Some(handle) = &state.network_handle {
					handle.send(NetworkCommand::VotePoll { poll_id: poll.id.clone(), choices });
				} else {
					live_region::announce(live_region, "Network not available");
				}
			}
		}
		UiCommand::EditProfile => {
			if let Some(handle) = &state.network_handle {
				handle.send(NetworkCommand::FetchCredentials);
			} else {
				live_region::announce(live_region, "Network not available");
			}
		}
		UiCommand::ViewHelp => {
			if let Ok(mut path) = std::env::current_exe() {
				path.pop();
				path.push("readme.html");
				if path.exists() {
					live_region::announce(live_region, "Opening help");
					let _ = wxdragon::utils::launch_default_browser(
						&path.to_string_lossy(),
						wxdragon::utils::BrowserLaunchFlags::Default,
					);
				} else {
					live_region::announce(live_region, "Help file not found");
					dialogs::show_error(
						frame,
						&anyhow::anyhow!("Help file (readme.html) not found in application directory."),
					);
				}
			} else {
				live_region::announce(live_region, "Could not determine help path");
			}
		}
	}
}

/// Gets the currently selected timeline entry.
pub(crate) fn get_selected_entry(state: &AppState) -> Option<&TimelineEntry> {
	let timeline = state.timeline_manager.active()?;
	let index = timeline.selected_index?;

	let final_index = match state.config.sort_order {
		SortOrder::NewestToOldest => index,
		SortOrder::OldestToNewest => timeline.entries.len().checked_sub(1)?.checked_sub(index)?,
	};

	timeline.entries.get(final_index)
}

/// Gets the currently selected status (unwrapping from notification if needed).
pub(crate) fn get_selected_status(state: &AppState) -> Option<&Status> {
	get_selected_entry(state)?.as_status()
}

/// Sends a favorite or unfavorite request for the selected status.
fn do_favorite(state: &AppState, live_region: &StaticText) {
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
		handle.send(NetworkCommand::Unfavorite { status_id });
	} else {
		handle.send(NetworkCommand::Favorite { status_id });
	}
}

fn do_bookmark(state: &AppState, live_region: &StaticText) {
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
	let target = status.reblog.as_ref().map(|r| r.as_ref()).unwrap_or(status);
	let status_id = target.id.clone();
	if target.bookmarked {
		handle.send(NetworkCommand::Unbookmark { status_id });
	} else {
		handle.send(NetworkCommand::Bookmark { status_id });
	}
}

/// Sends a boost or unboost request for the selected status.
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
	if target.visibility == "direct" {
		live_region::announce(live_region, "Cannot boost direct messages");
		return;
	}
	let status_id = target.id.clone();
	if target.reblogged {
		handle.send(NetworkCommand::Unboost { status_id });
	} else {
		handle.send(NetworkCommand::Boost { status_id });
	}
}

/// Opens a new timeline or switches to it if already open.
fn open_timeline(
	state: &mut AppState,
	selector: &ListBox,
	timeline_list: &ListBox,
	timeline_type: TimelineType,
	suppress_selection: &Cell<bool>,
	live_region: &StaticText,
	frame: &Frame,
) {
	if matches!(timeline_type, TimelineType::User { .. } | TimelineType::Thread { .. }) {
		state.timeline_manager.snapshot_active_to_history();
	}

	if !state.timeline_manager.open(timeline_type.clone()) {
		if let Some(index) = state.timeline_manager.index_of(&timeline_type) {
			state.timeline_manager.set_active(index);
			with_suppressed_selection(suppress_selection, || {
				selector.set_selection(index as u32, true);
			});
			if let Some(active) = state.timeline_manager.active_mut() {
				update_active_timeline_ui(
					timeline_list,
					active,
					suppress_selection,
					state.config.sort_order,
					state.config.timestamp_format,
					state.config.content_warning_display,
					&state.cw_expanded,
				);
			}
		}
		if let Some(mb) = frame.get_menu_bar() {
			update_menu_labels(&mb, state);
		}
		live_region::announce(live_region, "Timeline already open");
		return;
	}
	selector.append(&timeline_type.display_name());
	let new_index = state.timeline_manager.len() - 1;
	state.timeline_manager.set_active(new_index);
	with_suppressed_selection(suppress_selection, || {
		selector.set_selection(new_index as u32, true);
	});
	if !matches!(timeline_type, TimelineType::Thread { .. }) {
		if let Some(handle) = &state.network_handle {
			handle.send(NetworkCommand::FetchTimeline {
				timeline_type: timeline_type.clone(),
				limit: Some(40),
				max_id: None,
			});
		}
		start_streaming_for_timeline(state, &timeline_type);
	}
	with_suppressed_selection(suppress_selection, || {
		timeline_list.clear();
	});
	if let Some(mb) = frame.get_menu_bar() {
		update_menu_labels(&mb, state);
	}
}

/// Closes the current timeline if it's closeable.
fn close_timeline(
	state: &mut AppState,
	selector: &ListBox,
	timeline_list: &ListBox,
	suppress_selection: &Cell<bool>,
	live_region: &StaticText,
	use_history: bool,
) {
	let active_type = match state.timeline_manager.active() {
		Some(t) => t.timeline_type.clone(),
		None => return,
	};
	if !active_type.is_closeable() {
		live_region::announce(live_region, &format!("Cannot close the {} timeline", active_type.display_name()));
		return;
	}
	if !state.timeline_manager.close(&active_type, use_history) {
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
			state.config.content_warning_display,
			&state.cw_expanded,
		);
	}
}
