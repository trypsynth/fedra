//! The UI command queue: every user action the app supports becomes a [`UiCommand`].

// Every handler below takes the context by `&mut` and receives the command's payload by
// value, mirroring the `UiCommand` variant it came from, so the dispatch table stays uniform
// even where an individual handler only reads what it is given.
#[allow(clippy::needless_pass_by_ref_mut, clippy::needless_pass_by_value)]
mod account;
#[allow(clippy::needless_pass_by_ref_mut, clippy::needless_pass_by_value)]
mod app;
pub mod deferred;
#[allow(clippy::needless_pass_by_ref_mut, clippy::needless_pass_by_value)]
mod find;
#[allow(clippy::needless_pass_by_ref_mut, clippy::needless_pass_by_value)]
mod global;
#[allow(clippy::needless_pass_by_ref_mut, clippy::needless_pass_by_value)]
mod post;
mod selection;
#[allow(clippy::needless_pass_by_ref_mut, clippy::needless_pass_by_value)]
mod settings;
#[allow(clippy::needless_pass_by_ref_mut, clippy::needless_pass_by_value)]
mod timeline;
#[allow(clippy::needless_pass_by_ref_mut, clippy::needless_pass_by_value)]
mod user;

use std::{
	cell::{Cell, RefCell},
	future::pending,
	rc::{Rc, Weak},
};

pub use post::run_edit_post_dialog;
pub use selection::{get_selected_entry, get_selected_status};
pub use settings::register_hotkeys;
use url::Url;
use wx_utils::tasks::{sleep, spawn_for};
use wxdragon::prelude::*;

use crate::{
	AppState, STATE_BUSY_RETRY, auth,
	config::{ActionId, AutoloadMode, ShortcutsConfig, SortOrder},
	mastodon::{Account, MastodonClient, Status},
	network::Session,
	responses::open_account,
	timeline::TimelineType,
	ui::{
		dialogs::{FollowList, UserLookupAction},
		timeline_list::TimelineList,
	},
	ui_wake::UiCommandSender,
};

/// Commands that can be triggered by UI events.
pub enum UiCommand {
	NewPost,
	Reply {
		reply_all: bool,
	},
	Quote,
	DeletePost,
	EditPost,
	CopyPost,
	CopyPostLink,
	Favorite,
	Bookmark,
	Boost,
	MuteConversation,
	ViewEditHistory,
	ScheduledPosts,
	Pin,
	Refresh,
	OpenTimeline(TimelineType),
	SentTimeline,
	OpenUserTimeline,
	OpenUserTimelineByInput,
	OpenInstanceTimelineByInput,
	CloseTimeline,
	TogglePermanentTimeline,
	ToggleTimelineNotifications,
	SetTimelineSound,
	ResetTimelineSound,
	ClearTimeline,
	ClearAllTimelines,
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
	FollowListClosed(FollowList),
	/// The user's own followers or follows.
	ShowOwnFollowList(FollowList),
	/// `account` is whose followers or follows to show, and `None` for the user's own mutes and blocks.
	ShowFollowList {
		account: Option<Box<Account>>,
		list: FollowList,
	},
	OpenAccount {
		account: Box<Account>,
		action: UserLookupAction,
	},
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

	ToggleContentWarning,
	ToggleFollow,
	// Sent by the tray icon, which isn't installed on macOS.
	#[cfg_attr(target_os = "macos", allow(dead_code))]
	ToggleWindowVisibility,
	ShowWindow,
	Global(crate::config::GlobalAction),
	SetQuickActionKeysEnabled(bool),
	SwitchTimelineByIndex(usize),
	OAuthResult {
		result: Result<auth::OAuthResult, String>,
		instance_url: Url,
	},
	CancelAuth,
	EditProfile,
	ViewHelp,
	ViewPost,
	Search,
	CheckForUpdates,
	ManageFilters,
	MessageRequests,
	ManageLists,
	ManageListsDialogClosed,
	ManageListMembersDialogClosed,
	OpenList,
	AddUserToList(String),
	RemoveUserFromList(String),
	SendDirectMessage(Box<Account>),
	SetUserAlias(Box<Account>),
	ContinueThread(Box<Status>),
	Find(String),
	FindNext,
	FindPrev,
	AppClosing,
	ExitApp,
	RecoverDraft,
	PollStreamable,
}

