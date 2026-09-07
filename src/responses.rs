mod accounts;
mod helpers;
mod lists;
mod statuses;
mod stream;
mod tags;
mod timeline_updates;
mod timelines;

use std::cell::{Cell, RefCell};

pub use stream::process_stream_events;
use wxdragon::prelude::*;

use self::helpers::{spoken_failure, summarize_api_error};
use crate::{
	AppState, UiCommand,
	config::{AutoloadMode, ShortcutsConfig, SortOrder},
	network::NetworkResponse,
	timeline::TimelineType,
	ui::{
		commands::{UiCommandContext, handle_ui_command},
		menu::update_menu_labels,
		timeline_list::TimelineList,
		timeline_view::update_active_timeline_ui,
	},
	ui_wake::UiCommandSender,
};

/// Everything a response handler needs to update application state and the UI.
pub struct NetworkResponseContext<'a> {
	pub frame: &'a Frame,
	pub state: &'a mut AppState,
	pub timelines_selector: ListBox,
	pub timeline_list: TimelineList,
	pub suppress_selection: &'a Cell<bool>,
	pub live_region: &'a TimelineList,
	pub quick_action_keys_enabled: &'a Cell<bool>,
	pub autoload_mode: &'a Cell<AutoloadMode>,
	pub sort_order_cell: &'a Cell<SortOrder>,
	pub tray_hidden: &'a Cell<bool>,
	pub shortcuts_cell: &'a RefCell<ShortcutsConfig>,
	pub ui_tx: &'a UiCommandSender,
}

impl NetworkResponseContext<'_> {
	fn announce(&self, message: &str) {
		self.live_region.announce(message);
	}

	fn announce_failure(&self, prefix: &str, err: &anyhow::Error) {
		self.announce(&spoken_failure(prefix, err));
	}

	/// Runs a UI command through the same path the main loop uses.
	fn dispatch(&mut self, cmd: UiCommand) {
		let mut ctx = UiCommandContext {
			state: &mut *self.state,
			frame: self.frame,
			timelines_selector: self.timelines_selector,
			timeline_list: self.timeline_list.clone(),
			suppress_selection: self.suppress_selection,
			live_region: self.live_region,
			quick_action_keys_enabled: self.quick_action_keys_enabled,
			autoload_mode: self.autoload_mode,
			sort_order_cell: self.sort_order_cell,
			tray_hidden: self.tray_hidden,
			shortcuts_cell: self.shortcuts_cell,
			ui_tx: self.ui_tx,
		};
		handle_ui_command(cmd, &mut ctx);
	}

	/// Redraws the active timeline so entry changes reach the screen reader.
	fn refresh_active_timeline(&mut self) {
		let state = &mut *self.state;
		let view_options = state.timeline_manager.active().map(|a| state.timeline_view_options_for(&a.timeline_type));
		let active_index = state.timeline_manager.active_index();
		if let Some(view_options) = view_options
			&& let Some(active) = state.timeline_manager.active_mut()
		{
			update_active_timeline_ui(
				&self.timeline_list,
				active,
				self.suppress_selection,
				&view_options,
				&state.cw_expanded,
				active_index,
			);
		}
	}

	fn refresh_menu_labels(&mut self) {
		if let Some(mb) = self.frame.get_menu_bar() {
			update_menu_labels(&mb, self.state);
		}
	}
}

/// Processes network responses from the background network thread.
pub fn process_network_responses(ctx: &mut NetworkResponseContext<'_>) {
	let Some(handle) = &ctx.state.network_handle else { return };
	let responses = handle.drain();
	let active_type = ctx.state.timeline_manager.active().map(|t| t.timeline_type.clone());
	for response in responses {
		handle_response(ctx, response, active_type.as_ref());
	}
}

