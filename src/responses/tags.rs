use anyhow::Result;

use super::{NetworkResponseContext, helpers::summarize_api_error, timeline_updates::update_tag_in_timelines};
use crate::{UiCommand, mastodon::Tag, ui::dialogs};

pub(super) fn following_changed<T>(
	ctx: &mut NetworkResponseContext<'_>,
	name: &str,
	result: &Result<T>,
	following: bool,
) {
	match result {
		Ok(_) => {
			update_tag_in_timelines(ctx.state, name, following);
			if let Some(dlg) = &ctx.state.hashtag_dialog {
				dlg.update_tag(name, following);
			}
			ctx.announce(&format!("{} #{name}", if following { "Followed" } else { "Unfollowed" }));
		}
		Err(err) => ctx.announce(&format!(
			"Failed to {} #{name}: {}",
			if following { "follow" } else { "unfollow" },
			summarize_api_error(err)
		)),
	}
}

pub(super) fn muted_changed<T>(ctx: &mut NetworkResponseContext<'_>, name: &str, result: &Result<T>, muted: bool) {
	match result {
		Ok(_) => {
			if let Some(dlg) = &ctx.state.hashtag_dialog {
				dlg.update_tag_muted(name, muted);
			}
			ctx.announce(&format!("{} #{name}", if muted { "Muted" } else { "Unmuted" }));
		}
		Err(err) => ctx.announce(&format!(
			"Failed to {} #{name}: {}",
			if muted { "mute" } else { "unmute" },
			summarize_api_error(err)
		)),
	}
}

pub(super) fn info_fetched(ctx: &mut NetworkResponseContext<'_>, tags: Vec<Tag>) {
	let Some(net_tx) = ctx.state.network_handle.as_ref().map(|h| h.command_tx.clone()) else { return };
	let ui_tx_dlg = ctx.ui_tx.clone();
	let dlg = dialogs::HashtagDialog::new(ctx.frame, tags, net_tx, ctx.ui_tx.clone(), move || {
		let _ = ui_tx_dlg.send(UiCommand::HashtagDialogClosed);
	});
	dlg.show();
	ctx.state.hashtag_dialog = Some(dlg);
}
