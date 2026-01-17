use std::{
	sync::mpsc::{self, Receiver, Sender},
	thread::{self, JoinHandle},
};

use url::Url;

use crate::{
	error::Result,
	mastodon::{MastodonClient, Status},
};

#[derive(Debug)]
pub enum NetworkCommand {
	FetchTimeline { limit: Option<u32> },
	PostStatus { content: String },
	Shutdown,
}

#[derive(Debug)]
pub enum NetworkResponse {
	TimelineLoaded(Result<Vec<Status>>),
	PostComplete(Result<()>),
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
			Ok(NetworkCommand::FetchTimeline { limit }) => {
				let result = client.get_home_timeline(&access_token, limit);
				let _ = responses.send(NetworkResponse::TimelineLoaded(result));
			}
			Ok(NetworkCommand::PostStatus { content }) => {
				let result = client.post_status(&access_token, &content);
				let _ = responses.send(NetworkResponse::PostComplete(result));
			}
			Ok(NetworkCommand::Shutdown) | Err(_) => {
				break;
			}
		}
	}
}
