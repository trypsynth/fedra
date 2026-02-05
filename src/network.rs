use std::{
	cmp::Ordering,
	slice,
	sync::mpsc::{self, Receiver, Sender},
	thread::{self, JoinHandle},
};

use anyhow::Result;
use chrono::DateTime;
use url::Url;

use crate::{
	mastodon::{Account, Conversation, MastodonClient, Notification, SearchResults, SearchType, Status, StatusContext},
	timeline::TimelineType,
	ui_wake::UiWaker,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationshipAction {
	Follow,
	Unfollow,
	Block,
	Unblock,
	Mute,
	Unmute,
	ShowBoosts,
	HideBoosts,
}

#[derive(Debug)]
pub enum NetworkCommand {
	FetchTimeline {
		timeline_type: TimelineType,
		limit: Option<u32>,
		max_id: Option<String>,
	},
	FetchThread {
		timeline_type: TimelineType,
		focus: Status,
	},
	LookupAccount {
		handle: String,
	},
	PostStatus {
		content: String,
		visibility: String,
		spoiler_text: Option<String>,
		content_type: Option<String>,
		media: Vec<MediaUpload>,
		poll: Option<PollData>,
	},
	Favorite {
		status_id: String,
	},
	Bookmark {
		status_id: String,
	},
	Unfavorite {
		status_id: String,
	},
	Unbookmark {
		status_id: String,
	},
	Boost {
		status_id: String,
	},
	Unboost {
		status_id: String,
	},
	Reply {
		in_reply_to_id: String,
		content: String,
		visibility: String,
		spoiler_text: Option<String>,
		content_type: Option<String>,
		media: Vec<MediaUpload>,
		poll: Option<PollData>,
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
		action: RelationshipAction,
	},
	UnfollowAccount {
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
	},
	FetchFollowing {
		account_id: String,
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
		spoiler_text: Option<String>,
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
}

#[derive(Debug)]
pub enum NetworkResponse {
	TimelineLoaded {
		timeline_type: TimelineType,
		result: Result<TimelineData>,
		max_id: Option<String>,
	},
	AccountLookupResult {
		handle: String,
		result: Result<Account>,
	},
	PostComplete(Result<()>),
	Favorited {
		status_id: String,
		result: Result<Status>,
	},
	Bookmarked {
		status_id: String,
		result: Result<Status>,
	},
	Unfavorited {
		status_id: String,
		result: Result<Status>,
	},
	Unbookmarked {
		status_id: String,
		result: Result<Status>,
	},
	Boosted {
		status_id: String,
		result: Result<Status>,
	},
	Unboosted {
		status_id: String,
		result: Result<Status>,
	},
	Replied(Result<()>),
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
	TagUnfollowed {
		name: String,
		result: Result<crate::mastodon::Tag>,
	},
	RelationshipUpdated {
		_account_id: String,
		target_name: String,
		action: RelationshipAction,
		result: Result<crate::mastodon::Relationship>,
	},
	RelationshipLoaded {
		_account_id: String,
		result: Result<crate::mastodon::Relationship>,
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
	RebloggedByLoaded {
		result: Result<Vec<Account>>,
	},
	FavoritedByLoaded {
		result: Result<Vec<Account>>,
	},
	FollowersLoaded {
		result: Result<Vec<Account>>,
	},
	FollowingLoaded {
		result: Result<Vec<Account>>,
	},
	CredentialsFetched {
		result: Result<Account>,
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
}

#[derive(Debug)]
pub enum TimelineData {
	Statuses(Vec<Status>, Option<String>),
	Notifications(Vec<Notification>, Option<String>),
	Conversations(Vec<Conversation>, Option<String>),
}

fn post_with_media(
	client: &MastodonClient,
	access_token: &str,
	content: &str,
	visibility: &str,
	spoiler_text: Option<&str>,
	content_type: Option<&str>,
	media: Vec<MediaUpload>,
	poll: Option<&PollData>,
	in_reply_to_id: Option<&str>,
) -> Result<()> {
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
		spoiler_text,
		&media_ids,
		content_type,
		poll,
		in_reply_to_id,
	)
}

fn edit_with_media(
	client: &MastodonClient,
	access_token: &str,
	status_id: &str,
	content: &str,
	spoiler_text: Option<&str>,
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
	client.edit_status(access_token, status_id, content, spoiler_text, &media_ids, poll)
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

pub fn start_network(base_url: Url, access_token: String, ui_waker: UiWaker) -> Result<NetworkHandle> {
	let client = MastodonClient::new(base_url)?;
	let (cmd_tx, cmd_rx) = mpsc::channel();
	let (resp_tx, resp_rx) = mpsc::channel();
	let thread = thread::spawn(move || {
		network_loop(&client, &access_token, &cmd_rx, &resp_tx, &ui_waker);
	});
	Ok(NetworkHandle { command_tx: cmd_tx, response_rx: resp_rx, _thread: thread })
}

fn send_response(responses: &Sender<NetworkResponse>, ui_waker: &UiWaker, response: NetworkResponse) {
	let _ = responses.send(response);
	ui_waker.wake();
}

fn prepare_thread_timeline(focus: Status, context: StatusContext) -> TimelineData {
	let mut statuses = context.ancestors;
	statuses.push(focus);
	statuses.extend(context.descendants);
	let mut indexed: Vec<(usize, Status)> = statuses.into_iter().enumerate().collect();
	indexed.sort_by(|(a_idx, a), (b_idx, b)| {
		let a_time: Option<DateTime<chrono::Utc>> = a.created_at.parse().ok();
		let b_time: Option<DateTime<chrono::Utc>> = b.created_at.parse().ok();
		match (a_time, b_time) {
			(Some(a_time), Some(b_time)) => b_time.cmp(&a_time).then_with(|| a_idx.cmp(b_idx)),
			(Some(_), None) => Ordering::Less,
			(None, Some(_)) => Ordering::Greater,
			(None, None) => a_idx.cmp(b_idx),
		}
	});
	let sorted: Vec<Status> = indexed.into_iter().map(|(_, status)| status).collect();
	TimelineData::Statuses(sorted, None)
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
				let result = match timeline_type {
					TimelineType::Notifications => client
						.get_notifications(access_token, limit, max_id.as_deref())
						.map(|(n, next)| TimelineData::Notifications(n, next)),
					TimelineType::Direct => client
						.get_conversations(access_token, limit, max_id.as_deref())
						.map(|(c, next)| TimelineData::Conversations(c, next)),
					TimelineType::Thread { ref id, .. } => match client.get_status(access_token, id) {
						Ok(focus) => match client.get_status_context(access_token, id) {
							Ok(context) => Ok(prepare_thread_timeline(focus, context)),
							Err(e) => Err(e),
						},
						Err(e) => Err(e),
					},
					_ => client
						.get_timeline(access_token, &timeline_type, limit, max_id.as_deref())
						.map(|(s, next)| TimelineData::Statuses(s, next)),
				};
				send_response(responses, ui_waker, NetworkResponse::TimelineLoaded { timeline_type, result, max_id });
			}
			Ok(NetworkCommand::FetchThread { timeline_type, focus }) => {
				let result = client
					.get_status_context(access_token, &focus.id)
					.map(|context| prepare_thread_timeline(focus, context));
				send_response(
					responses,
					ui_waker,
					NetworkResponse::TimelineLoaded { timeline_type, result, max_id: None },
				);
			}
			Ok(NetworkCommand::LookupAccount { handle }) => {
				let result = client.lookup_account(access_token, &handle);
				send_response(responses, ui_waker, NetworkResponse::AccountLookupResult { handle, result });
			}
			Ok(NetworkCommand::PostStatus { content, visibility, spoiler_text, content_type, media, poll }) => {
				let result = post_with_media(
					client,
					access_token,
					&content,
					&visibility,
					spoiler_text.as_deref(),
					content_type.as_deref(),
					media,
					poll.as_ref(),
					None,
				);
				send_response(responses, ui_waker, NetworkResponse::PostComplete(result));
			}
			Ok(NetworkCommand::EditStatus { status_id, content, spoiler_text, media, poll }) => {
				let result = edit_with_media(
					client,
					access_token,
					&status_id,
					&content,
					spoiler_text.as_deref(),
					media,
					poll.as_ref(),
				);
				send_response(responses, ui_waker, NetworkResponse::StatusEdited { _status_id: status_id, result });
			}
			Ok(NetworkCommand::DeleteStatus { status_id }) => {
				let result = client.delete_status(access_token, &status_id);
				send_response(responses, ui_waker, NetworkResponse::StatusDeleted { status_id, result });
			}
			Ok(NetworkCommand::Favorite { status_id }) => {
				let result = client.favorite(access_token, &status_id);
				send_response(responses, ui_waker, NetworkResponse::Favorited { status_id, result });
			}
			Ok(NetworkCommand::Bookmark { status_id }) => {
				let result = client.bookmark(access_token, &status_id);
				send_response(responses, ui_waker, NetworkResponse::Bookmarked { status_id, result });
			}
			Ok(NetworkCommand::Unfavorite { status_id }) => {
				let result = client.unfavorite(access_token, &status_id);
				send_response(responses, ui_waker, NetworkResponse::Unfavorited { status_id, result });
			}
			Ok(NetworkCommand::Unbookmark { status_id }) => {
				let result = client.unbookmark(access_token, &status_id);
				send_response(responses, ui_waker, NetworkResponse::Unbookmarked { status_id, result });
			}
			Ok(NetworkCommand::Boost { status_id }) => {
				let result = client.reblog(access_token, &status_id);
				send_response(responses, ui_waker, NetworkResponse::Boosted { status_id, result });
			}
			Ok(NetworkCommand::Unboost { status_id }) => {
				let result = client.unreblog(access_token, &status_id);
				send_response(responses, ui_waker, NetworkResponse::Unboosted { status_id, result });
			}
			Ok(NetworkCommand::Reply {
				in_reply_to_id,
				content,
				visibility,
				spoiler_text,
				content_type,
				media,
				poll,
			}) => {
				let result = post_with_media(
					client,
					access_token,
					&content,
					&visibility,
					spoiler_text.as_deref(),
					content_type.as_deref(),
					media,
					poll.as_ref(),
					Some(&in_reply_to_id),
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
				for name in names {
					if let Ok(tag) = client.get_tag(access_token, &name) {
						tags.push(tag);
					}
				}
				let result = Ok(tags);
				send_response(responses, ui_waker, NetworkResponse::TagsInfoFetched { result });
			}
			Ok(NetworkCommand::FetchRebloggedBy { status_id }) => {
				let result = client.get_reblogged_by(access_token, &status_id);
				send_response(responses, ui_waker, NetworkResponse::RebloggedByLoaded { result });
			}
			Ok(NetworkCommand::FetchFavoritedBy { status_id }) => {
				let result = client.get_favourited_by(access_token, &status_id);
				send_response(responses, ui_waker, NetworkResponse::FavoritedByLoaded { result });
			}
			Ok(NetworkCommand::FetchFollowers { account_id }) => {
				let result = client.get_followers(access_token, &account_id);
				send_response(responses, ui_waker, NetworkResponse::FollowersLoaded { result });
			}
			Ok(NetworkCommand::FetchFollowing { account_id }) => {
				let result = client.get_following(access_token, &account_id);
				send_response(responses, ui_waker, NetworkResponse::FollowingLoaded { result });
			}
			Ok(NetworkCommand::FollowAccount { account_id, target_name, reblogs, action }) => {
				let result = client.follow_account_with_options(access_token, &account_id, reblogs);
				send_response(
					responses,
					ui_waker,
					NetworkResponse::RelationshipUpdated { _account_id: account_id, target_name, action, result },
				);
			}
			Ok(NetworkCommand::UnfollowAccount { account_id, target_name }) => {
				let result = client.unfollow_account(access_token, &account_id);
				send_response(
					responses,
					ui_waker,
					NetworkResponse::RelationshipUpdated {
						_account_id: account_id,
						target_name,
						action: RelationshipAction::Unfollow,
						result,
					},
				);
			}
			Ok(NetworkCommand::BlockAccount { account_id, target_name }) => {
				let result = client.block_account(access_token, &account_id);
				send_response(
					responses,
					ui_waker,
					NetworkResponse::RelationshipUpdated {
						_account_id: account_id,
						target_name,
						action: RelationshipAction::Block,
						result,
					},
				);
			}
			Ok(NetworkCommand::UnblockAccount { account_id, target_name }) => {
				let result = client.unblock_account(access_token, &account_id);
				send_response(
					responses,
					ui_waker,
					NetworkResponse::RelationshipUpdated {
						_account_id: account_id,
						target_name,
						action: RelationshipAction::Unblock,
						result,
					},
				);
			}
			Ok(NetworkCommand::MuteAccount { account_id, target_name }) => {
				let result = client.mute_account(access_token, &account_id);
				send_response(
					responses,
					ui_waker,
					NetworkResponse::RelationshipUpdated {
						_account_id: account_id,
						target_name,
						action: RelationshipAction::Mute,
						result,
					},
				);
			}
			Ok(NetworkCommand::UnmuteAccount { account_id, target_name }) => {
				let result = client.unmute_account(access_token, &account_id);
				send_response(
					responses,
					ui_waker,
					NetworkResponse::RelationshipUpdated {
						_account_id: account_id,
						target_name,
						action: RelationshipAction::Unmute,
						result,
					},
				);
			}
			Ok(NetworkCommand::FetchRelationship { account_id }) => {
				let result =
					client.get_relationships(access_token, slice::from_ref(&account_id)).map(|mut rels| rels.remove(0));
				send_response(
					responses,
					ui_waker,
					NetworkResponse::RelationshipLoaded { _account_id: account_id, result },
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
				send_response(responses, ui_waker, NetworkResponse::CredentialsFetched { result });
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
			Ok(NetworkCommand::Shutdown) | Err(_) => {
				break;
			}
		}
	}
}
