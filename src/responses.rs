mod accounts;
mod helpers;
pub mod lists;
pub mod statuses;
mod stream;
mod tags;
mod timeline_updates;
pub mod timelines;

pub use accounts::{
	edit_profile, fetch_instance_info, fetch_markers, fetch_notification_policy, fetch_posting_defaults,
	fill_in_profile, look_up, note_saved, open_account, relationship_updated,
};
pub use helpers::spoken_failure;
pub use statuses::action as status_acted;
pub use stream::process_stream_events;
pub use tags::{
	followed_fetched as followed_tags_fetched, following_changed as tag_following_changed,
	info_fetched as tags_fetched, muted_changed as tag_muted_changed,
};
pub use timelines::search;

use crate::{
	UiCommand,
	ui::{
		commands::{UiCommandContext, handle_ui_command},
		menu::update_menu_labels,
	},
};

/// Responses are handled with the same context as UI commands, so a handler can run one directly.
pub type NetworkResponseContext<'a> = UiCommandContext<'a>;

impl NetworkResponseContext<'_> {
	fn announce(&self, message: &str) {
		tracing::debug!(message, "announcing");
		self.live_region.announce(message);
	}

	fn announce_failure(&self, prefix: &str, err: &anyhow::Error) {
		tracing::warn!(error = format!("{err:#}"), "{prefix}");
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
