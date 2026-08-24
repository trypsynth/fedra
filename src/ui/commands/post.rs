//! Commands that compose, edit, or act on a single post.

use wxdragon::prelude::*;

use super::{
	UiCommand, UiCommandContext, handle_ui_command,
	selection::{foreign_url, get_selected_entry, get_selected_status},
};
use crate::{
	AppState,
	config::{ContentWarningDisplay, SortOrder},
	html,
	mastodon::Status,
	network,
	network::{ForeignInteraction, NetworkCommand},
	timeline::{TimelineTextOptions, TimelineType},
	ui::{
		dialogs,
		timeline_view::{list_index_to_entry_index, update_active_timeline_ui},
	},
};

pub(super) fn post_result_to_data(post: dialogs::PostResult, quoted_status_id: Option<String>) -> network::PostData {
	network::PostData {
		content: post.content,
		visibility: post.visibility.as_api_str().to_string(),
		sensitive: post.sensitive,
		spoiler_text: post.spoiler_text,
		content_type: post.content_type,
		language: post.language,
		media: post
			.media
			.into_iter()
			.map(|item| network::MediaUpload { path: item.path, description: item.description })
			.collect(),
		poll: post.poll.map(|poll| network::PollData {
			options: poll.options,
			expires_in: poll.expires_in,
			multiple: poll.multiple,
			hide_totals: poll.hide_totals,
		}),
		quoted_status_id,
		scheduled_at: post.scheduled_at,
	}
}

pub(super) fn do_favorite(state: &AppState, live_region: &crate::ui::timeline_list::TimelineList) {
	let Some(status) = get_selected_status(state) else {
		live_region.announce("No post selected");
		return;
	};
	let Some(handle) = &state.network_handle else {
		live_region.announce("Network not available");
		return;
	};
	let target = status.reblog.as_ref().map_or(status, std::convert::AsRef::as_ref);

	let is_foreign =
		matches!(state.timeline_manager.active().map(|t| &t.timeline_type), Some(TimelineType::InstanceLocal { .. }));
	if is_foreign {
		if let Some(url) = &target.url {
			let interaction =
				if target.favourited { ForeignInteraction::Unfavorite } else { ForeignInteraction::Favorite };
			handle.send(NetworkCommand::ResolveAndInteract { url: url.clone(), interaction });
			return;
		}
	}

	let status_id = target.id.clone();
	if target.favourited {
		handle.send(NetworkCommand::Unfavorite { status_id });
	} else {
		handle.send(NetworkCommand::Favorite { status_id });
	}
}

pub(super) fn do_bookmark(state: &AppState, live_region: &crate::ui::timeline_list::TimelineList) {
	let Some(status) = get_selected_status(state) else {
		live_region.announce("No post selected");
		return;
	};
	let Some(handle) = &state.network_handle else {
		live_region.announce("Network not available");
		return;
	};
	let target = status.reblog.as_ref().map_or(status, std::convert::AsRef::as_ref);

	if let Some(url) = foreign_url(state, target.url.as_ref()) {
		let interaction = if target.bookmarked { ForeignInteraction::Unbookmark } else { ForeignInteraction::Bookmark };
		handle.send(NetworkCommand::ResolveAndInteract { url, interaction });
		return;
	}

	let status_id = target.id.clone();
	if target.bookmarked {
		handle.send(NetworkCommand::Unbookmark { status_id });
	} else {
		handle.send(NetworkCommand::Bookmark { status_id });
	}
}

pub(super) fn do_pin(state: &AppState, live_region: &crate::ui::timeline_list::TimelineList) {
	let Some(status) = get_selected_status(state) else {
		live_region.announce("No post selected");
		return;
	};
	let Some(handle) = &state.network_handle else {
		live_region.announce("Network not available");
		return;
	};
	let target = status.reblog.as_ref().map_or(status, std::convert::AsRef::as_ref);
	if Some(&target.account.id) != state.current_user_id.as_ref() {
		live_region.announce("You can only pin your own posts");
		return;
	}

	let is_foreign =
		matches!(state.timeline_manager.active().map(|t| &t.timeline_type), Some(TimelineType::InstanceLocal { .. }));
	if is_foreign {
		if let Some(url) = &target.url {
			let interaction = if target.pinned { ForeignInteraction::Unpin } else { ForeignInteraction::Pin };
			handle.send(NetworkCommand::ResolveAndInteract { url: url.clone(), interaction });
			return;
		}
	}

	let status_id = target.id.clone();
	if target.pinned {
		handle.send(NetworkCommand::Unpin { status_id });
	} else {
		handle.send(NetworkCommand::Pin { status_id });
	}
}

