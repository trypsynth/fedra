use std::{
	sync::mpsc::{self, Receiver, Sender},
	thread::{self, JoinHandle},
};

use anyhow::Result;
use chrono::DateTime;
use url::Url;

use crate::{
	mastodon::{Account, MastodonClient, Notification, Status},
	timeline::TimelineType,
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
	Favourite {
		status_id: String,
	},
	Unfavourite {
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
	FetchTagsInfo {
		names: Vec<String>,
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
	Shutdown,
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
	Favourited {
		status_id: String,
		result: Result<Status>,
	},
	Unfavourited {
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
		result: Result<Status>,
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
	PollVoted {
		result: Result<crate::mastodon::Poll>,
	},
	TagsInfoFetched {
		result: Result<Vec<crate::mastodon::Tag>>,
	},
}

#[derive(Debug)]
pub enum TimelineData {
	Statuses(Vec<Status>),
	Notifications(Vec<Notification>),
}

fn post_with_media(
	client: &MastodonClient,
	access_token: &str,
	content: &str,
	visibility: &str,
	spoiler_text: Option<&str>,
	content_type: Option<&str>,
	media: Vec<MediaUpload>,
	poll: Option<PollData>,
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
		poll.as_ref(),
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
	poll: Option<PollData>,
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
	client.edit_status(access_token, status_id, content, spoiler_text, &media_ids, poll.as_ref())
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

pub fn start_network(base_url: Url, access_token: String) -> Result<NetworkHandle> {
	let client = MastodonClient::new(base_url)?;
	let (cmd_tx, cmd_rx) = mpsc::channel();
	let (resp_tx, resp_rx) = mpsc::channel();
	let thread = thread::spawn(move || {
		network_loop(client, access_token, cmd_rx, resp_tx);
	});
	Ok(NetworkHandle { command_tx: cmd_tx, response_rx: resp_rx, _thread: thread })
}

fn network_loop(
	client: MastodonClient,
	access_token: String,
	commands: Receiver<NetworkCommand>,
	responses: Sender<NetworkResponse>,
) {
	loop {
		match commands.recv() {
			Ok(NetworkCommand::FetchTimeline { timeline_type, limit, max_id }) => {
				let result = match timeline_type {
					TimelineType::Notifications => client
						.get_notifications(&access_token, limit, max_id.as_deref())
						.map(TimelineData::Notifications),
					_ => client
						.get_timeline(&access_token, &timeline_type, limit, max_id.as_deref())
						.map(TimelineData::Statuses),
				};
				let _ = responses.send(NetworkResponse::TimelineLoaded { timeline_type, result, max_id });
			}
			Ok(NetworkCommand::FetchThread { timeline_type, focus }) => {
				let result = client.get_status_context(&access_token, &focus.id).map(|context| {
					let mut statuses = context.ancestors;
					statuses.push(focus);
					statuses.extend(context.descendants);
					let mut indexed: Vec<(usize, Status)> = statuses.into_iter().enumerate().collect();
					indexed.sort_by(|(a_idx, a), (b_idx, b)| {
						let a_time: Option<DateTime<chrono::Utc>> = a.created_at.parse().ok();
						let b_time: Option<DateTime<chrono::Utc>> = b.created_at.parse().ok();
						match (a_time, b_time) {
							(Some(a_time), Some(b_time)) => b_time.cmp(&a_time).then_with(|| a_idx.cmp(b_idx)),
							(Some(_), None) => std::cmp::Ordering::Less,
							(None, Some(_)) => std::cmp::Ordering::Greater,
							(None, None) => a_idx.cmp(b_idx),
						}
					});
					let sorted: Vec<Status> = indexed.into_iter().map(|(_, status)| status).collect();
					TimelineData::Statuses(sorted)
				});
				let _ = responses.send(NetworkResponse::TimelineLoaded { timeline_type, result, max_id: None });
			}
			Ok(NetworkCommand::LookupAccount { handle }) => {
				let result = client.lookup_account(&access_token, &handle);
				let _ = responses.send(NetworkResponse::AccountLookupResult { handle, result });
			}
			Ok(NetworkCommand::PostStatus { content, visibility, spoiler_text, content_type, media, poll }) => {
				let result = post_with_media(
					&client,
					&access_token,
					&content,
					&visibility,
					spoiler_text.as_deref(),
					content_type.as_deref(),
					media,
					poll,
					None,
				);
				let _ = responses.send(NetworkResponse::PostComplete(result));
			}
			Ok(NetworkCommand::EditStatus { status_id, content, spoiler_text, media, poll }) => {
				let result =
					edit_with_media(&client, &access_token, &status_id, &content, spoiler_text.as_deref(), media, poll);
				let _ = responses.send(NetworkResponse::StatusEdited { _status_id: status_id, result });
			}
			Ok(NetworkCommand::DeleteStatus { status_id }) => {
				let result = client.delete_status(&access_token, &status_id);
				let _ = responses.send(NetworkResponse::StatusDeleted { status_id, result });
			}
			Ok(NetworkCommand::Favourite { status_id }) => {
				let result = client.favourite(&access_token, &status_id);
				let _ = responses.send(NetworkResponse::Favourited { status_id, result });
			}
			Ok(NetworkCommand::Unfavourite { status_id }) => {
				let result = client.unfavourite(&access_token, &status_id);
				let _ = responses.send(NetworkResponse::Unfavourited { status_id, result });
			}
			Ok(NetworkCommand::Boost { status_id }) => {
				let result = client.reblog(&access_token, &status_id);
				let _ = responses.send(NetworkResponse::Boosted { status_id, result });
			}
			Ok(NetworkCommand::Unboost { status_id }) => {
				let result = client.unreblog(&access_token, &status_id);
				let _ = responses.send(NetworkResponse::Unboosted { status_id, result });
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
					&client,
					&access_token,
					&content,
					&visibility,
					spoiler_text.as_deref(),
					content_type.as_deref(),
					media,
					poll,
					Some(&in_reply_to_id),
				);
				let _ = responses.send(NetworkResponse::Replied(result));
			}
			Ok(NetworkCommand::FollowTag { name }) => {
				let result = client.follow_tag(&access_token, &name);
				let _ = responses.send(NetworkResponse::TagFollowed { name, result });
			}
			Ok(NetworkCommand::UnfollowTag { name }) => {
				let result = client.unfollow_tag(&access_token, &name);
				let _ = responses.send(NetworkResponse::TagUnfollowed { name, result });
			}
			Ok(NetworkCommand::FetchTagsInfo { names }) => {
				let mut tags = Vec::new();
				for name in names {
					if let Ok(tag) = client.get_tag(&access_token, &name) {
						tags.push(tag)
					}
				}
				let result = Ok(tags);
				let _ = responses.send(NetworkResponse::TagsInfoFetched { result });
			}
			Ok(NetworkCommand::FollowAccount { account_id, target_name, reblogs, action }) => {
				let result = client.follow_account_with_options(&access_token, &account_id, reblogs);
				let _ = responses.send(NetworkResponse::RelationshipUpdated {
					_account_id: account_id,
					target_name,
					action,
					result,
				});
			}
			Ok(NetworkCommand::UnfollowAccount { account_id, target_name }) => {
				let result = client.unfollow_account(&access_token, &account_id);
				let _ = responses.send(NetworkResponse::RelationshipUpdated {
					_account_id: account_id,
					target_name,
					action: RelationshipAction::Unfollow,
					result,
				});
			}
			Ok(NetworkCommand::BlockAccount { account_id, target_name }) => {
				let result = client.block_account(&access_token, &account_id);
				let _ = responses.send(NetworkResponse::RelationshipUpdated {
					_account_id: account_id,
					target_name,
					action: RelationshipAction::Block,
					result,
				});
			}
			Ok(NetworkCommand::UnblockAccount { account_id, target_name }) => {
				let result = client.unblock_account(&access_token, &account_id);
				let _ = responses.send(NetworkResponse::RelationshipUpdated {
					_account_id: account_id,
					target_name,
					action: RelationshipAction::Unblock,
					result,
				});
			}
			Ok(NetworkCommand::MuteAccount { account_id, target_name }) => {
				let result = client.mute_account(&access_token, &account_id);
				let _ = responses.send(NetworkResponse::RelationshipUpdated {
					_account_id: account_id,
					target_name,
					action: RelationshipAction::Mute,
					result,
				});
			}
			Ok(NetworkCommand::UnmuteAccount { account_id, target_name }) => {
				let result = client.unmute_account(&access_token, &account_id);
				let _ = responses.send(NetworkResponse::RelationshipUpdated {
					_account_id: account_id,
					target_name,
					action: RelationshipAction::Unmute,
					result,
				});
			}
			Ok(NetworkCommand::FetchRelationship { account_id }) => {
				let result =
					client.get_relationships(&access_token, &[account_id.clone()]).map(|mut rels| rels.remove(0));
				let _ = responses.send(NetworkResponse::RelationshipLoaded { _account_id: account_id, result });
			}
			Ok(NetworkCommand::VotePoll { poll_id, choices }) => {
				let result = client.vote_poll(&access_token, &poll_id, &choices);
				let _ = responses.send(NetworkResponse::PollVoted { result });
			}
			Ok(NetworkCommand::Shutdown) | Err(_) => {
				break;
			}
		}
	}
}