impl UiCommand {
	/// Just the variant, for the log: some commands carry a whole post, or the tokens from signing in.
	const fn name(&self) -> &'static str {
		match self {
			Self::NewPost => "NewPost",
			Self::Reply { .. } => "Reply",
			Self::Quote => "Quote",
			Self::DeletePost => "DeletePost",
			Self::EditPost => "EditPost",
			Self::CopyPost => "CopyPost",
			Self::CopyPostLink => "CopyPostLink",
			Self::Favorite => "Favorite",
			Self::Bookmark => "Bookmark",
			Self::MuteConversation => "MuteConversation",
			Self::ViewEditHistory => "ViewEditHistory",
			Self::ScheduledPosts => "ScheduledPosts",
			Self::Boost => "Boost",
			Self::Pin => "Pin",
			Self::Refresh => "Refresh",
			Self::OpenTimeline(..) => "OpenTimeline",
			Self::SentTimeline => "SentTimeline",
			Self::OpenUserTimeline => "OpenUserTimeline",
			Self::OpenUserTimelineByInput => "OpenUserTimelineByInput",
			Self::OpenInstanceTimelineByInput => "OpenInstanceTimelineByInput",
			Self::CloseTimeline => "CloseTimeline",
			Self::TogglePermanentTimeline => "TogglePermanentTimeline",
			Self::ToggleTimelineNotifications => "ToggleTimelineNotifications",
			Self::SetTimelineSound => "SetTimelineSound",
			Self::ResetTimelineSound => "ResetTimelineSound",
			Self::ClearTimeline => "ClearTimeline",
			Self::ClearAllTimelines => "ClearAllTimelines",
			Self::TimelineSelectionChanged(..) => "TimelineSelectionChanged",
			Self::TimelineEntrySelectionChanged(..) => "TimelineEntrySelectionChanged",
			Self::ShowOptions => "ShowOptions",
			Self::CustomizeShortcuts => "CustomizeShortcuts",
			Self::ManageAccounts => "ManageAccounts",
			Self::SwitchAccount(..) => "SwitchAccount",
			Self::SwitchNextAccount => "SwitchNextAccount",
			Self::SwitchPrevAccount => "SwitchPrevAccount",
			Self::SwitchNextTimeline => "SwitchNextTimeline",
			Self::SwitchPrevTimeline => "SwitchPrevTimeline",
			Self::MoveTimelineLeft => "MoveTimelineLeft",
			Self::MoveTimelineRight => "MoveTimelineRight",
			Self::RemoveAccount(..) => "RemoveAccount",
			Self::ViewProfile => "ViewProfile",
			Self::ViewMentions => "ViewMentions",
			Self::ViewHashtags => "ViewHashtags",
			Self::ViewBoosts => "ViewBoosts",
			Self::ViewFavorites => "ViewFavorites",
			Self::HashtagDialogClosed => "HashtagDialogClosed",
			Self::ProfileDialogClosed => "ProfileDialogClosed",
			Self::FollowListClosed(..) => "FollowListClosed",
			Self::ShowOwnFollowList(..) => "ShowOwnFollowList",
			Self::ShowFollowList { .. } => "ShowFollowList",
			Self::OpenAccount { .. } => "OpenAccount",
			Self::OpenLinks => "OpenLinks",
			Self::ViewInBrowser => "ViewInBrowser",
			Self::PlayMedia => "PlayMedia",
			Self::ViewThread => "ViewThread",
			Self::ViewResolvedThread(..) => "ViewResolvedThread",
			Self::PromptForQuote(..) => "PromptForQuote",
			Self::ViewQuotedThread => "ViewQuotedThread",
			Self::Vote => "Vote",
			Self::LoadMore => "LoadMore",
			Self::LoadMoreBackground => "LoadMoreBackground",
			Self::ToggleContentWarning => "ToggleContentWarning",
			Self::ToggleFollow => "ToggleFollow",
			Self::ToggleWindowVisibility => "ToggleWindowVisibility",
			Self::ShowWindow => "ShowWindow",
			Self::Global(..) => "Global",
			Self::SetQuickActionKeysEnabled(..) => "SetQuickActionKeysEnabled",
			Self::SwitchTimelineByIndex(..) => "SwitchTimelineByIndex",
			Self::OAuthResult { .. } => "OAuthResult",
			Self::CancelAuth => "CancelAuth",
			Self::EditProfile => "EditProfile",
			Self::ViewHelp => "ViewHelp",
			Self::ViewPost => "ViewPost",
			Self::Search => "Search",
			Self::CheckForUpdates => "CheckForUpdates",
			Self::ManageFilters => "ManageFilters",
			Self::MessageRequests => "MessageRequests",
			Self::ManageLists => "ManageLists",
			Self::ManageListsDialogClosed => "ManageListsDialogClosed",
			Self::ManageListMembersDialogClosed => "ManageListMembersDialogClosed",
			Self::OpenList => "OpenList",
			Self::AddUserToList(..) => "AddUserToList",
			Self::RemoveUserFromList(..) => "RemoveUserFromList",
			Self::SendDirectMessage(..) => "SendDirectMessage",
			Self::SetUserAlias(..) => "SetUserAlias",
			Self::ContinueThread(..) => "ContinueThread",
			Self::Find(..) => "Find",
			Self::FindNext => "FindNext",
			Self::FindPrev => "FindPrev",
			Self::AppClosing => "AppClosing",
			Self::ExitApp => "ExitApp",
			Self::RecoverDraft => "RecoverDraft",
			Self::PollStreamable => "PollStreamable",
		}
	}
}