pub(super) fn do_boost(state: &AppState, live_region: &crate::ui::timeline_list::TimelineList) {
	let Some(status) = get_selected_status(state) else {
		live_region.announce("No post selected");
		return;
	};
	let Some(handle) = &state.network_handle else {
		live_region.announce("Network not available");
		return;
	};
	let target = status.reblog.as_ref().map_or(status, std::convert::AsRef::as_ref);
	if target.visibility == "direct" {
		live_region.announce("Cannot boost direct messages");
		return;
	}

	if let Some(url) = foreign_url(state, target.url.as_ref()) {
		let interaction = if target.reblogged { ForeignInteraction::Unboost } else { ForeignInteraction::Boost };
		handle.send(NetworkCommand::ResolveAndInteract { url, interaction });
		return;
	}

	let status_id = target.id.clone();
	if target.reblogged {
		handle.send(NetworkCommand::Unboost { status_id });
	} else {
		handle.send(NetworkCommand::Boost { status_id });
	}
}

pub fn run_edit_post_dialog(
	frame: &Frame,
	state: &mut AppState,
	target: &crate::mastodon::Status,
	source_text: Option<&str>,
) {
	let max_post_chars = state.max_post_chars;
	let enter_to_send = state.config.enter_to_send;
	let Some((edit, config)) =
		dialogs::prompt_for_edit(frame, target, source_text, max_post_chars, &state.poll_limits, enter_to_send)
	else {
		return;
	};
	if let Some(handle) = &state.network_handle {
		state.pending_post = Some(crate::PendingPost {
			config,
			operation: crate::PostOperation::Edit { status_id: target.id.clone() },
			last_result: edit.clone(),
		});
		let media = edit
			.media
			.into_iter()
			.map(|item| {
				if item.is_existing {
					network::EditMedia::Existing(item.path)
				} else {
					network::EditMedia::New(network::MediaUpload { path: item.path, description: item.description })
				}
			})
			.collect();

		handle.send(NetworkCommand::EditStatus {
			status_id: target.id.clone(),
			content: edit.content,
			sensitive: edit.sensitive,
			spoiler_text: edit.spoiler_text,
			language: edit.language,
			media,
			poll: edit.poll.map(|poll| network::PollData {
				options: poll.options,
				expires_in: poll.expires_in,
				multiple: poll.multiple,
				hide_totals: poll.hide_totals,
			}),
		});
	}
}

pub(super) fn new_post(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	let frame = ctx.frame;
	let live_region = ctx.live_region;
	let (has_account, max_post_chars, poll_limits, enter_to_send) =
		(state.active_account().is_some(), state.max_post_chars, state.poll_limits.clone(), state.config.enter_to_send);
	if !has_account {
		live_region.announce("No account configured");
		return;
	}
	let default_visibility = if state.timeline_manager.active().is_some_and(|t| t.timeline_type == TimelineType::Direct)
	{
		Some(dialogs::PostVisibility::Direct)
	} else {
		state.active_account().and_then(|a| a.default_post_visibility.as_deref()).and_then(|v| match v {
			"public" => Some(dialogs::PostVisibility::Public),
			"unlisted" => Some(dialogs::PostVisibility::Unlisted),
			"private" => Some(dialogs::PostVisibility::Private),
			"direct" => Some(dialogs::PostVisibility::Direct),
			_ => None,
		})
	};
	let Some((post, config)) =
		dialogs::prompt_for_post(frame, max_post_chars, &poll_limits, enter_to_send, default_visibility)
	else {
		return;
	};
	if let Some(handle) = &state.network_handle {
		state.pending_thread_continuation = post.continue_thread;
		state.pending_post =
			Some(crate::PendingPost { config, operation: crate::PostOperation::NewPost, last_result: post.clone() });
		handle.send(NetworkCommand::PostStatus { post: post_result_to_data(post, None) });
	} else {
		live_region.announce("Network not available");
	}
}

