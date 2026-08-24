//! Commands that manage accounts and the sign-in flow.

use url::Url;
use wxdragon::prelude::*;

use super::{UiCommand, UiCommandContext, handle_ui_command};
use crate::{
	accounts::{start_add_account_flow, switch_to_account, try_oob_oauth},
	auth, config,
	config::Account,
	mastodon::MastodonClient,
	network::NetworkCommand,
	ui::dialogs,
};

pub(super) fn manage_accounts(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	let frame = ctx.frame;
	let ui_tx = ctx.ui_tx;
	let result = dialogs::show_manage_accounts_dialog(
		frame,
		&state.config.accounts,
		state.active_account().map(|a| a.id.as_str()),
	);
	match result {
		dialogs::ManageAccountsResult::Add => {
			let _ = start_add_account_flow(frame, ui_tx, state);
		}
		dialogs::ManageAccountsResult::Remove(id) => {
			handle_ui_command(UiCommand::RemoveAccount(id), ctx);
		}
		dialogs::ManageAccountsResult::Switch(id) => {
			handle_ui_command(UiCommand::SwitchAccount(id), ctx);
		}
		dialogs::ManageAccountsResult::None => {}
	}
}

pub(super) fn switch_account(ctx: &mut UiCommandContext<'_>, id: String) {
	let state = &mut *ctx.state;
	let frame = ctx.frame;
	let timelines_selector = ctx.timelines_selector;
	let timeline_list = &ctx.timeline_list;
	let suppress_selection = ctx.suppress_selection;
	if state.config.active_account_id.as_ref() == Some(&id) {
		return;
	}
	switch_to_account(state, frame, timelines_selector, timeline_list, suppress_selection, true, Some(id));
}

pub(super) fn switch_next_account(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
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
	handle_ui_command(UiCommand::SwitchAccount(next_id), ctx);
}

pub(super) fn switch_prev_account(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
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
	handle_ui_command(UiCommand::SwitchAccount(prev_id), ctx);
}

pub(super) fn remove_account(ctx: &mut UiCommandContext<'_>, id: String) {
	let state = &mut *ctx.state;
	let frame = ctx.frame;
	let timelines_selector = ctx.timelines_selector;
	let timeline_list = &ctx.timeline_list;
	let suppress_selection = ctx.suppress_selection;
	let ui_tx = ctx.ui_tx;
	let is_active = state.config.active_account_id.as_ref() == Some(&id);
	state.config.accounts.retain(|a| a.id != id);
	state.account_timelines.remove(&id);
	state.account_cw_expanded.remove(&id);

	if is_active {
		let next_id = state.config.accounts.first().map(|a| a.id.clone());
		if next_id.is_none() {
			if !start_add_account_flow(frame, ui_tx, state) {
				frame.close(true);
				return;
			}
			// If flow started, we return and wait for OAuthResult
			return;
		}
		switch_to_account(state, frame, timelines_selector, timeline_list, suppress_selection, true, next_id);
	} else {
		let _ = config::ConfigStore::new().save(&state.config);
	}
}

pub(super) fn oauth_result(
	ctx: &mut UiCommandContext<'_>,
	result: Result<auth::OAuthResult, String>,
	instance_url: Url,
) {
	let state = &mut *ctx.state;
	let frame = ctx.frame;
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
		handle_ui_command(UiCommand::SwitchAccount(id), ctx);
	} else if state.config.accounts.is_empty() {
		frame.close(true);
	}
}

pub(super) fn cancel_auth(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	let frame = ctx.frame;
	if let Some(dialog) = state.pending_auth_dialog.take() {
		dialog.destroy();
	}
	if state.config.accounts.is_empty() {
		frame.close(true);
	}
}

pub(super) fn edit_profile(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	let live_region = ctx.live_region;
	if let Some(handle) = &state.network_handle {
		handle.send(NetworkCommand::FetchCredentials);
	} else {
		live_region.announce("Network not available");
	}
}