/// Handles a UI command, updating state and UI as needed.
/// The command a shortcut action runs. `None` for the two that need more than a command: Find
/// asks for a query first, and toggling quick action keys changes state the key handler owns.
pub const fn command_for(action: ActionId) -> Option<UiCommand> {
	Some(match action {
		ActionId::NewPost => UiCommand::NewPost,
		ActionId::Reply => UiCommand::Reply { reply_all: true },
		ActionId::ReplyAuthor => UiCommand::Reply { reply_all: false },
		ActionId::Quote => UiCommand::Quote,
		ActionId::ToggleFollow => UiCommand::ToggleFollow,
		ActionId::ViewProfile => UiCommand::ViewProfile,
		ActionId::ViewMentions => UiCommand::ViewMentions,
		ActionId::ViewHashtags => UiCommand::ViewHashtags,
		ActionId::OpenLinks => UiCommand::OpenLinks,
		ActionId::PlayMedia => UiCommand::PlayMedia,
		ActionId::ViewInBrowser => UiCommand::ViewInBrowser,
		ActionId::CopyPost => UiCommand::CopyPost,
		ActionId::CopyPostLink => UiCommand::CopyPostLink,
		ActionId::ViewPost => UiCommand::ViewPost,
		ActionId::ViewThread => UiCommand::ViewThread,
		ActionId::ViewQuotedThread => UiCommand::ViewQuotedThread,
		ActionId::EditPost => UiCommand::EditPost,
		ActionId::DeletePost => UiCommand::DeletePost,
		ActionId::PinPost => UiCommand::Pin,
		ActionId::Vote => UiCommand::Vote,
		ActionId::Favorite => UiCommand::Favorite,
		ActionId::Bookmark => UiCommand::Bookmark,
		ActionId::MuteConversation => UiCommand::MuteConversation,
		ActionId::ViewEditHistory => UiCommand::ViewEditHistory,
		ActionId::ScheduledPosts => UiCommand::ScheduledPosts,
		ActionId::Boost => UiCommand::Boost,
		ActionId::ViewBoosts => UiCommand::ViewBoosts,
		ActionId::ViewFavorites => UiCommand::ViewFavorites,
		ActionId::OpenUserTimeline => UiCommand::OpenUserTimeline,
		ActionId::OpenUserTimelineByInput => UiCommand::OpenUserTimelineByInput,
		ActionId::Search => UiCommand::Search,
		ActionId::FindNext => UiCommand::FindNext,
		ActionId::FindPrev => UiCommand::FindPrev,
		ActionId::HomeTimeline => UiCommand::OpenTimeline(TimelineType::Home),
		ActionId::NotificationsTimeline => UiCommand::OpenTimeline(TimelineType::Notifications),
		ActionId::SentTimeline => UiCommand::SentTimeline,
		ActionId::LocalTimeline => UiCommand::OpenTimeline(TimelineType::Local),
		ActionId::OpenInstanceTimelineByInput => UiCommand::OpenInstanceTimelineByInput,
		ActionId::FederatedTimeline => UiCommand::OpenTimeline(TimelineType::Federated),
		ActionId::DirectTimeline => UiCommand::OpenTimeline(TimelineType::Direct),
		ActionId::MentionsTimeline => UiCommand::OpenTimeline(TimelineType::Mentions),
		ActionId::BookmarksTimeline => UiCommand::OpenTimeline(TimelineType::Bookmarks),
		ActionId::FavoritesTimeline => UiCommand::OpenTimeline(TimelineType::Favorites),
		ActionId::OpenList => UiCommand::OpenList,
		ActionId::LoadMore => UiCommand::LoadMore,
		ActionId::CloseTimeline => UiCommand::CloseTimeline,
		ActionId::TogglePermanentTimeline => UiCommand::TogglePermanentTimeline,
		ActionId::ToggleTimelineNotifications => UiCommand::ToggleTimelineNotifications,
		ActionId::SetTimelineSound => UiCommand::SetTimelineSound,
		ActionId::ResetTimelineSound => UiCommand::ResetTimelineSound,
		ActionId::ClearTimeline => UiCommand::ClearTimeline,
		ActionId::ClearAllTimelines => UiCommand::ClearAllTimelines,
		ActionId::Refresh => UiCommand::Refresh,
		ActionId::SwitchPrevTimeline => UiCommand::SwitchPrevTimeline,
		ActionId::SwitchNextTimeline => UiCommand::SwitchNextTimeline,
		ActionId::MoveTimelineLeft => UiCommand::MoveTimelineLeft,
		ActionId::MoveTimelineRight => UiCommand::MoveTimelineRight,
		ActionId::SwitchPrevAccount => UiCommand::SwitchPrevAccount,
		ActionId::SwitchNextAccount => UiCommand::SwitchNextAccount,
		ActionId::ToggleContentWarning => UiCommand::ToggleContentWarning,
		ActionId::ManageAccounts => UiCommand::ManageAccounts,
		ActionId::ManageFilters => UiCommand::ManageFilters,
		ActionId::MessageRequests => UiCommand::MessageRequests,
		ActionId::ManageLists => UiCommand::ManageLists,
		ActionId::YourFollowers => UiCommand::ShowOwnFollowList(FollowList::Followers),
		ActionId::YourFollowing => UiCommand::ShowOwnFollowList(FollowList::Following),
		ActionId::MutedUsers => UiCommand::ShowFollowList { account: None, list: FollowList::Muted },
		ActionId::BlockedUsers => UiCommand::ShowFollowList { account: None, list: FollowList::Blocked },
		ActionId::EditProfile => UiCommand::EditProfile,
		ActionId::Options => UiCommand::ShowOptions,
		ActionId::CustomizeShortcuts => UiCommand::CustomizeShortcuts,
		ActionId::CheckForUpdates => UiCommand::CheckForUpdates,
		ActionId::ViewHelp => UiCommand::ViewHelp,
		ActionId::Find | ActionId::ToggleQuickActionKeys => return None,
	})
}