fn handle_response(
	ctx: &mut NetworkResponseContext<'_>,
	response: NetworkResponse,
	active_type: Option<&TimelineType>,
) {
	match response {
		NetworkResponse::TimelineLoaded { timeline_type, result: Ok(data), max_id } => {
			timelines::loaded(ctx, timeline_type, data, max_id, active_type);
		}
		NetworkResponse::TimelineLoaded { timeline_type, result: Err(err), max_id } => {
			timelines::load_failed(ctx, &timeline_type, &err, max_id.is_some());
		}
		NetworkResponse::StatusResolvedForThread { result: Ok(focus) } => {
			ctx.ui_tx.send(UiCommand::ViewResolvedThread(Box::new(focus))).unwrap();
		}
		NetworkResponse::StatusResolvedForThread { result: Err(err) } => {
			ctx.announce(&format!("Failed to resolve thread: {}", summarize_api_error(&err)));
		}
		NetworkResponse::StatusResolvedForQuote { result: Ok(focus) } => {
			ctx.ui_tx.send(UiCommand::PromptForQuote(Box::new(focus))).unwrap();
		}
		NetworkResponse::StatusResolvedForQuote { result: Err(err) } => {
			ctx.announce(&format!("Failed to resolve post for quote: {}", summarize_api_error(&err)));
		}
		NetworkResponse::StatusSourceFetched { status, result } => statuses::source_fetched(ctx, *status, result),
		NetworkResponse::AccountLookupResult { handle: _, result: Ok(account) } => {
			accounts::lookup_succeeded(ctx, &account);
		}
		NetworkResponse::AccountLookupResult { handle, result: Err(err) } => {
			ctx.state.pending_user_lookup_action = None;
			ctx.announce(&format!("Failed to find user {handle}: {}", summarize_api_error(&err)));
		}
		NetworkResponse::PostComplete(result) => statuses::post_complete(ctx, result),
		NetworkResponse::Favorited { status_id, result } => {
			statuses::action(ctx, &status_id, result, &statuses::FAVORITE);
		}
		NetworkResponse::Unfavorited { status_id, result } => {
			statuses::action(ctx, &status_id, result, &statuses::UNFAVORITE);
		}
		NetworkResponse::Bookmarked { status_id, result } => {
			statuses::action(ctx, &status_id, result, &statuses::BOOKMARK);
		}
		NetworkResponse::Unbookmarked { status_id, result } => {
			statuses::action(ctx, &status_id, result, &statuses::UNBOOKMARK);
		}
		NetworkResponse::Pinned { status_id, result } => statuses::action(ctx, &status_id, result, &statuses::PIN),
		NetworkResponse::Unpinned { status_id, result } => statuses::action(ctx, &status_id, result, &statuses::UNPIN),
		NetworkResponse::Boosted { status_id, result } => statuses::action(ctx, &status_id, result, &statuses::BOOST),
		NetworkResponse::Unboosted { status_id, result } => {
			statuses::action(ctx, &status_id, result, &statuses::UNBOOST);
		}
		NetworkResponse::Replied(result) => statuses::replied(ctx, result),
		NetworkResponse::StatusDeleted { status_id, result: Ok(()) } => statuses::deleted(ctx, &status_id),
		NetworkResponse::StatusDeleted { result: Err(err), .. } => ctx.announce_failure("Failed to delete", &err),
		NetworkResponse::StatusEdited { _status_id: _, result: Ok(status) } => statuses::edited(ctx, &status),
		NetworkResponse::StatusEdited { result: Err(err), .. } => ctx.announce_failure("Failed to edit", &err),
		NetworkResponse::PollVoted { result } => statuses::poll_voted(ctx, result),
		NetworkResponse::TagFollowed { name, result } => tags::following_changed(ctx, &name, &result, true),
		NetworkResponse::TagUnfollowed { name, result } => tags::following_changed(ctx, &name, &result, false),
		NetworkResponse::TagMuted { name, result } => tags::muted_changed(ctx, &name, &result, true),
		NetworkResponse::TagUnmuted { name, result } => tags::muted_changed(ctx, &name, &result, false),
		NetworkResponse::TagsInfoFetched { result: Ok(tags) } => tags::info_fetched(ctx, tags),
		NetworkResponse::TagsInfoFetched { result: Err(err) } => {
			ctx.announce_failure("Failed to load hashtags", &err);
		}
		NetworkResponse::RebloggedByLoaded { result: Ok(accounts) } => {
			accounts::pick_from_account_list(ctx, &accounts, "Boosts", "Users who boosted this post");
		}
		NetworkResponse::RebloggedByLoaded { result: Err(err) } => {
			ctx.announce_failure("Failed to load boosts", &err);
		}
		NetworkResponse::FavoritedByLoaded { result: Ok(accounts) } => {
			accounts::pick_from_account_list(ctx, &accounts, "Favorites", "Users who favorited this post");
		}
		NetworkResponse::FavoritedByLoaded { result: Err(err) } => {
			ctx.announce_failure("Failed to load favorites", &err);
		}
		NetworkResponse::FollowersLoaded { result, total_count, account_id } => {
			accounts::follow_list_loaded(ctx, accounts::FollowList::Followers, result, total_count, account_id);
		}
		NetworkResponse::FollowingLoaded { result, total_count, account_id } => {
			accounts::follow_list_loaded(ctx, accounts::FollowList::Following, result, total_count, account_id);
		}
		NetworkResponse::FollowersNextPageLoaded { result } => {
			accounts::follow_list_next_page(ctx, accounts::FollowList::Followers, result);
		}
		NetworkResponse::FollowingNextPageLoaded { result } => {
			accounts::follow_list_next_page(ctx, accounts::FollowList::Following, result);
		}
		NetworkResponse::RelationshipsForListLoaded { results, for_followers } => {
			let dialog = if for_followers { &ctx.state.followers_dialog } else { &ctx.state.following_dialog };
			if let Some(dlg) = dialog {
				dlg.update_relationships(&results);
			}
		}
		NetworkResponse::RelationshipUpdated { _account_id: _, target_name, action, result } => {
			accounts::relationship_updated(ctx, &target_name, action, result);
		}
		NetworkResponse::RelationshipLoaded { _account_id: _, result } => {
			if let Ok(rel) = result
				&& let Some(dlg) = &ctx.state.profile_dialog
			{
				dlg.update_relationship(&rel);
			}
		}
		NetworkResponse::AccountFetched { result } => {
			if let Ok(account) = result
				&& let Some(dlg) = &ctx.state.profile_dialog
			{
				dlg.update_account(&account);
			}
		}
		NetworkResponse::CredentialsFetched { result: Ok(account) } => accounts::credentials_fetched(ctx, &account),
		NetworkResponse::CredentialsFetched { result: Err(err) } => {
			ctx.announce_failure("Failed to fetch profile", &err);
		}
		NetworkResponse::ProfileUpdated { result: Ok(account) } => accounts::profile_updated(ctx, account),
		NetworkResponse::ProfileUpdated { result: Err(err) } => {
			ctx.announce_failure("Failed to update profile", &err);
		}
		NetworkResponse::SearchLoaded { query, search_type, result: Ok(results), offset } => {
			timelines::search_loaded(ctx, query, search_type, results, offset, active_type);
		}
		NetworkResponse::SearchLoaded { query, search_type, result: Err(err), .. } => {
			timelines::search_failed(ctx, &query, search_type, &err);
		}
		NetworkResponse::ListsFetched { result: Ok(lists) } => lists::fetched(ctx, lists),
		NetworkResponse::ListsFetched { result: Err(err) } => ctx.announce_failure("Failed to fetch lists", &err),
		NetworkResponse::ListCreated { result: Ok(list) } => {
			lists::changed(ctx, &format!("List '{}' created", list.title));
		}
		NetworkResponse::ListUpdated { result: Ok(list) } => {
			lists::changed(ctx, &format!("List '{}' updated", list.title));
		}
		NetworkResponse::ListDeleted { id: _, result: Ok(()) } => lists::changed(ctx, "List deleted"),
		NetworkResponse::ListAccountsFetched { list_id, result: Ok(members) } => {
			lists::accounts_fetched(ctx, list_id, members);
		}
		NetworkResponse::ListAccountAdded { result: Ok(()), .. } => lists::membership_changed(ctx, "Member added"),
		NetworkResponse::ListAccountRemoved { result: Ok(()), .. } => lists::membership_changed(ctx, "Member removed"),
		NetworkResponse::ListCreated { result: Err(err) }
		| NetworkResponse::ListUpdated { result: Err(err) }
		| NetworkResponse::ListDeleted { result: Err(err), .. }
		| NetworkResponse::ListAccountsFetched { result: Err(err), .. }
		| NetworkResponse::ListAccountAdded { result: Err(err), .. }
		| NetworkResponse::ListAccountRemoved { result: Err(err), .. } => lists::operation_failed(ctx, &err),
	}
}
