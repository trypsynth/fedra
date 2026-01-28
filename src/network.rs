use std::{
	sync::mpsc::{self, Receiver, Sender},
	thread::{self, JoinHandle},
};

use url::Url;

use crate::{
	error::Result,
	mastodon::{MastodonClient, Notification, Status},
	timeline::TimelineType,
};

#[derive(Debug)]
pub enum NetworkCommand {
	FetchTimeline {
		timeline_type: TimelineType,
		limit: Option<u32>,
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
	Shutdown,
}

#[derive(Debug, Clone)]
pub struct MediaUpload {
	pub path: String,
	pub description: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PollData {
	pub options: Vec<String>,
	pub expires_in: u32,
	pub multiple: bool,
}

#[derive(Debug)]
pub enum NetworkResponse {
	TimelineLoaded { timeline_type: TimelineType, result: Result<TimelineData> },
	PostComplete(Result<()>),
	Favourited { status_id: String, result: Result<Status> },
	Unfavourited { status_id: String, result: Result<Status> },
	Boosted { status_id: String, result: Result<Status> },
	Unboosted { status_id: String, result: Result<Status> },
	Replied(Result<()>),
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

pub struct NetworkHandle {
	command_tx: Sender<NetworkCommand>,
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
			Ok(NetworkCommand::FetchTimeline { timeline_type, limit }) => {
				let result = match timeline_type {
					TimelineType::Notifications => {
						client.get_notifications(&access_token, limit).map(TimelineData::Notifications)
					}
					_ => client.get_timeline(&access_token, &timeline_type, limit).map(TimelineData::Statuses),
				};
				let _ = responses.send(NetworkResponse::TimelineLoaded { timeline_type, result });
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
			Ok(NetworkCommand::Shutdown) | Err(_) => {
				break;
			}
		}
	}
}