pub(super) fn continue_thread(ctx: &mut UiCommandContext<'_>, mut status: Box<Status>) {
	let state = &mut *ctx.state;
	let frame = ctx.frame;
	let live_region = ctx.live_region;
	if status.visibility == "public" {
		status.visibility = "unlisted".to_string();
	}
	let (max_post_chars, enter_to_send) = (state.max_post_chars, state.config.enter_to_send);
	let self_acct = state.active_account().and_then(|account| account.acct.as_deref());
	let Some((reply, config)) = dialogs::prompt_for_reply(
		frame,
		&status,
		max_post_chars,
		&state.poll_limits,
		true,
		self_acct,
		enter_to_send,
		true,
	) else {
		return;
	};
	if let Some(handle) = &state.network_handle {
		state.pending_thread_continuation = reply.continue_thread;
		state.pending_post = Some(crate::PendingPost {
			config,
			operation: crate::PostOperation::Reply { in_reply_to_id: status.id.clone() },
			last_result: reply.clone(),
		});
		let post_data = post_result_to_data(reply, None);
		handle.send(NetworkCommand::Reply {
			in_reply_to_id: status.id.clone(),
			content: post_data.content,
			visibility: post_data.visibility,
			sensitive: post_data.sensitive,
			spoiler_text: post_data.spoiler_text,
			content_type: post_data.content_type,
			language: post_data.language,
			media: post_data.media,
			poll: post_data.poll,
			scheduled_at: post_data.scheduled_at,
		});
	} else {
		live_region.announce("Network not available");
	}
}

pub(super) fn reply(ctx: &mut UiCommandContext<'_>, reply_all: bool) {
	let state = &mut *ctx.state;
	let frame = ctx.frame;
	let live_region = ctx.live_region;
	let (status, max_post_chars, enter_to_send) =
		(get_selected_status(state).cloned(), state.max_post_chars, state.config.enter_to_send);
	let Some(status) = status else {
		live_region.announce("No post selected");
		return;
	};
	let target = status.reblog.as_ref().map_or(&status, std::convert::AsRef::as_ref);
	let self_acct = state.active_account().and_then(|account| account.acct.as_deref());
	let Some((reply, config)) = dialogs::prompt_for_reply(
		frame,
		target,
		max_post_chars,
		&state.poll_limits,
		reply_all,
		self_acct,
		enter_to_send,
		false,
	) else {
		return;
	};
	if let Some(handle) = &state.network_handle {
		state.pending_thread_continuation = reply.continue_thread;
		state.pending_post = Some(crate::PendingPost {
			config,
			operation: crate::PostOperation::Reply { in_reply_to_id: target.id.clone() },
			last_result: reply.clone(),
		});
		let post_data = post_result_to_data(reply, None);
		let is_foreign = matches!(
			state.timeline_manager.active().map(|t| &t.timeline_type),
			Some(TimelineType::InstanceLocal { .. })
		);
		if is_foreign {
			if let Some(url) = &target.url {
				handle.send(NetworkCommand::ResolveAndInteract {
					url: url.clone(),
					interaction: ForeignInteraction::Reply(Box::new(post_data)),
				});
				return;
			}
		}
		handle.send(NetworkCommand::Reply {
			in_reply_to_id: target.id.clone(),
			content: post_data.content,
			visibility: post_data.visibility,
			sensitive: post_data.sensitive,
			spoiler_text: post_data.spoiler_text,
			content_type: post_data.content_type,
			language: post_data.language,
			media: post_data.media,
			poll: post_data.poll,
			scheduled_at: post_data.scheduled_at,
		});
	} else {
		live_region.announce("Network not available");
	}
}

pub(super) fn quote(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	let live_region = ctx.live_region;
	let status = get_selected_status(state).cloned();
	let Some(status) = status else {
		live_region.announce("No post selected");
		return;
	};
	let target = status.reblog.as_ref().map_or(&status, std::convert::AsRef::as_ref);
	if target.visibility == "direct" {
		live_region.announce("Cannot quote direct messages");
		return;
	}
	if let Some(url) = foreign_url(state, target.url.as_ref()) {
		if let Some(handle) = &state.network_handle {
			handle.send(NetworkCommand::ResolveStatusForQuote { url });
		} else {
			live_region.announce("Network not available");
		}
		return;
	}
	handle_ui_command(UiCommand::PromptForQuote(Box::new(target.clone())), ctx);
}

