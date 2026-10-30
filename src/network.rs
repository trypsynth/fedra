use std::{
	collections::{HashMap, HashSet},
	slice,
	sync::mpsc::{self, Receiver, Sender},
	thread::{self, JoinHandle},
};

use anyhow::{Context, Result};
use chrono::DateTime;

use crate::{
	mastodon::{
		Account, Conversation, Filter, FilterAction, FilterContext, InstanceInfo, MastodonClient, Notification,
		PostSubmission, Relationship, SearchResults, SearchType, Status, StatusAction, StatusContext,
	},
	timeline::TimelineType,
	ui_wake::UiWaker,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationshipAction {
	Follow,
	Unfollow,
	CancelFollowRequest,
	AcceptFollowRequest,
	RejectFollowRequest,
	Block,
	Unblock,
	Mute,
	Unmute,
	ShowBoosts,
	HideBoosts,
	EnableNotifications,
	DisableNotifications,
	/// Only reported when the current follow state couldn't be looked up.
	ToggleFollow,
}

#[derive(Debug, Clone)]
pub struct PostData {
	pub content: String,
	pub visibility: String,
	pub sensitive: bool,
	pub spoiler_text: Option<String>,
	pub content_type: Option<String>,
	pub language: Option<String>,
	pub media: Vec<MediaUpload>,
	pub poll: Option<PollData>,
	pub quoted_status_id: Option<String>,
	pub scheduled_at: Option<String>,
}

#[derive(Debug)]
pub enum ForeignInteraction {
	Action(StatusAction),
	Vote(Vec<usize>),
	Reply(Box<PostData>),
}

pub enum NetworkCommand {
	FetchTimeline {
		timeline_type: TimelineType,
		limit: Option<u32>,
		max_id: Option<String>,
	},
	FetchThread {
		timeline_type: TimelineType,
		focus: Box<Status>,
	},
	FetchStatusSource {
		status: Box<Status>,
	},
	ResolveStatusForThread {
		url: String,
	},
	ResolveStatusForQuote {
		url: String,
	},
	ResolveAndInteract {
		url: String,
		interaction: ForeignInteraction,
	},
	ResolveAccount {
		url: String,
	},
	LookupAccount {
		handle: String,
	},
	PostStatus {
		post: PostData,
	},
	ActOnStatus {
		status_id: String,
		action: StatusAction,
	},
	Reply {
		in_reply_to_id: String,
		content: String,
		visibility: String,
		sensitive: bool,
		spoiler_text: Option<String>,
		content_type: Option<String>,
		language: Option<String>,
		media: Vec<MediaUpload>,
		poll: Option<PollData>,
		scheduled_at: Option<String>,
	},
	FollowTag {
		name: String,
	},
	UnfollowTag {
		name: String,
	},
	FollowAccount {
		account_id: String,
		target_name: String,
		reblogs: bool,
		notify: bool,
		action: RelationshipAction,
	},
	ToggleFollow {
		account_id: Option<String>,
		acct: String,
		target_name: String,
	},
	UnfollowAccount {
		account_id: String,
		target_name: String,
		action: RelationshipAction,
	},
	AuthorizeFollowRequest {
		account_id: String,
		target_name: String,
	},
	RejectFollowRequest {
		account_id: String,
		target_name: String,
	},
	BlockAccount {
		account_id: String,
		target_name: String,
	},
	UnblockAccount {
		account_id: String,
		target_name: String,
	},
	MuteAccount {
		account_id: String,
		target_name: String,
	},
	UnmuteAccount {
		account_id: String,
		target_name: String,
	},
	FetchRelationship {
		account_id: String,
	},
	FetchRelationshipsForList {
		account_ids: Vec<String>,
		for_followers: bool,
	},
	FetchAccount {
		account_id: String,
	},
	FetchTagsInfo {
		names: Vec<String>,
	},
	FetchRebloggedBy {
		status_id: String,
	},
	FetchFavoritedBy {
		status_id: String,
	},
	FetchFollowers {
		account_id: String,
		acct: String,
		total_count: u64,
	},
	FetchFollowing {
		account_id: String,
		acct: String,
		total_count: u64,
	},
	FetchNextFollowersPage {
		account_id: String,
		max_id: String,
	},
	FetchNextFollowingPage {
		account_id: String,
		max_id: String,
	},
	VotePoll {
		poll_id: String,
		choices: Vec<usize>,
	},
	DeleteStatus {
		status_id: String,
	},
	EditStatus {
		status_id: String,
		content: String,
		sensitive: bool,
		spoiler_text: Option<String>,
		language: Option<String>,
		media: Vec<EditMedia>,
		poll: Option<PollData>,
	},
	FetchCredentials,
	UpdateProfile {
		update: ProfileUpdate,
	},
	Search {
		query: String,
		search_type: SearchType,
		limit: Option<u32>,
		offset: Option<u32>,
	},
	FetchLists,
	CreateList {
		title: String,
		policy: String,
		exclusive: bool,
	},
	UpdateList {
		id: String,
		title: String,
		policy: String,
		exclusive: bool,
	},
	DeleteList {
		id: String,
	},
	FetchListAccounts {
		list_id: String,
	},
	FetchAccountLists {
		account_id: String,
	},
	AddListAccount {
		list_id: String,
		account_id: String,
	},
	RemoveListAccount {
		list_id: String,
		account_id: String,
	},
	MuteTag {
		name: String,
	},
	UnmuteTag {
		name: String,
	},
	FetchInstanceInfo,
	FetchMarkers,
	FetchNotificationPolicy,
	SaveMarker {
		timeline: &'static str,
		last_read_id: String,
	},
	Shutdown,
}

#[derive(Debug, Clone)]
pub struct ProfileUpdate {
	pub display_name: Option<String>,
	pub note: Option<String>,
	pub avatar: Option<String>,
	pub header: Option<String>,
	pub locked: Option<bool>,
	pub bot: Option<bool>,
	pub discoverable: Option<bool>,
	pub fields_attributes: Option<Vec<(String, String)>>,
	pub source: Option<crate::mastodon::Source>,
}

#[derive(Debug, Clone)]
pub struct MediaUpload {
	pub path: String,
	pub description: Option<String>,
}

#[derive(Debug, Clone)]
pub enum EditMedia {
	New(MediaUpload),
	Existing(String),
}

#[derive(Debug, Clone)]
pub struct PollData {
	pub options: Vec<String>,
	pub expires_in: u32,
	pub multiple: bool,
	pub hide_totals: bool,
}

#[derive(Debug)]
pub enum NetworkResponse {
	TimelineLoaded {
		timeline_type: TimelineType,
		result: Result<TimelineData>,
		max_id: Option<String>,
	},
	StatusResolvedForThread {
		result: Result<Status>,
	},
	StatusResolvedForQuote {
		result: Result<Status>,
	},
	StatusSourceFetched {
		status: Box<Status>,
		result: Result<crate::mastodon::StatusSource>,
	},
	AccountLookupResult {
		handle: String,
		result: Result<Account>,
	},
	PostComplete(Result<PostSubmission>),
	/// Pairs the acted-on status id with the server's answer, which for a boost is the new wrapper.
	StatusActed {
		action: StatusAction,
		result: Result<(String, Status)>,
	},
	Replied(Result<PostSubmission>),
	StatusDeleted {
		status_id: String,
		result: Result<()>,
	},
	StatusEdited {
		_status_id: String,
		result: Result<Status>,
	},
	TagFollowed {
		name: String,
		result: Result<crate::mastodon::Tag>,
	},
	InstanceInfoFetched(Result<InstanceInfo>),
	MarkersLoaded(Result<crate::mastodon::Markers>),
	NotificationPolicyLoaded(Result<crate::mastodon::NotificationPolicy>),
	TagUnfollowed {
		name: String,
		result: Result<crate::mastodon::Tag>,
	},
	RelationshipUpdated {
		target_name: String,
		action: RelationshipAction,
		result: Result<crate::mastodon::Relationship>,
	},
	RelationshipLoaded {
		_account_id: String,
		result: Result<crate::mastodon::Relationship>,
	},
	RelationshipsForListLoaded {
		results: Vec<crate::mastodon::Relationship>,
		for_followers: bool,
	},
	AccountFetched {
		result: Result<Account>,
	},
	PollVoted {
		result: Result<crate::mastodon::Poll>,
	},
	TagsInfoFetched {
		result: Result<Vec<crate::mastodon::Tag>>,
	},
	TagMuted {
		name: String,
		result: Result<()>,
	},
	TagUnmuted {
		name: String,
		result: Result<()>,
	},
	RebloggedByLoaded {
		result: Result<Vec<Account>>,
	},
	FavoritedByLoaded {
		result: Result<Vec<Account>>,
	},
	FollowersLoaded {
		result: Result<(Vec<Account>, Option<String>)>,
		total_count: u64,
		account_id: String,
	},
	FollowingLoaded {
		result: Result<(Vec<Account>, Option<String>)>,
		total_count: u64,
		account_id: String,
	},
	FollowersNextPageLoaded {
		result: Result<(Vec<Account>, Option<String>)>,
	},
	FollowingNextPageLoaded {
		result: Result<(Vec<Account>, Option<String>)>,
	},
	CredentialsFetched {
		result: Result<Account>,
		max_profile_fields: Option<usize>,
	},
	ProfileUpdated {
		result: Result<Account>,
	},
	SearchLoaded {
		query: String,
		search_type: SearchType,
		result: Result<SearchResults>,
		offset: Option<u32>,
	},
	ListsFetched {
		result: Result<Vec<crate::mastodon::List>>,
	},
	ListCreated {
		result: Result<crate::mastodon::List>,
	},
	ListUpdated {
		result: Result<crate::mastodon::List>,
	},
	ListDeleted {
		result: Result<()>,
	},
	ListAccountsFetched {
		list_id: String,
		result: Result<Vec<Account>>,
	},
	AccountListsFetched {
		account_id: String,
		result: Result<Vec<crate::mastodon::List>>,
	},
	ListAccountAdded {
		result: Result<()>,
	},
	ListAccountRemoved {
		result: Result<()>,
	},
}

#[derive(Debug)]
pub enum TimelineData {
	Statuses(Vec<Status>, Option<String>),
	Notifications(Vec<Notification>, Option<String>),
	Conversations(Vec<Conversation>, Option<String>),
}

/// Tag mutes live in a filter Fedra owns, so unmuting never touches filters the user made.
const MUTED_TAGS_FILTER: &str = "Muted hashtags";

fn is_muted_tags_filter(filter: &Filter) -> bool {
	filter.title == MUTED_TAGS_FILTER && filter.action == FilterAction::Hide
}

fn post_with_media(
	client: &MastodonClient,
	access_token: &str,
	content: &str,
	visibility: &str,
	sensitive: bool,
	spoiler_text: Option<&str>,
	content_type: Option<&str>,
	language: Option<&str>,
	media: Vec<MediaUpload>,
	poll: Option<&PollData>,
	in_reply_to_id: Option<&str>,
	quoted_status_id: Option<&str>,
	scheduled_at: Option<&str>,
) -> Result<PostSubmission> {
	let mut media_ids = Vec::new();
	let mut upload_failed = None;
	for item in media {
		match client.upload_media(access_token, &item.path, item.description.as_deref()) {
			Ok(id) => media_ids.push(id),
			Err(err) => {
				upload_failed = Some(err);
				break;
			}
		}
	}
	if let Some(err) = upload_failed {
		return Err(err);
	}
	client.post_status_with_media(
		access_token,
		content,
		visibility,
		sensitive,
		spoiler_text,
		&media_ids,
		content_type,
		language,
		poll,
		in_reply_to_id,
		quoted_status_id,
		scheduled_at,
	)
}

fn edit_with_media(
	client: &MastodonClient,
	access_token: &str,
	status_id: &str,
	content: &str,
	sensitive: bool,
	spoiler_text: Option<&str>,
	language: Option<&str>,
	media: Vec<EditMedia>,
	poll: Option<&PollData>,
) -> Result<Status> {
	let mut media_ids = Vec::new();
	let mut upload_failed = None;
	for item in media {
		match item {
			EditMedia::New(upload) => {
				match client.upload_media(access_token, &upload.path, upload.description.as_deref()) {
					Ok(id) => media_ids.push(id),
					Err(err) => {
						upload_failed = Some(err);
						break;
					}
				}
			}
			EditMedia::Existing(id) => media_ids.push(id),
		}
	}
	if let Some(err) = upload_failed {
		return Err(err);
	}
	client.edit_status(access_token, status_id, content, sensitive, spoiler_text, language, &media_ids, poll)
}

pub struct NetworkHandle {
	pub command_tx: Sender<NetworkCommand>,
	response_rx: Receiver<NetworkResponse>,
	_thread: JoinHandle<()>,
}

impl NetworkHandle {
	pub fn send(&self, cmd: NetworkCommand) {
		let _ = self.command_tx.send(cmd);
	}

	pub fn try_recv(&self) -> Option<NetworkResponse> {
		self.response_rx.try_recv().ok()
	}

	pub fn drain(&self) -> Vec<NetworkResponse> {
		let mut responses = Vec::new();
		while let Some(resp) = self.try_recv() {
			responses.push(resp);
		}
		responses
	}

	pub fn shutdown(&self) {
		let _ = self.command_tx.send(NetworkCommand::Shutdown);
	}
}

impl Drop for NetworkHandle {
	fn drop(&mut self) {
		self.shutdown();
	}
}

/// `client` is shared with the UI thread's own uses so both reuse one connection pool.
pub fn start_network(client: MastodonClient, access_token: String, ui_waker: UiWaker) -> NetworkHandle {
	let (cmd_tx, cmd_rx) = mpsc::channel();
	let (resp_tx, resp_rx) = mpsc::channel();
	let thread = thread::spawn(move || {
		network_loop(&client, &access_token, &cmd_rx, &resp_tx, &ui_waker);
	});
	NetworkHandle { command_tx: cmd_tx, response_rx: resp_rx, _thread: thread }
}

fn send_response(responses: &Sender<NetworkResponse>, ui_waker: &UiWaker, response: NetworkResponse) {
	let _ = responses.send(response);
	ui_waker.wake();
}

fn prepare_thread_timeline(focus: Status, context: StatusContext) -> TimelineData {
	let mut statuses = context.ancestors;
	statuses.push(focus);
	statuses.extend(context.descendants);
	let mut status_map: HashMap<String, Status> = HashMap::new();
	let mut children_map: HashMap<String, Vec<String>> = HashMap::new();
	for status in statuses {
		status_map.insert(status.id.clone(), status.clone());
		if let Some(parent_id) = &status.in_reply_to_id {
			children_map.entry(parent_id.clone()).or_default().push(status.id.clone());
		}
	}
	let mut roots: Vec<String> = status_map
		.values()
		.filter(|s| s.in_reply_to_id.as_ref().is_none_or(|parent_id| !status_map.contains_key(parent_id)))
		.map(|s| s.id.clone())
		.collect();
	let sort_key = |s: &Status| {
		let time: Option<DateTime<chrono::Utc>> = s.created_at.parse().ok();
		(time, s.id.clone())
	};
	roots.sort_by(|a_id, b_id| {
		let a = &status_map[a_id];
		let b = &status_map[b_id];
		sort_key(a).cmp(&sort_key(b))
	});
	let mut sorted_statuses: Vec<Status> = Vec::new();
	let mut visited: HashSet<String> = HashSet::new();
	let mut stack: Vec<String> = roots;
	stack.reverse();
	while let Some(current_id) = stack.pop() {
		if !visited.insert(current_id.clone()) {
			continue;
		}
		if let Some(status) = status_map.get(&current_id) {
			sorted_statuses.push(status.clone());
			if let Some(children) = children_map.get_mut(&current_id) {
				children.sort_by(|a_id, b_id| {
					let a = &status_map[a_id];
					let b = &status_map[b_id];
					sort_key(a).cmp(&sort_key(b))
				});
				for child_id in children.iter().rev() {
					stack.push(child_id.clone());
				}
			}
		}
	}
	sorted_statuses.reverse();
	TimelineData::Statuses(sorted_statuses, None)
}

fn first_relationship_result(relationships: Vec<Relationship>) -> Result<Relationship> {
	relationships.into_iter().next().ok_or_else(|| anyhow::anyhow!("Relationship lookup returned no results"))
}

fn network_loop(
	client: &MastodonClient,
	access_token: &str,
	commands: &Receiver<NetworkCommand>,
	responses: &Sender<NetworkResponse>,
	ui_waker: &UiWaker,
) {
	loop {
		match commands.recv() {
			Ok(NetworkCommand::FetchTimeline { timeline_type, limit, max_id }) => {
				let mut result = match timeline_type {
					TimelineType::Notifications | TimelineType::Mentions => client
						.get_notifications(access_token, &timeline_type, limit, max_id.as_deref())
						.map(|(n, next)| TimelineData::Notifications(n, next)),
					TimelineType::Direct => client
						.get_conversations(access_token, limit, max_id.as_deref())
						.map(|(c, next)| TimelineData::Conversations(c, next)),
					TimelineType::Thread { ref id, .. } => client.get_status(access_token, id).and_then(|focus| {
						client
							.get_status_context(access_token, id)
							.map(|context| prepare_thread_timeline(focus, context))
					}),
					_ => {
						let mut statuses = Vec::new();
						if let TimelineType::User { ref id, .. } = timeline_type
							&& max_id.is_none()
							&& let Ok(mut pinned) = client.get_pinned_statuses(access_token, id)
						{
							for p in &mut pinned {
								p.pinned = true;
							}
							statuses.extend(pinned);
						}
						let res = client.get_timeline(access_token, &timeline_type, limit, max_id.as_deref());
						match res {
							Ok((s, n)) => {
								let mut seen = std::collections::HashSet::new();
								for st in &statuses {
									seen.insert(st.id.clone());
								}
								for st in s {
									if !seen.contains(&st.id) {
										seen.insert(st.id.clone());
										statuses.push(st);
									}
								}
								Ok(TimelineData::Statuses(statuses, n))
							}
							Err(e) => {
								if statuses.is_empty() {
									Err(e)
								} else {
									Ok(TimelineData::Statuses(statuses, None))
								}
							}
						}
					}
				};
				// Remote instances fetch anonymously, so their statuses never carry our filter matches.
				if let TimelineType::InstanceLocal { .. } = timeline_type
					&& let Ok(TimelineData::Statuses(statuses, _)) = &mut result
					&& let Ok(filters) = client.get_filters(access_token)
				{
					for status in statuses {
						status.apply_filters(&filters);
					}
				}
				send_response(responses, ui_waker, NetworkResponse::TimelineLoaded { timeline_type, result, max_id });
			}
			Ok(NetworkCommand::FetchThread { timeline_type, focus }) => {
				let result = client
					.get_status_context(access_token, &focus.id)
					.map(|context| prepare_thread_timeline(*focus, context));
				send_response(
					responses,
					ui_waker,
					NetworkResponse::TimelineLoaded { timeline_type, result, max_id: None },
				);
			}
			Ok(NetworkCommand::FetchStatusSource { status }) => {
				let result = client.fetch_status_source(access_token, &status.id);
				send_response(responses, ui_waker, NetworkResponse::StatusSourceFetched { status, result });
			}
			Ok(NetworkCommand::ResolveStatusForThread { url }) => {
				let result = client
					.search(access_token, &url, SearchType::Statuses, Some(1), None)
					.and_then(|mut res| res.statuses.pop().context("Status not found on local instance"));
				send_response(responses, ui_waker, NetworkResponse::StatusResolvedForThread { result });
			}
			Ok(NetworkCommand::ResolveStatusForQuote { url }) => {
				let result = client
					.search(access_token, &url, SearchType::Statuses, Some(1), None)
					.and_then(|mut res| res.statuses.pop().context("Status not found on local instance"));
				send_response(responses, ui_waker, NetworkResponse::StatusResolvedForQuote { result });
			}
			Ok(NetworkCommand::ResolveAndInteract { url, interaction }) => {
				let status = client
					.search(access_token, &url, SearchType::Statuses, Some(1), None)
					.and_then(|mut res| res.statuses.pop().context("Status not found on local instance"));
				let response = match interaction {
					ForeignInteraction::Action(action) => NetworkResponse::StatusActed {
						action,
						result: status.and_then(|status| {
							client.status_action(access_token, &status.id, action).map(|result| (status.id, result))
						}),
					},
					ForeignInteraction::Vote(choices) => NetworkResponse::PollVoted {
						result: status.and_then(|status| {
							let poll = status.poll.context("Post has no poll")?;
							client.vote_poll(access_token, &poll.id, &choices)
						}),
					},
					ForeignInteraction::Reply(post) => NetworkResponse::Replied(status.and_then(|status| {
						post_with_media(
							client,
							access_token,
							&post.content,
							&post.visibility,
							post.sensitive,
							post.spoiler_text.as_deref(),
							post.content_type.as_deref(),
							post.language.as_deref(),
							post.media,
							post.poll.as_ref(),
							Some(&status.id),
							None,
							post.scheduled_at.as_deref(),
						)
					})),
				};
				send_response(responses, ui_waker, response);
			}
			Ok(NetworkCommand::ResolveAccount { url }) => {
				let result = client
					.search(access_token, &url, SearchType::Accounts, Some(1), None)
					.and_then(|mut res| res.accounts.pop().context("Account not found on local instance"));
				send_response(responses, ui_waker, NetworkResponse::AccountLookupResult { handle: url, result });
			}
			Ok(NetworkCommand::LookupAccount { handle }) => {
				let result = client.lookup_account(access_token, &handle);
				send_response(responses, ui_waker, NetworkResponse::AccountLookupResult { handle, result });
			}
			Ok(NetworkCommand::PostStatus { post }) => {
				let result = post_with_media(
					client,
					access_token,
					&post.content,
					&post.visibility,
					post.sensitive,
					post.spoiler_text.as_deref(),
					post.content_type.as_deref(),
					post.language.as_deref(),
					post.media,
					post.poll.as_ref(),
					None,
					post.quoted_status_id.as_deref(),
					post.scheduled_at.as_deref(),
				);
				send_response(responses, ui_waker, NetworkResponse::PostComplete(result));
			}
			Ok(NetworkCommand::EditStatus { status_id, content, sensitive, spoiler_text, language, media, poll }) => {
				let result = edit_with_media(
					client,
					access_token,
					&status_id,
					&content,
					sensitive,
					spoiler_text.as_deref(),
					language.as_deref(),
					media,
					poll.as_ref(),
				);
				send_response(responses, ui_waker, NetworkResponse::StatusEdited { _status_id: status_id, result });
			}
			Ok(NetworkCommand::DeleteStatus { status_id }) => {
				let result = client.delete_status(access_token, &status_id);
				send_response(responses, ui_waker, NetworkResponse::StatusDeleted { status_id, result });
			}
			Ok(NetworkCommand::ActOnStatus { status_id, action }) => {
				let result = client.status_action(access_token, &status_id, action).map(|status| (status_id, status));
				send_response(responses, ui_waker, NetworkResponse::StatusActed { action, result });
			}
			Ok(NetworkCommand::Reply {
				in_reply_to_id,
				content,
				visibility,
				sensitive,
				spoiler_text,
				content_type,
				language,
				media,
				poll,
				scheduled_at,
			}) => {
				let result = post_with_media(
					client,
					access_token,
					&content,
					&visibility,
					sensitive,
					spoiler_text.as_deref(),
					content_type.as_deref(),
					language.as_deref(),
					media,
					poll.as_ref(),
					Some(&in_reply_to_id),
					None,
					scheduled_at.as_deref(),
				);
				send_response(responses, ui_waker, NetworkResponse::Replied(result));
			}
			Ok(NetworkCommand::FollowTag { name }) => {
				let result = client.follow_tag(access_token, &name);
				send_response(responses, ui_waker, NetworkResponse::TagFollowed { name, result });
			}
			Ok(NetworkCommand::UnfollowTag { name }) => {
				let result = client.unfollow_tag(access_token, &name);
				send_response(responses, ui_waker, NetworkResponse::TagUnfollowed { name, result });
			}
			Ok(NetworkCommand::FetchTagsInfo { names }) => {
				let mut tags = Vec::new();
				for name in &names {
					if let Ok(tag) = client.get_tag(access_token, name) {
						tags.push(tag);
					}
				}
				if let Ok(filters) = client.get_filters(access_token)
					&& let Some(filter) = filters.iter().find(|f| is_muted_tags_filter(f))
				{
					for tag in &mut tags {
						let keyword = format!("#{}", tag.name);
						tag.muted = filter.keywords.iter().any(|kw| kw.keyword.eq_ignore_ascii_case(&keyword));
					}
				}
				let result = Ok(tags);
				send_response(responses, ui_waker, NetworkResponse::TagsInfoFetched { result });
			}
			Ok(NetworkCommand::MuteTag { name }) => {
				let result = (|| -> Result<()> {
					let filters = client.get_filters(access_token)?;
					let keyword = format!("#{name}");
					if let Some(filter) = filters.into_iter().find(is_muted_tags_filter) {
						if !filter.keywords.iter().any(|kw| kw.keyword.eq_ignore_ascii_case(&keyword)) {
							client.add_filter_keyword(access_token, &filter.id, &keyword, true)?;
						}
					} else {
						client.create_filter(
							access_token,
							MUTED_TAGS_FILTER,
							&[
								FilterContext::Home,
								FilterContext::Notifications,
								FilterContext::Public,
								FilterContext::Thread,
								FilterContext::Account,
							],
							&FilterAction::Hide,
							&[(keyword, true)],
							None,
						)?;
					}
					Ok(())
				})();
				send_response(responses, ui_waker, NetworkResponse::TagMuted { name, result });
			}
			Ok(NetworkCommand::UnmuteTag { name }) => {
				let result = (|| -> Result<()> {
					let filters = client.get_filters(access_token)?;
					let keyword = format!("#{name}");
					for filter in filters.iter().filter(|f| is_muted_tags_filter(f)) {
						for kw in filter.keywords.iter().filter(|kw| kw.keyword.eq_ignore_ascii_case(&keyword)) {
							client.delete_filter_keyword(access_token, &kw.id)?;
						}
					}
					Ok(())
				})();
				send_response(responses, ui_waker, NetworkResponse::TagUnmuted { name, result });
			}
			Ok(NetworkCommand::FetchRebloggedBy { status_id }) => {
				let result = client.get_reblogged_by(access_token, &status_id);
				send_response(responses, ui_waker, NetworkResponse::RebloggedByLoaded { result });
			}
			Ok(NetworkCommand::FetchFavoritedBy { status_id }) => {
				let result = client.get_favourited_by(access_token, &status_id);
				send_response(responses, ui_waker, NetworkResponse::FavoritedByLoaded { result });
			}
			Ok(NetworkCommand::FetchFollowers { account_id, acct, total_count }) => {
				let result = if acct.contains('@') {
					client.get_remote_followers(&acct).map(|accounts| (accounts, None))
				} else {
					client.get_followers_page(access_token, &account_id, None)
				};
				send_response(
					responses,
					ui_waker,
					NetworkResponse::FollowersLoaded { result, total_count, account_id },
				);
			}
			Ok(NetworkCommand::FetchFollowing { account_id, acct, total_count }) => {
				let result = if acct.contains('@') {
					client.get_remote_following(&acct).map(|accounts| (accounts, None))
				} else {
					client.get_following_page(access_token, &account_id, None)
				};
				send_response(
					responses,
					ui_waker,
					NetworkResponse::FollowingLoaded { result, total_count, account_id },
				);
			}
			Ok(NetworkCommand::FetchNextFollowersPage { account_id, max_id }) => {
				let result = client.get_followers_page(access_token, &account_id, Some(&max_id));
				send_response(responses, ui_waker, NetworkResponse::FollowersNextPageLoaded { result });
			}
			Ok(NetworkCommand::FetchNextFollowingPage { account_id, max_id }) => {
				let result = client.get_following_page(access_token, &account_id, Some(&max_id));
				send_response(responses, ui_waker, NetworkResponse::FollowingNextPageLoaded { result });
			}
			Ok(NetworkCommand::FollowAccount { account_id, target_name, reblogs, notify, action }) => {
				let result = client.follow_account_with_options(access_token, &account_id, reblogs, notify);
				send_response(
					responses,
					ui_waker,
					NetworkResponse::RelationshipUpdated { target_name, action, result },
				);
			}
			Ok(NetworkCommand::ToggleFollow { account_id, acct, target_name }) => {
				let current = account_id
					.map_or_else(|| client.lookup_account(access_token, &acct).map(|account| account.id), Ok)
					.and_then(|id| {
						let rel = client
							.get_relationships(access_token, slice::from_ref(&id))?
							.pop()
							.context("Server returned no relationship")?;
						Ok((id, rel))
					});
				let (action, result) = match current {
					Ok((id, rel)) if rel.following => {
						(RelationshipAction::Unfollow, client.unfollow_account(access_token, &id))
					}
					Ok((id, rel)) if rel.requested => {
						(RelationshipAction::CancelFollowRequest, client.unfollow_account(access_token, &id))
					}
					Ok((id, _)) => {
						(RelationshipAction::Follow, client.follow_account_with_options(access_token, &id, true, false))
					}
					Err(err) => (RelationshipAction::ToggleFollow, Err(err)),
				};
				send_response(
					responses,
					ui_waker,
					NetworkResponse::RelationshipUpdated { target_name, action, result },
				);
			}
			Ok(NetworkCommand::UnfollowAccount { account_id, target_name, action }) => {
				let result = client.unfollow_account(access_token, &account_id);
				send_response(
					responses,
					ui_waker,
					NetworkResponse::RelationshipUpdated { target_name, action, result },
				);
			}
			Ok(NetworkCommand::AuthorizeFollowRequest { account_id, target_name }) => {
				let result = client.authorize_follow_request(access_token, &account_id);
				send_response(
					responses,
					ui_waker,
					NetworkResponse::RelationshipUpdated {
						target_name,
						action: RelationshipAction::AcceptFollowRequest,
						result,
					},
				);
			}
			Ok(NetworkCommand::RejectFollowRequest { account_id, target_name }) => {
				let result = client.reject_follow_request(access_token, &account_id);
				send_response(
					responses,
					ui_waker,
					NetworkResponse::RelationshipUpdated {
						target_name,
						action: RelationshipAction::RejectFollowRequest,
						result,
					},
				);
			}
			Ok(NetworkCommand::BlockAccount { account_id, target_name }) => {
				let result = client.block_account(access_token, &account_id);
				send_response(
					responses,
					ui_waker,
					NetworkResponse::RelationshipUpdated { target_name, action: RelationshipAction::Block, result },
				);
			}
			Ok(NetworkCommand::UnblockAccount { account_id, target_name }) => {
				let result = client.unblock_account(access_token, &account_id);
				send_response(
					responses,
					ui_waker,
					NetworkResponse::RelationshipUpdated { target_name, action: RelationshipAction::Unblock, result },
				);
			}
			Ok(NetworkCommand::MuteAccount { account_id, target_name }) => {
				let result = client.mute_account(access_token, &account_id);
				send_response(
					responses,
					ui_waker,
					NetworkResponse::RelationshipUpdated { target_name, action: RelationshipAction::Mute, result },
				);
			}
			Ok(NetworkCommand::UnmuteAccount { account_id, target_name }) => {
				let result = client.unmute_account(access_token, &account_id);
				send_response(
					responses,
					ui_waker,
					NetworkResponse::RelationshipUpdated { target_name, action: RelationshipAction::Unmute, result },
				);
			}
			Ok(NetworkCommand::FetchRelationship { account_id }) => {
				let result = client
					.get_relationships(access_token, slice::from_ref(&account_id))
					.and_then(first_relationship_result);
				send_response(
					responses,
					ui_waker,
					NetworkResponse::RelationshipLoaded { _account_id: account_id, result },
				);
			}
			Ok(NetworkCommand::FetchRelationshipsForList { account_ids, for_followers }) => {
				let results = client.get_relationships(access_token, &account_ids).unwrap_or_default();
				send_response(
					responses,
					ui_waker,
					NetworkResponse::RelationshipsForListLoaded { results, for_followers },
				);
			}
			Ok(NetworkCommand::FetchAccount { account_id }) => {
				let result = client.get_account(access_token, &account_id);
				send_response(responses, ui_waker, NetworkResponse::AccountFetched { result });
			}
			Ok(NetworkCommand::VotePoll { poll_id, choices }) => {
				let result = client.vote_poll(access_token, &poll_id, &choices);
				send_response(responses, ui_waker, NetworkResponse::PollVoted { result });
			}
			Ok(NetworkCommand::FetchCredentials) => {
				let result = client.verify_credentials(access_token);
				let max_profile_fields = if result.is_ok() { client.get_max_profile_fields() } else { None };
				send_response(responses, ui_waker, NetworkResponse::CredentialsFetched { result, max_profile_fields });
			}
			Ok(NetworkCommand::UpdateProfile { update }) => {
				let result = client.update_credentials(
					access_token,
					update.display_name.as_deref(),
					update.note.as_deref(),
					update.avatar.as_deref(),
					update.header.as_deref(),
					update.locked,
					update.bot,
					update.discoverable,
					update.fields_attributes.as_deref(),
					update.source.as_ref().and_then(|s| s.privacy.as_deref()),
					update.source.as_ref().and_then(|s| s.sensitive),
					update.source.as_ref().and_then(|s| s.language.as_deref()),
				);
				send_response(responses, ui_waker, NetworkResponse::ProfileUpdated { result });
			}
			Ok(NetworkCommand::Search { query, search_type, limit, offset }) => {
				let result = client.search(access_token, &query, search_type, limit, offset);
				send_response(
					responses,
					ui_waker,
					NetworkResponse::SearchLoaded { query, search_type, result, offset },
				);
			}
			Ok(NetworkCommand::FetchLists) => {
				let result = client.get_lists(access_token);
				send_response(responses, ui_waker, NetworkResponse::ListsFetched { result });
			}
			Ok(NetworkCommand::CreateList { title, policy, exclusive }) => {
				let result = client.create_list(access_token, &title, &policy, exclusive);
				send_response(responses, ui_waker, NetworkResponse::ListCreated { result });
			}
			Ok(NetworkCommand::UpdateList { id, title, policy, exclusive }) => {
				let result = client.update_list(access_token, &id, &title, &policy, exclusive);
				send_response(responses, ui_waker, NetworkResponse::ListUpdated { result });
			}
			Ok(NetworkCommand::DeleteList { id }) => {
				let result = client.delete_list(access_token, &id);
				send_response(responses, ui_waker, NetworkResponse::ListDeleted { result });
			}
			Ok(NetworkCommand::FetchListAccounts { list_id }) => {
				let result = client.get_list_accounts(access_token, &list_id);
				send_response(responses, ui_waker, NetworkResponse::ListAccountsFetched { list_id, result });
			}
			Ok(NetworkCommand::FetchAccountLists { account_id }) => {
				let result = client.get_account_lists(access_token, &account_id);
				send_response(responses, ui_waker, NetworkResponse::AccountListsFetched { account_id, result });
			}
			Ok(NetworkCommand::AddListAccount { list_id, account_id }) => {
				let result = client.add_list_accounts(access_token, &list_id, slice::from_ref(&account_id));
				send_response(responses, ui_waker, NetworkResponse::ListAccountAdded { result });
			}
			Ok(NetworkCommand::RemoveListAccount { list_id, account_id }) => {
				let result = client.remove_list_accounts(access_token, &list_id, slice::from_ref(&account_id));
				send_response(responses, ui_waker, NetworkResponse::ListAccountRemoved { result });
			}
			Ok(NetworkCommand::FetchNotificationPolicy) => {
				let result = client.get_notification_policy(access_token);
				send_response(responses, ui_waker, NetworkResponse::NotificationPolicyLoaded(result));
			}
			Ok(NetworkCommand::FetchInstanceInfo) => {
				send_response(responses, ui_waker, NetworkResponse::InstanceInfoFetched(client.get_instance_info()));
			}
			Ok(NetworkCommand::FetchMarkers) => {
				let result = client.get_markers(access_token);
				send_response(responses, ui_waker, NetworkResponse::MarkersLoaded(result));
			}
			Ok(NetworkCommand::SaveMarker { timeline, last_read_id }) => {
				let _ = client.set_marker(access_token, timeline, &last_read_id);
			}
			Ok(NetworkCommand::Shutdown) | Err(_) => {
				break;
			}
		}
	}
}