pub struct UiCommandContext<'a> {
	pub state: &'a mut AppState,
	pub frame: &'a Frame,
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
	/// What this context was made from, for UI tasks that need one of their own later.
	pub handles: &'a UiHandles,
}

impl UiCommandContext<'_> {
	/// What a dialog keeps so it can make requests for the active account, if there is one.
	pub fn requests(&self) -> Option<Requests> {
		Some(Requests::new(self.handles.clone(), self.state.session()?))
	}
}

/// What a dialog keeps to make requests for the account it was opened for.
#[derive(Clone)]
pub struct Requests {
	handles: UiHandles,
	session: Session,
}

impl Requests {
	pub const fn new(handles: UiHandles, session: Session) -> Self {
		Self { handles, session }
	}

	/// See [`UiHandles::request`].
	pub fn request<T, F, G>(&self, request: F, then: G)
	where
		F: FnOnce(&MastodonClient, &str) -> T + Send + 'static,
		T: Send + 'static,
		G: FnOnce(&mut UiCommandContext<'_>, T) + 'static,
	{
		self.handles.request(&self.session, request, then);
	}
}

/// Everything a `UiCommandContext` is made from, which UI tasks keep so they can make one.
#[derive(Clone)]
pub struct UiHandles {
	/// Weak, since the state keeps a copy of these handles.
	pub state: Weak<RefCell<AppState>>,
	pub frame: Frame,
	pub timelines_selector: ListBox,
	pub timeline_list: TimelineList,
	pub suppress_selection: Rc<Cell<bool>>,
	pub quick_action_keys_enabled: Rc<Cell<bool>>,
	pub autoload_mode: Rc<Cell<AutoloadMode>>,
	pub sort_order_cell: Rc<Cell<SortOrder>>,
	pub tray_hidden: Rc<Cell<bool>>,
	pub shortcuts_cell: Rc<RefCell<ShortcutsConfig>>,
	pub ui_tx: UiCommandSender,
	pub is_shutting_down: Rc<Cell<bool>>,
}

impl UiHandles {
	pub fn context<'a>(&'a self, state: &'a mut AppState) -> UiCommandContext<'a> {
		UiCommandContext {
			state,
			frame: &self.frame,
			timelines_selector: self.timelines_selector,
			timeline_list: self.timeline_list.clone(),
			suppress_selection: &self.suppress_selection,
			live_region: &self.timeline_list,
			quick_action_keys_enabled: &self.quick_action_keys_enabled,
			autoload_mode: &self.autoload_mode,
			sort_order_cell: &self.sort_order_cell,
			tray_hidden: &self.tray_hidden,
			shortcuts_cell: &self.shortcuts_cell,
			ui_tx: &self.ui_tx,
			handles: self,
		}
	}