pub(super) fn prompt_for_quote(ctx: &mut UiCommandContext<'_>, target: Box<Status>) {
	let state = &mut *ctx.state;
	let frame = ctx.frame;
	let live_region = ctx.live_region;
	if let Some(approval) = &target.quote_approval
		&& approval.current_user == "denied"
	{
		live_region.announce("You are not allowed to quote this post");
		return;
	}
	let target_id = target.id.clone();
	let Some((post, config)) =
		dialogs::prompt_for_quote(frame, &target, state.max_post_chars, &state.poll_limits, state.config.enter_to_send)
	else {
		return;
	};
	if let Some(handle) = &state.network_handle {
		state.pending_thread_continuation = post.continue_thread;
		state.pending_post = Some(crate::PendingPost {
			config,
			operation: crate::PostOperation::Quote { quoted_status_id: target_id.clone() },
			last_result: post.clone(),
		});
		let post_data = post_result_to_data(post, Some(target_id));
		handle.send(NetworkCommand::PostStatus { post: post_data });
	} else {
		live_region.announce("Network not available");
	}
}

pub(super) fn delete_post(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	let frame = ctx.frame;
	let live_region = ctx.live_region;
	let Some(status) = get_selected_status(state) else {
		live_region.announce("No post selected");
		return;
	};
	let target = status.reblog.as_ref().map_or(status, std::convert::AsRef::as_ref);
	if let Some(current_user) = &state.current_user_id {
		if &target.account.id != current_user {
			live_region.announce("You can only delete your own posts");
			return;
		}
	} else {
		live_region.announce("Cannot verify ownership");
		return;
	}

	let confirm = MessageDialog::builder(frame, "Are you sure you want to delete this post?", "Delete Post")
		.with_style(MessageDialogStyle::YesNo | MessageDialogStyle::IconWarning)
		.build();
	if confirm.show_modal() == ID_YES {
		if let Some(handle) = &state.network_handle {
			handle.send(NetworkCommand::DeleteStatus { status_id: target.id.clone() });
		} else {
			live_region.announce("Network not available");
		}
	}
}

pub(super) fn edit_post(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	let live_region = ctx.live_region;
	let status = get_selected_status(state).cloned();
	let Some(status) = status else {
		live_region.announce("No post selected");
		return;
	};
	let target = status.reblog.as_ref().map_or(&status, std::convert::AsRef::as_ref);
	if let Some(current_user) = &state.current_user_id {
		if &target.account.id != current_user {
			live_region.announce("You can only edit your own posts");
			return;
		}
	} else {
		live_region.announce("Cannot verify ownership");
		return;
	}
	if let Some(handle) = &state.network_handle {
		handle.send(NetworkCommand::FetchStatusSource { status: Box::new(target.clone()) });
	} else {
		live_region.announce("Network not available");
	}
}

pub(super) fn copy_post(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	let live_region = ctx.live_region;
	let Some(entry) = get_selected_entry(state) else {
		live_region.announce("No post selected");
		return;
	};
	let mut options = state.timeline_manager.active().map_or_else(
		|| TimelineTextOptions::from_config_default(&state.config),
		|a| TimelineTextOptions::from_config(&state.config, &a.timeline_type),
	);
	let strip_stats = |template: &str| -> String {
		let mut t = template.to_string();
		let to_remove = [
			" - {{ relative_time }}",
			" - {{ absolute_time }}",
			"{{ relative_time }}",
			"{{ absolute_time }}",
			", {{ visibility }}",
			"{{ visibility }}",
			"{% if reply_count %}, {{ reply_count }}{% endif %}",
			"{% if boost_count %}, {{ boost_count }}{% endif %}",
			"{% if favorite_count %}, {{ favorite_count }}{% endif %}",
			"{% if client %}, via {{ client }}{% endif %}",
		];
		for s in to_remove {
			t = t.replace(s, "");
		}
		t = t.replace(" - ,", " -");
		t = t.replace(", ,", ",");
		let mut cleaned = t.trim().to_string();
		if cleaned.ends_with(" -") {
			cleaned.truncate(cleaned.len() - 2);
		}
		if cleaned.ends_with(',') {
			cleaned.pop();
		}
		cleaned.trim().to_string()
	};
	options.post_template = strip_stats(&options.post_template);
	options.boost_template = strip_stats(&options.boost_template);
	options.quote_template = strip_stats(&options.quote_template);
	options.show_link_previews = false;
	let is_expanded = state.cw_expanded.contains(entry.id());
	let mut text = entry.display_text(&options, is_expanded).trim().to_string();
	while text.ends_with(" -") || text.ends_with(',') {
		if text.ends_with(" -") {
			text.truncate(text.len() - 2);
		} else if text.ends_with(',') {
			text.pop();
		}
		text = text.trim().to_string();
	}
	if text.is_empty() {
		live_region.announce("Post has no text");
		return;
	}
	let clipboard = Clipboard::get();
	let _ = clipboard.set_text(&text);
	live_region.announce("Post copied");
}

