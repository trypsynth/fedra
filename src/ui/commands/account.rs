//! Commands that manage accounts and the sign-in flow.

use url::Url;
use wxdragon::prelude::*;

use super::{UiCommand, UiCommandContext, deferred, handle_ui_command};
use crate::{
	accounts::{continue_add_account_flow, switch_to_account, try_oob_oauth},
	auth,
	config::Account,
	mastodon::MastodonClient,
	network::NetworkCommand,
	ui::dialogs,
};

pub(super) fn manage_accounts(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	let accounts = state.config.accounts.clone();
	let active_id = state.active_account().map(|a| a.id.clone());
	deferred::dialog(
		ctx,
		move |frame| dialogs::show_manage_accounts_dialog(frame, &accounts, active_id.as_deref()),
		|ctx, result| match result {
			dialogs::ManageAccountsResult::Add => add_account(ctx, false),
			dialogs::ManageAccountsResult::Remove(id) => handle_ui_command(UiCommand::RemoveAccount(id), ctx),
			dialogs::ManageAccountsResult::Switch(id) => handle_ui_command(UiCommand::SwitchAccount(id), ctx),
			dialogs::ManageAccountsResult::None => {}
		},
	);
}

/// Asks which instance to sign in to, then starts signing in. Closing the app when there's
/// nothing to cancel back to is up to `close_if_cancelled`.
fn add_account(ctx: &mut UiCommandContext<'_>, close_if_cancelled: bool) {
	deferred::dialog(ctx, dialogs::prompt_for_instance, move |ctx, instance_url| match instance_url {
		Some(instance_url) => continue_add_account_flow(ctx.frame, ctx.ui_tx, ctx.state, instance_url),
		None if close_if_cancelled => {
			ctx.frame.close(true);
		}
		None => {}
	});
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
	let is_active = state.config.active_account_id.as_ref() == Some(&id);
	state.config.accounts.retain(|a| a.id != id);
	state.account_timelines.remove(&id);
	state.account_cw_expanded.remove(&id);
	if is_active {
		let next_id = state.config.accounts.first().map(|a| a.id.clone());
		if next_id.is_none() {
			// Waits for the OAuthResult once the instance has been chosen.
			add_account(ctx, true);
			return;
		}
		switch_to_account(state, frame, timelines_selector, timeline_list, suppress_selection, true, next_id);
	} else {
		let _ = state.save_config();
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
	frame.raise();
	let mut account = Account::new(instance_url.to_string());
	let client = match MastodonClient::new(instance_url.clone()) {
		Ok(c) => c,
		Err(e) => {
			deferred::dialog(
				ctx,
				move |frame| dialogs::show_error(frame, &anyhow::anyhow!(e)),
				|ctx, ()| {
					if ctx.state.config.accounts.is_empty() {
						ctx.frame.close(true);
					}
				},
			);
			return;
		}
	};
	match result {
		Ok(res) => {
			account.access_token = Some(res.access_token);
			account.client_id = Some(res.client_id);
			account.client_secret = Some(res.client_secret);
			finish_oauth(ctx, Some(account));
		}
		Err(_) => {
			// Fall back to pasting a code, then to pasting a token.
			deferred::dialog(
				ctx,
				move |frame| {
					if let Some(acc) = try_oob_oauth(frame, &client, &instance_url, &mut account) {
						return Some(acc);
					}
					dialogs::prompt_for_access_token(frame, &instance_url).map(|token| {
						account.access_token = Some(token);
						account
					})
				},
				finish_oauth,
			);
		}
	}
}

/// Adds the account that signing in produced, or gives up when it produced none.
fn finish_oauth(ctx: &mut UiCommandContext<'_>, account: Option<Account>) {
	if let Some(account) = account {
		let id = account.id.clone();
		ctx.state.config.accounts.push(account);
		let _ = ctx.state.save_config();
		handle_ui_command(UiCommand::SwitchAccount(id), ctx);
	} else if ctx.state.config.accounts.is_empty() {
		ctx.frame.close(true);
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
