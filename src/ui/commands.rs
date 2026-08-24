//! The UI command queue: every user action the app supports becomes a [`UiCommand`].

// Every handler below takes the context by `&mut` and receives the command's payload by
// value, mirroring the `UiCommand` variant it came from, so the dispatch table stays uniform
// even where an individual handler only reads what it is given.
#[allow(clippy::needless_pass_by_ref_mut, clippy::needless_pass_by_value)]
mod account;
#[allow(clippy::needless_pass_by_ref_mut, clippy::needless_pass_by_value)]
mod app;
#[allow(clippy::needless_pass_by_ref_mut, clippy::needless_pass_by_value)]
mod find;
#[allow(clippy::needless_pass_by_ref_mut, clippy::needless_pass_by_value)]
mod post;
mod selection;
#[allow(clippy::needless_pass_by_ref_mut, clippy::needless_pass_by_value)]
mod settings;
#[allow(clippy::needless_pass_by_ref_mut, clippy::needless_pass_by_value)]
mod timeline;
#[allow(clippy::needless_pass_by_ref_mut, clippy::needless_pass_by_value)]
mod user;

use std::cell::Cell;

pub use post::run_edit_post_dialog;
pub use selection::get_selected_status;
use url::Url;
use wxdragon::prelude::*;

use crate::{
	AppState, auth,
	config::{AutoloadMode, SortOrder},
	mastodon::Status,
	timeline::TimelineType,
	ui_wake::UiCommandSender,
};

/// Commands that can be triggered by UI events.
pub enum UiCommand {
	NewPost,
	Reply { reply_all: bool },
	Quote,
	DeletePost,
	EditPost,
	CopyPost,
	CopyPostLink,
	Favorite,
	Bookmark,
	Boost,
	Pin,
	Refresh,
	OpenTimeline(TimelineType),
	SentTimeline,
	OpenUserTimeline,
	OpenUserTimelineByInput,
	OpenInstanceTimelineByInput,
	CloseTimeline,
	TimelineSelectionChanged(usize),
	TimelineEntrySelectionChanged(usize),
	ShowOptions,
	CustomizeShortcuts,
	ManageAccounts,
	SwitchAccount(String),
	SwitchNextAccount,
	SwitchPrevAccount,
	SwitchNextTimeline,
	SwitchPrevTimeline,
	MoveTimelineLeft,
	MoveTimelineRight,
	RemoveAccount(String),
	ViewProfile,
	ViewMentions,
	ViewHashtags,
	ViewBoosts,
	ViewFavorites,
	HashtagDialogClosed,
	ProfileDialogClosed,
	FollowersDialogClosed,
	FollowingDialogClosed,
	OpenLinks,
	ViewInBrowser,
	PlayMedia,
	ViewThread,
	ViewResolvedThread(Box<Status>),
	PromptForQuote(Box<Status>),
	ViewQuotedThread,
	Vote,
	LoadMore,
	LoadMoreBackground,

	HomePressed,
	ToggleContentWarning,
	ToggleFollow,
	ToggleWindowVisibility,
	SetQuickActionKeysEnabled(bool),
	SwitchTimelineByIndex(usize),
	OAuthResult { result: Result<auth::OAuthResult, String>, instance_url: Url },
	CancelAuth,
	EditProfile,
	ViewHelp,
	ViewPost,
	Search,
	CheckForUpdates,
	ManageFilters,
	ManageLists,
	ManageListsDialogClosed,
	ManageListMembersDialogClosed,
	OpenList,
	AddUserToList(String),
	ContinueThread(Box<Status>),
	Find(String),
	FindNext,
	FindPrev,
	AppClosing,
	ExitApp,
	RecoverDraft,
	PollNonStreaming,
}

/// Handles a UI command, updating state and UI as needed.
pub struct UiCommandContext<'a> {
	pub state: &'a mut AppState,
	pub frame: &'a Frame,
	pub timelines_selector: ListBox,
	pub timeline_list: crate::ui::timeline_list::TimelineList,
	pub suppress_selection: &'a Cell<bool>,
	pub live_region: &'a crate::ui::timeline_list::TimelineList,
	pub quick_action_keys_enabled: &'a Cell<bool>,
	pub autoload_mode: &'a Cell<AutoloadMode>,
	pub sort_order_cell: &'a Cell<SortOrder>,
	pub tray_hidden: &'a Cell<bool>,
	pub shortcuts_cell: &'a std::cell::RefCell<crate::config::ShortcutsConfig>,
	pub ui_tx: &'a UiCommandSender,
}