pub(super) fn copy_post_link(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	let live_region = ctx.live_region;
	let Some(entry) = get_selected_entry(state) else {
		live_region.announce("No post selected");
		return;
	};
	if let Some(status) = entry.as_status() {
		let target = status.reblog.as_ref().map_or(status, std::convert::AsRef::as_ref);
		if let Some(url) = &target.url {
			let clipboard = Clipboard::get();
			let _ = clipboard.set_text(url);
			live_region.announce("Post link copied");
		} else {
			live_region.announce("Post has no link");
		}
	} else {
		live_region.announce("Selected item is not a post");
	}
}

pub(super) fn favorite(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	let live_region = ctx.live_region;
	do_favorite(state, live_region);
}

pub(super) fn bookmark(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	let live_region = ctx.live_region;
	do_bookmark(state, live_region);
}

pub(super) fn boost(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	let live_region = ctx.live_region;
	do_boost(state, live_region);
}

pub(super) fn pin(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	let live_region = ctx.live_region;
	do_pin(state, live_region);
}

pub(super) fn toggle_content_warning(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	let timeline_list = &ctx.timeline_list;
	let suppress_selection = ctx.suppress_selection;
	let live_region = ctx.live_region;
	if state.config.content_warning_display != ContentWarningDisplay::WarningOnly {
		return;
	}
	let _text_options = state.timeline_manager.active().map_or_else(
		|| TimelineTextOptions::from_config_default(&state.config),
		|a| TimelineTextOptions::from_config(&state.config, &a.timeline_type),
	);
	let timeline_type = state.timeline_manager.active().map(|a| a.timeline_type.clone());
	let view_options = timeline_type.map(|t| state.timeline_view_options_for(&t));
	let active_index = state.timeline_manager.active_index();
	let Some(active) = state.timeline_manager.active_mut() else { return };
	let Some(list_index) = active.selected_index else {
		live_region.announce("No post selected");
		return;
	};
	let effective_sort_order =
		if state.config.preserve_thread_order && matches!(active.timeline_type, TimelineType::Thread { .. }) {
			SortOrder::OldestToNewest
		} else {
			state.config.sort_order
		};
	let Some(entry_index) = list_index_to_entry_index(list_index, active.entries.len(), effective_sort_order) else {
		return;
	};
	let Some(entry) = active.entries.get(entry_index) else { return };
	let Some(status) = entry.as_status() else {
		live_region.announce("No post selected");
		return;
	};
	let target = status.reblog.as_ref().map_or(status, std::convert::AsRef::as_ref);
	if target.spoiler_text.trim().is_empty() {
		live_region.announce("No content warning");
		return;
	}
	let entry_id = entry.id();
	let expanded = state.cw_expanded.contains(entry_id);
	if expanded {
		state.cw_expanded.remove(entry_id);
	} else {
		state.cw_expanded.insert(entry_id.to_string());
	}
	if let Some(view_options) = view_options {
		update_active_timeline_ui(
			timeline_list,
			active,
			suppress_selection,
			&view_options,
			&state.cw_expanded,
			active_index,
		);
	}
}