	/// Queues `request` behind the account's other requests, then hands its answer to `then` along
	/// with a context. An answer that arrives after the user has switched accounts is dropped,
	/// since it belongs to an account that's no longer shown.
	pub fn request<T, F, G>(&self, session: &Session, request: F, then: G)
	where
		F: FnOnce(&MastodonClient, &str) -> T + Send + 'static,
		T: Send + 'static,
		G: FnOnce(&mut UiCommandContext<'_>, T) + 'static,
	{
		let answer = session.queued(request);
		let (handles, session) = (self.clone(), session.clone());
		spawn_for(&self.frame, async move {
			let answer = answer.await;
			handles
				.with_ctx(|ctx| {
					if ctx.state.session.as_ref().is_some_and(|current| current.is(&session)) {
						then(ctx, answer);
					}
				})
				.await;
		});
	}

	/// Runs `f` with a context once nothing else holds the state, such as a handler with a modal
	/// dialog open. Waiting for it, rather than borrowing outright, is what keeps that from
	/// panicking. Once the app is closing, `f` never runs, and the task waits to be cancelled
	/// along with the main window.
	pub async fn with_ctx<R>(&self, f: impl FnOnce(&mut UiCommandContext<'_>) -> R) -> R {
		loop {
			let Some(state) = self.state.upgrade().filter(|_| !self.is_shutting_down.get()) else {
				return pending().await;
			};
			if let Ok(mut state) = state.try_borrow_mut() {
				return f(&mut self.context(&mut state));
			}
			sleep(STATE_BUSY_RETRY).await;
		}
	}
}

/// Handles a UI command, updating state and UI as needed.
pub fn handle_ui_command(cmd: UiCommand, ctx: &mut UiCommandContext<'_>) {
	// These come with every timer tick and arrow key, so they'd bury the rest at the debug level.
	if matches!(
		cmd,
		UiCommand::PollStreamable
			| UiCommand::TimelineEntrySelectionChanged(_)
			| UiCommand::TimelineSelectionChanged(_)
	) {
		tracing::trace!(command = cmd.name(), "handling");
	} else {
		tracing::debug!(command = cmd.name(), "handling");
	}
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
		UiCommand::MuteConversation => post::mute_conversation(ctx),
		UiCommand::ViewEditHistory => post::view_edit_history(ctx),
		UiCommand::ScheduledPosts => post::scheduled_posts(ctx),
		UiCommand::Boost => post::boost(ctx),
		UiCommand::Pin => post::pin(ctx),
		UiCommand::Refresh => timeline::refresh(ctx),
		UiCommand::PollStreamable => timeline::poll_streamable(ctx),
		UiCommand::OpenTimeline(timeline_type) => timeline::open(ctx, timeline_type),
		UiCommand::SentTimeline => timeline::sent_timeline(ctx),
		UiCommand::CloseTimeline => timeline::close(ctx),
		UiCommand::TogglePermanentTimeline => timeline::toggle_permanent(ctx),
		UiCommand::ToggleTimelineNotifications => timeline::toggle_notifications(ctx),
		UiCommand::SetTimelineSound => timeline::set_sound(ctx),
		UiCommand::ResetTimelineSound => timeline::reset_sound(ctx),
		UiCommand::ClearTimeline => timeline::clear(ctx, false),
		UiCommand::ClearAllTimelines => timeline::clear(ctx, true),
		UiCommand::LoadMoreBackground => timeline::load_more_background(ctx),
		UiCommand::LoadMore => timeline::load_more(ctx),
		UiCommand::ToggleContentWarning => post::toggle_content_warning(ctx),
		UiCommand::ToggleWindowVisibility => app::toggle_window_visibility(ctx),
		UiCommand::ShowWindow => app::show_window(ctx),
		UiCommand::Global(action) => global::run(ctx, action),
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
		UiCommand::FollowListClosed(list) => user::follow_list_closed(ctx, list),
		UiCommand::ShowOwnFollowList(list) => user::show_own_follow_list(ctx, list),
		UiCommand::ShowFollowList { account, list } => user::show_follow_list(ctx, account.as_deref(), list),
		UiCommand::OpenAccount { account, action } => open_account(ctx, &account, action),
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
		UiCommand::MessageRequests => settings::message_requests(ctx),
		UiCommand::ManageLists => settings::manage_lists(ctx),
		UiCommand::AddUserToList(account_id) => user::add_user_to_list(ctx, account_id),
		UiCommand::RemoveUserFromList(account_id) => user::remove_user_from_list(ctx, account_id),
		UiCommand::SendDirectMessage(account) => user::send_direct_message(ctx, *account),
		UiCommand::SetUserAlias(account) => user::set_alias(ctx, &account),
		UiCommand::Find(query) => find::find(ctx, query),
		UiCommand::FindNext => find::find_next(ctx),
		UiCommand::FindPrev => find::find_prev(ctx),
		UiCommand::AppClosing => app::app_closing(ctx),
		UiCommand::ExitApp => app::exit_app(ctx),
		UiCommand::RecoverDraft => post::recover_draft(ctx),
	}
}
