use anyhow::Result;

use super::{NetworkResponseContext, helpers::summarize_api_error, timeline_updates::update_tag_in_timelines};
use crate::{UiCommand, mastodon::Tag, soundpack::SoundEvent, ui::dialogs};

pub fn following_changed<T>(ctx: &mut NetworkResponseContext<'_>, name: &str, result: &Result<T>, following: bool) {
	match result {
		Ok(_) => {
			update_tag_in_timelines(ctx.state, name, following);
			if let Some(dlg) = &ctx.state.hashtag_dialog {
				dlg.update_tag(name, following);
			}
			if following {
				ctx.play(SoundEvent::Follow);
			}
			ctx.announce(&format!("{} #{name}", if following { "Followed" } else { "Unfollowed" }));
		}
		Err(err) => ctx.announce_error(&format!(
			"Failed to {} #{name}: {}",
			if following { "follow" } else { "unfollow" },
			summarize_api_error(err)
		)),
	}
}

pub fn muted_changed<T>(ctx: &NetworkResponseContext<'_>, name: &str, result: &Result<T>, muted: bool) {
	match result {
		Ok(_) => {
			if let Some(dlg) = &ctx.state.hashtag_dialog {
				dlg.update_tag_muted(name, muted);
			}
			ctx.announce(&format!("{} #{name}", if muted { "Muted" } else { "Unmuted" }));
		}
		Err(err) => ctx.announce_error(&format!(
			"Failed to {} #{name}: {}",
			if muted { "mute" } else { "unmute" },
			summarize_api_error(err)
		)),
	}
}

pub fn info_fetched(ctx: &mut NetworkResponseContext<'_>, tags: Vec<Tag>) {
	show(ctx, "Hashtags", "Hashtags in post:", tags);
}

pub fn followed_fetched(ctx: &mut NetworkResponseContext<'_>, result: Result<Vec<Tag>>) {
	match result {
		Ok(tags) if tags.is_empty() => ctx.announce("You don't follow any hashtags"),
		Ok(tags) => show(ctx, "Followed Hashtags", "Hashtags you follow:", tags),
		Err(err) => ctx.announce_failure("Failed to load followed hashtags", &err),
	}
}

fn show(ctx: &mut NetworkResponseContext<'_>, title: &str, label: &str, tags: Vec<Tag>) {
	let Some(requests) = ctx.requests() else { return };
	let ui_tx_dlg = ctx.ui_tx.clone();
	let dlg = dialogs::HashtagDialog::new(ctx.frame, title, label, tags, requests, ctx.ui_tx.clone(), move || {
		let _ = ui_tx_dlg.send(UiCommand::HashtagDialogClosed);
	});
	if let Some(previous) = ctx.state.hashtag_dialog.replace(dlg.clone()) {
		previous.destroy();
	}
	dlg.show();
}