pub(super) fn open_links(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	let frame = ctx.frame;
	let live_region = ctx.live_region;
	let Some(status) = get_selected_status(state) else { return };
	let target = status.reblog.as_ref().map_or(status, std::convert::AsRef::as_ref);
	let mut links = html::extract_links(&target.content);
	if let Some(quote) = &target.quote
		&& let Some(quoted_status) = &quote.quoted_status
	{
		let mut quote_links = html::extract_links(&quoted_status.content);
		links.append(&mut quote_links);
		if let Some(quote_url) = &quoted_status.url {
			links.retain(|link| link.url != *quote_url);
		}
	}
	// Remove duplicates while preserving order
	let mut seen = std::collections::HashSet::new();
	links.retain(|link| seen.insert(link.url.clone()));

	if links.is_empty() {
		live_region.announce("No links in this post");
		return;
	}
	if state.config.strip_tracking {
		for link in &mut links {
			link.url = html::clean_url(&link.url);
		}
	}
	let url_to_open = if links.len() == 1 && !state.config.always_show_link_dialog {
		Some(links[0].url.clone())
	} else {
		dialogs::show_link_selection_dialog(frame, &links)
	};
	if let Some(url) = url_to_open {
		live_region.announce("Opening link");
		let _ = launch_default_browser(&url, BrowserLaunchFlags::Default);
	}
}

pub(super) fn play_media(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	let frame = ctx.frame;
	let live_region = ctx.live_region;
	let Some(status) = get_selected_status(state) else {
		live_region.announce("No post selected");
		return;
	};
	let target = status.reblog.as_ref().map_or(status, std::convert::AsRef::as_ref);

	if target.media_attachments.is_empty() {
		live_region.announce("No media attached to this post");
		return;
	}

	let media = if target.media_attachments.len() == 1 {
		&target.media_attachments[0]
	} else {
		let options: Vec<String> = target
			.media_attachments
			.iter()
			.enumerate()
			.map(|(i, m)| {
				let name = format!("{} {}", m.kind, i + 1);
				if let Some(desc) = &m.description {
					if !desc.is_empty() {
						return format!("{} - {}", name, desc);
					}
				}
				name
			})
			.collect();

		// SingleChoiceDialog might require &[&str], so map to it just in case
		let options_refs: Vec<&str> = options.iter().map(AsRef::as_ref).collect();

		let dialog = SingleChoiceDialog::builder(frame, "Select media to play", "Play Media", &options_refs).build();

		if dialog.show_modal() == ID_OK {
			let selection = dialog.get_selection();
			if let Ok(idx) = usize::try_from(selection)
				&& idx < target.media_attachments.len()
			{
				&target.media_attachments[idx]
			} else {
				return;
			}
		} else {
			return;
		}
	};

	crate::ui::dialogs::show_media_player(frame, media.url.clone(), state.access_token.clone());
}

pub(super) fn view_in_browser(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	let frame = ctx.frame;
	let live_region = ctx.live_region;
	let Some(status) = get_selected_status(state) else {
		live_region.announce("No post selected");
		return;
	};
	let target = status.reblog.as_ref().map_or(status, std::convert::AsRef::as_ref);

	let mut options = Vec::new();
	let mut urls = Vec::new();

	if let Some(url) = &target.url {
		options.push("Original Post");
		urls.push(url.clone());
	}

	if let Some(quote) = &target.quote
		&& let Some(quoted_status) = &quote.quoted_status
		&& let Some(quote_url) = &quoted_status.url
	{
		options.push("Quoted Post");
		urls.push(quote_url.clone());
	}

	if options.is_empty() {
		live_region.announce("Post URL not available");
		return;
	}

	let url_to_open = if options.len() == 1 {
		Some(urls[0].clone())
	} else {
		let dialog =
			SingleChoiceDialog::builder(frame, "Which post do you want to open?", "View in Browser", &options).build();

		if dialog.show_modal() == ID_OK {
			let selection = dialog.get_selection();
			if let Ok(idx) = usize::try_from(selection)
				&& idx < urls.len()
			{
				Some(urls[idx].clone())
			} else {
				None
			}
		} else {
			None
		}
	};

	if let Some(url) = url_to_open {
		live_region.announce("Opening post in browser");
		let _ = launch_default_browser(&url, BrowserLaunchFlags::Default);
	}
}

