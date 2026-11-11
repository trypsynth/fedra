use wxdragon::prelude::*;

use super::NetworkResponseContext;
use crate::{
	UiCommand,
	mastodon::{Account, List},
	network::NetworkCommand,
	timeline::TimelineType,
	ui::{commands::deferred, dialogs},
};

pub(super) fn fetched(ctx: &mut NetworkResponseContext<'_>, lists: Vec<List>) {
	if let Some(account_id) = ctx.state.pending_add_to_list_user.take() {
		if lists.is_empty() {
			ctx.announce("No lists found to add user to");
		} else {
			deferred::dialog_some(
				ctx,
				move |frame| dialogs::show_list_selection_dialog(frame, &lists, "Add to List", "Add"),
				move |ctx, list| {
					if let Some(handle) = &ctx.state.network_handle {
						handle.send(NetworkCommand::AddListAccount { list_id: list.id, account_id });
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
pub(super) fn changed(ctx: &NetworkResponseContext<'_>, message: &str) {
	ctx.announce(message);
	if let Some(handle) = &ctx.state.network_handle {
		handle.send(NetworkCommand::FetchLists);
	}
}

pub(super) fn account_lists_fetched(ctx: &NetworkResponseContext<'_>, account_id: String, lists: &[List]) {
	if lists.is_empty() {
		ctx.announce("User is not in any lists");
	} else {
		let lists = lists.to_vec();
		deferred::dialog_some(
			ctx,
			move |frame| dialogs::show_list_selection_dialog(frame, &lists, "Remove from List", "Remove"),
			move |ctx, list| {
				if let Some(handle) = &ctx.state.network_handle {
					handle.send(NetworkCommand::RemoveListAccount { list_id: list.id, account_id });
				}
			},
		);
	}
}

pub(super) fn membership_changed(ctx: &NetworkResponseContext<'_>, message: &str) {
	ctx.announce(message);
	if let Some(dlg) = &ctx.state.manage_list_members_dialog
		&& let Some(handle) = &ctx.state.network_handle
	{
		handle.send(NetworkCommand::FetchListAccounts { list_id: dlg.get_list_id().to_string() });
	}
}

pub(super) fn accounts_fetched(ctx: &mut NetworkResponseContext<'_>, list_id: String, members: Vec<Account>) {
	if let Some(dlg) = &ctx.state.manage_list_members_dialog
		&& dlg.get_list_id() == list_id
	{
		dlg.update_members(members);
		return;
	}
	let Some(dlg) = &ctx.state.manage_lists_dialog else { return };
	let Some(handle) = &ctx.state.network_handle else { return };
	let list_title = dlg.get_list_title(&list_id).unwrap_or_default();
	let net_tx = handle.command_tx.clone();
	let ui_tx_dlg = ctx.ui_tx.clone();
	let members_dlg =
		dialogs::ManageListMembersDialog::new(dlg.get_dialog(), list_id, &list_title, members, net_tx, move || {
			let _ = ui_tx_dlg.send(UiCommand::ManageListMembersDialogClosed);
		});
	members_dlg.show();
	ctx.state.manage_list_members_dialog = Some(members_dlg);
}

pub(super) fn operation_failed(ctx: &mut NetworkResponseContext<'_>, err: &anyhow::Error) {
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