/// Handles a UI command, updating state and UI as needed.
pub fn handle_ui_command(cmd: UiCommand, ctx: &mut UiCommandContext<'_>) {
	match cmd {
		UiCommand::NewPost => post::new_post(ctx),
		UiCommand::ContinueThread(status) => post::continue_thread(ctx, status),
		UiCommand::Reply { reply_all } => post::reply(ctx, reply_all),
		UiCommand::Quote => post::quote(ctx),
		UiCommand::PromptForQuote(target) => post::prompt_for_quote(ctx, target),
		UiCommand::DeletePost => post::delete_post(ctx),
		UiCommand::EditPost => post::edit_post(ctx),
		UiCommand::CopyPost => post::copy_post(ctx),
		UiCommand::CopyPostLink => post::copy_post_link(ctx),
		UiCommand::Favorite => post::favorite(ctx),
		UiCommand::Bookmark => post::bookmark(ctx),
		UiCommand::Boost => post::boost(ctx),
		UiCommand::Pin => post::pin(ctx),
		UiCommand::Refresh => timeline::refresh(ctx),
		UiCommand::PollNonStreaming => timeline::poll_non_streaming(ctx),
		UiCommand::OpenTimeline(timeline_type) => timeline::open(ctx, timeline_type),
		UiCommand::SentTimeline => timeline::sent_timeline(ctx),
		UiCommand::CloseTimeline => timeline::close(ctx),
		UiCommand::LoadMoreBackground => timeline::load_more_background(ctx),
		UiCommand::HomePressed => timeline::home_pressed(ctx),
		UiCommand::LoadMore => timeline::load_more(ctx),
		UiCommand::ToggleContentWarning => post::toggle_content_warning(ctx),
		UiCommand::ToggleWindowVisibility => app::toggle_window_visibility(ctx),
		UiCommand::SetQuickActionKeysEnabled(enabled) => app::set_quick_action_keys_enabled(ctx, enabled),
		UiCommand::SwitchTimelineByIndex(index) => timeline::switch_timeline_by_index(ctx, index),
		UiCommand::TimelineSelectionChanged(index) => timeline::timeline_selection_changed(ctx, index),
		UiCommand::TimelineEntrySelectionChanged(index) => timeline::timeline_entry_selection_changed(ctx, index),
		UiCommand::ShowOptions => settings::show_options(ctx),
		UiCommand::CustomizeShortcuts => settings::customize_shortcuts(ctx),
		UiCommand::ManageAccounts => account::manage_accounts(ctx),
		UiCommand::SwitchAccount(id) => account::switch_account(ctx, id),
		UiCommand::SwitchNextAccount => account::switch_next_account(ctx),
		UiCommand::SwitchPrevAccount => account::switch_prev_account(ctx),
		UiCommand::SwitchNextTimeline => timeline::switch_next_timeline(ctx),
		UiCommand::SwitchPrevTimeline => timeline::switch_prev_timeline(ctx),
		UiCommand::MoveTimelineLeft => timeline::move_timeline_left(ctx),
		UiCommand::MoveTimelineRight => timeline::move_timeline_right(ctx),
		UiCommand::RemoveAccount(id) => account::remove_account(ctx, id),
		UiCommand::OAuthResult { result, instance_url } => account::oauth_result(ctx, result, instance_url),
		UiCommand::CancelAuth => account::cancel_auth(ctx),
		UiCommand::ViewProfile => user::view_profile(ctx),
		UiCommand::OpenUserTimeline => user::open_user_timeline(ctx),
		UiCommand::OpenUserTimelineByInput => user::open_user_timeline_by_input(ctx),
		UiCommand::OpenInstanceTimelineByInput => timeline::open_instance_timeline_by_input(ctx),
		UiCommand::ViewMentions => user::view_mentions(ctx),
		UiCommand::ViewHashtags => user::view_hashtags(ctx),
		UiCommand::ViewBoosts => user::view_boosts(ctx),
		UiCommand::ViewFavorites => user::view_favorites(ctx),
		UiCommand::HashtagDialogClosed => user::hashtag_dialog_closed(ctx),
		UiCommand::ProfileDialogClosed => user::profile_dialog_closed(ctx),
		UiCommand::FollowersDialogClosed => user::followers_dialog_closed(ctx),
		UiCommand::FollowingDialogClosed => user::following_dialog_closed(ctx),
		UiCommand::OpenLinks => post::open_links(ctx),
		UiCommand::ToggleFollow => user::toggle_follow(ctx),
		UiCommand::PlayMedia => post::play_media(ctx),
		UiCommand::ViewInBrowser => post::view_in_browser(ctx),
		UiCommand::ViewThread => timeline::view_thread(ctx),
		UiCommand::ViewResolvedThread(focus) => timeline::view_resolved_thread(ctx, focus),
		UiCommand::ViewQuotedThread => timeline::view_quoted_thread(ctx),
		UiCommand::Vote => post::vote(ctx),
		UiCommand::EditProfile => account::edit_profile(ctx),
		UiCommand::ViewHelp => app::view_help(ctx),
		UiCommand::ViewPost => post::view_post(ctx),
		UiCommand::Search => timeline::search(ctx),
		UiCommand::CheckForUpdates => app::check_for_updates(ctx),
		UiCommand::OpenList => timeline::open_list(ctx),
		UiCommand::ManageListsDialogClosed => settings::manage_lists_dialog_closed(ctx),
		UiCommand::ManageListMembersDialogClosed => settings::manage_list_members_dialog_closed(ctx),
		UiCommand::ManageFilters => settings::manage_filters(ctx),
		UiCommand::ManageLists => settings::manage_lists(ctx),
		UiCommand::AddUserToList(account_id) => user::add_user_to_list(ctx, account_id),
		UiCommand::Find(query) => find::find(ctx, query),
		UiCommand::FindNext => find::find_next(ctx),
		UiCommand::FindPrev => find::find_prev(ctx),
		UiCommand::AppClosing => app::app_closing(ctx),
		UiCommand::ExitApp => app::exit_app(ctx),
		UiCommand::RecoverDraft => post::recover_draft(ctx),
	}
}