pub(super) fn vote(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	let frame = ctx.frame;
	let live_region = ctx.live_region;
	let Some(status) = get_selected_status(state) else {
		live_region.announce("No post selected");
		return;
	};
	let target = status.reblog.as_ref().map_or(status, std::convert::AsRef::as_ref);
	let Some(poll) = &target.poll else {
		live_region.announce("No poll in this post");
		return;
	};
	let post_text = target.display_text();
	if let Some(choices) = dialogs::prompt_for_vote(frame, poll, &post_text) {
		if let Some(handle) = &state.network_handle {
			let is_foreign = matches!(
				state.timeline_manager.active().map(|t| &t.timeline_type),
				Some(TimelineType::InstanceLocal { .. })
			);
			if is_foreign {
				if let Some(url) = &target.url {
					handle.send(NetworkCommand::ResolveAndInteract {
						url: url.clone(),
						interaction: ForeignInteraction::Vote(choices),
					});
					return;
				}
			}
			handle.send(NetworkCommand::VotePoll { poll_id: poll.id.clone(), choices });
		} else {
			live_region.announce("Network not available");
		}
	}
}

pub(super) fn view_post(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	let frame = ctx.frame;
	let live_region = ctx.live_region;
	let Some(status) = get_selected_status(state) else {
		live_region.announce("No post selected");
		return;
	};
	let target = status.reblog.as_ref().map_or(status, std::convert::AsRef::as_ref);
	if let Some(next_cmd) = crate::ui::dialogs::show_post_view_dialog(frame, target) {
		handle_ui_command(next_cmd, ctx);
	}
}

pub(super) fn recover_draft(ctx: &mut UiCommandContext<'_>) {
	let state = &mut *ctx.state;
	let Some(pending) = state.pending_post.take() else { return };
	let mut config = pending.config;
	config.initial_content = pending.last_result.content;
	config.initial_cw = pending.last_result.spoiler_text;
	config.initial_sensitive = pending.last_result.sensitive;
	config.initial_language = pending.last_result.language;
	config.default_visibility = pending.last_result.visibility;
	config.initial_thread_mode = pending.last_result.continue_thread;

	let Some((new_post, new_config)) = dialogs::prompt_for_compose(
		ctx.frame,
		state.max_post_chars,
		&state.poll_limits,
		state.config.enter_to_send,
		config,
		pending.last_result.media,
		pending.last_result.poll,
	) else {
		return;
	};

	let quoted_id = match &pending.operation {
		crate::PostOperation::Quote { quoted_status_id } => Some(quoted_status_id.clone()),
		_ => None,
	};

	let post_data = post_result_to_data(new_post.clone(), quoted_id);

	let cmd = match pending.operation {
		crate::PostOperation::NewPost => NetworkCommand::PostStatus { post: post_data },
		crate::PostOperation::Reply { ref in_reply_to_id } => NetworkCommand::Reply {
			in_reply_to_id: in_reply_to_id.clone(),
			content: post_data.content,
			visibility: post_data.visibility,
			sensitive: post_data.sensitive,
			spoiler_text: post_data.spoiler_text,
			content_type: post_data.content_type,
			language: post_data.language,
			media: post_data.media,
			poll: post_data.poll,
			scheduled_at: post_data.scheduled_at,
		},
		crate::PostOperation::Edit { ref status_id } => {
			let media = new_post
				.media
				.clone()
				.into_iter()
				.map(|item| {
					if item.is_existing {
						network::EditMedia::Existing(item.path)
					} else {
						network::EditMedia::New(network::MediaUpload { path: item.path, description: item.description })
					}
				})
				.collect();
			NetworkCommand::EditStatus {
				status_id: status_id.clone(),
				content: post_data.content,
				sensitive: post_data.sensitive,
				spoiler_text: post_data.spoiler_text,
				language: post_data.language,
				media,
				poll: post_data.poll,
			}
		}
		crate::PostOperation::Quote { .. } => NetworkCommand::PostStatus { post: post_data },
	};

	state.pending_thread_continuation = new_post.continue_thread;
	state.pending_post =
		Some(crate::PendingPost { config: new_config, operation: pending.operation, last_result: new_post });

	if let Some(handle) = &state.network_handle {
		handle.send(cmd);
	} else {
		ctx.timeline_list.announce("Network not available");
	}
}
