mod accounts;
mod helpers;
pub mod lists;
pub mod statuses;
mod stream;
mod tags;
mod timeline_updates;
mod timelines;

pub use accounts::{edit_profile, fill_in_profile, look_up, open_account, relationship_updated};
pub use helpers::spoken_failure;
pub use statuses::action as status_acted;
pub use stream::process_stream_events;
pub use tags::{
	following_changed as tag_following_changed, info_fetched as tags_fetched, muted_changed as tag_muted_changed,
};
pub use timelines::search;

use crate::{
	UiCommand,
	network::NetworkResponse,
	ui::{
		commands::{UiCommandContext, handle_ui_command},
		menu::update_menu_labels,
	},
};

/// Responses are handled with the same context as UI commands, so a handler can run one directly.
pub type NetworkResponseContext<'a> = UiCommandContext<'a>;

impl NetworkResponseContext<'_> {
	fn announce(&self, message: &str) {
		self.live_region.announce(message);
	}

	fn announce_failure(&self, prefix: &str, err: &anyhow::Error) {
		self.announce(&spoken_failure(prefix, err));
	}

	fn dispatch(&mut self, cmd: UiCommand) {
		handle_ui_command(cmd, self);
	}

	/// Redraws the active timeline so entry changes reach the screen reader.
	fn refresh_active_timeline(&mut self) {
		self.state.redraw_active_timeline(&self.timeline_list, self.suppress_selection);
	}

	fn refresh_menu_labels(&self) {
		if let Some(mb) = self.frame.get_menu_bar() {
			update_menu_labels(&mb, self.state);
		}
	}
}

/// Processes network responses from the background network thread.
pub fn process_network_responses(ctx: &mut NetworkResponseContext<'_>) {
	let Some(handle) = &ctx.state.network_handle else { return };
	for response in handle.drain() {
		handle_response(ctx, response);
	}
}

fn handle_response(ctx: &mut NetworkResponseContext<'_>, response: NetworkResponse) {
	match response {
		NetworkResponse::TimelineLoaded { timeline_type, result: Ok(data), max_id } => {
			timelines::loaded(ctx, &timeline_type, data, max_id.as_deref());
		}
		NetworkResponse::TimelineLoaded { timeline_type, result: Err(err), max_id } => {
			timelines::load_failed(ctx, &timeline_type, &err, max_id.is_some());
		}
		NetworkResponse::InstanceInfoFetched(Ok(info)) => accounts::instance_info_fetched(ctx.state, info),
		NetworkResponse::MarkersLoaded(Ok(markers)) => crate::read_position::apply_markers(ctx.state, markers),
		// Servers before Mastodon 4.3 have no notification policy, and so no requests.
		NetworkResponse::InstanceInfoFetched(Err(_))
		| NetworkResponse::MarkersLoaded(Err(_))
		| NetworkResponse::NotificationPolicyLoaded(Err(_)) => {}
		NetworkResponse::NotificationPolicyLoaded(Ok(policy)) => {
			let count = policy.summary.pending_requests_count;
			// The first check after launch or an account switch only sets the baseline, which the
			// File menu shows, so requests that were already waiting don't sound like new ones.
			if ctx.state.pending_message_requests.replace(count).is_some_and(|previous| count > previous) {
				crate::notifications::notify_message_requests(ctx.state, count);
			}
			if let Some(menu_bar) = ctx.frame.get_menu_bar() {
				update_menu_labels(&menu_bar, ctx.state);
			}
		}
		NetworkResponse::SearchLoaded { query, search_type, result: Ok(results), offset } => {
			timelines::search_loaded(ctx, query, search_type, results, offset);
		}
		NetworkResponse::SearchLoaded { query, search_type, result: Err(err), .. } => {
			timelines::search_failed(ctx, &query, search_type, &err);
		}
	}
}
