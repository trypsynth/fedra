use std::slice;

use anyhow::Result;
use wxdragon::prelude::*;

use super::NetworkResponseContext;
use crate::{
	UiCommand,
	mastodon::{Account, List, MastodonClient},
	timeline::TimelineType,
	ui::{
		commands::{Requests, deferred},
		dialogs,
	},
};

/// Makes a list request, handing what it gives back to `done`, or its failure to
/// `operation_failed`.
fn request<T: Send + 'static>(
	requests: &Requests,
	request: impl FnOnce(&MastodonClient, &str) -> Result<T> + Send + 'static,
	done: impl FnOnce(&mut NetworkResponseContext<'_>, T) + 'static,
) {
	requests.request(request, move |ctx, result| match result {
		Ok(value) => done(ctx, value),
		Err(err) => operation_failed(ctx, &err),
	});
}

/// Fetches the account's lists, for whichever of adding a user, managing the lists or opening one
/// is waiting for them.
pub fn fetch(requests: &Requests) {
	requests.request(MastodonClient::get_lists, |ctx, result| match result {
		Ok(lists) => fetched(ctx, lists),
		Err(err) => ctx.announce_failure("Failed to fetch lists", &err),
	});
}

pub fn create(requests: &Requests, title: String, policy: String, exclusive: bool) {
	request(
		requests,
		move |client, token| client.create_list(token, &title, &policy, exclusive),
		|ctx, list| changed(ctx, &format!("List '{}' created", list.title)),
	);
}

pub fn update(requests: &Requests, id: String, title: String, policy: String, exclusive: bool) {
	request(
		requests,
		move |client, token| client.update_list(token, &id, &title, &policy, exclusive),
		|ctx, list| changed(ctx, &format!("List '{}' updated", list.title)),
	);
}

pub fn delete(requests: &Requests, id: String) {
	request(requests, move |client, token| client.delete_list(token, &id), |ctx, ()| changed(ctx, "List deleted"));
}

/// Fetches a list's members, for the open members dialog or a new one.
pub fn fetch_members(requests: &Requests, list_id: String) {
	let id = list_id.clone();
	request(
		requests,
		move |client, token| client.get_list_accounts(token, &id),
		move |ctx, members| accounts_fetched(ctx, list_id, members),
	);
}

/// Fetches the lists a user is in, to pick one to remove them from.
pub fn fetch_account_lists(requests: &Requests, account_id: String) {
	let id = account_id.clone();
	request(
		requests,
		move |client, token| client.get_account_lists(token, &id),
		move |ctx, lists| account_lists_fetched(ctx, account_id, &lists),
	);
}

pub fn add_member(requests: &Requests, list_id: String, account_id: String) {
	request(
		requests,
		move |client, token| client.add_list_accounts(token, &list_id, slice::from_ref(&account_id)),
		|ctx, ()| membership_changed(ctx, "Member added"),
	);
}

pub fn remove_member(requests: &Requests, list_id: String, account_id: String) {
	request(
		requests,
		move |client, token| client.remove_list_accounts(token, &list_id, slice::from_ref(&account_id)),
		|ctx, ()| membership_changed(ctx, "Member removed"),
	);
}

fn fetched(ctx: &mut NetworkResponseContext<'_>, lists: Vec<List>) {
	if let Some(account_id) = ctx.state.pending_add_to_list_user.take() {
		if lists.is_empty() {
			ctx.announce("No lists found to add user to");
		} else {
			deferred::dialog_some(
				ctx,
				move |frame| dialogs::show_list_selection_dialog(frame, &lists, "Add to List", "Add"),
				move |ctx, list| {
					if let Some(requests) = ctx.requests() {
						add_member(&requests, list.id, account_id);
					}
				},
			);
		}
		return;
	}
	if let Some(dlg) = &ctx.state.manage_lists_dialog {
		dlg.update_lists(lists);
		return;
	}
	if lists.is_empty() {
		ctx.announce("No lists found");
		return;
	}
	deferred::dialog_some(
		ctx,
		move |frame| dialogs::show_list_selection_dialog(frame, &lists, "Open List", "Open"),
		|ctx, list| ctx.dispatch(UiCommand::OpenTimeline(TimelineType::List { id: list.id, title: list.title })),
	);
}

/// Announces a change to the set of lists and refetches it.
fn changed(ctx: &NetworkResponseContext<'_>, message: &str) {
	ctx.announce(message);
	if let Some(requests) = ctx.requests() {
		fetch(&requests);
	}
}

fn account_lists_fetched(ctx: &NetworkResponseContext<'_>, account_id: String, lists: &[List]) {
	if lists.is_empty() {
		ctx.announce("User is not in any lists");
	} else {
		let lists = lists.to_vec();
		deferred::dialog_some(
			ctx,
			move |frame| dialogs::show_list_selection_dialog(frame, &lists, "Remove from List", "Remove"),
			move |ctx, list| {
				if let Some(requests) = ctx.requests() {
					remove_member(&requests, list.id, account_id);
				}
			},
		);
	}
}

fn membership_changed(ctx: &NetworkResponseContext<'_>, message: &str) {
	ctx.announce(message);
	if let Some(dlg) = &ctx.state.manage_list_members_dialog
		&& let Some(requests) = ctx.requests()
	{
		fetch_members(&requests, dlg.get_list_id().to_string());
	}
}

fn accounts_fetched(ctx: &mut NetworkResponseContext<'_>, list_id: String, members: Vec<Account>) {
	if let Some(dlg) = &ctx.state.manage_list_members_dialog
		&& dlg.get_list_id() == list_id
	{
		dlg.update_members(members);
		return;
	}
	let Some(dlg) = &ctx.state.manage_lists_dialog else { return };
	let Some(requests) = ctx.requests() else { return };
	let list_title = dlg.get_list_title(&list_id).unwrap_or_default();
	let ui_tx_dlg = ctx.ui_tx.clone();
	let members_dlg =
		dialogs::ManageListMembersDialog::new(dlg.get_dialog(), list_id, &list_title, members, requests, move || {
			let _ = ui_tx_dlg.send(UiCommand::ManageListMembersDialogClosed);
		});
	members_dlg.show();
	ctx.state.manage_list_members_dialog = Some(members_dlg);
}

fn operation_failed(ctx: &mut NetworkResponseContext<'_>, err: &anyhow::Error) {
	ctx.state.pending_add_to_list_user = None;
	// Over the lists dialog when that's what's open, so the error isn't hidden behind it.
	let parent: Option<Dialog> = ctx
		.state
		.manage_list_members_dialog
		.as_ref()
		.map(|dlg| *dlg.get_dialog())
		.or_else(|| ctx.state.manage_lists_dialog.as_ref().map(|dlg| *dlg.get_dialog()));
	let message = anyhow::anyhow!("{err:#}");
	deferred::show(ctx, move |frame| match &parent {
		Some(parent) => dialogs::show_error(parent, &message),
		None => dialogs::show_error(frame, &message),
	});
}
