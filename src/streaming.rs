use std::{
	sync::mpsc::{self, Receiver, Sender},
	thread::{self, JoinHandle},
	time::Duration,
};

use serde::Deserialize;
use tungstenite::{Message, connect};
use url::Url;

use crate::mastodon::Status;

#[derive(Debug, Clone)]
pub enum StreamEvent {
	Update(Box<Status>),
	Delete(String),
	Connected,
	Disconnected,
	Error(String),
}

pub struct StreamHandle {
	receiver: Receiver<StreamEvent>,
	_thread: JoinHandle<()>,
}

impl StreamHandle {
	pub fn try_recv(&self) -> Option<StreamEvent> {
		self.receiver.try_recv().ok()
	}

	pub fn drain(&self) -> Vec<StreamEvent> {
		let mut events = Vec::new();
		while let Some(event) = self.try_recv() {
			events.push(event);
		}
		events
	}
}

pub fn start_streaming(streaming_url: Url) -> StreamHandle {
	let (sender, receiver) = mpsc::channel();
	let thread = thread::spawn(move || {
		streaming_loop(streaming_url, sender);
	});
	StreamHandle { receiver, _thread: thread }
}

fn streaming_loop(url: Url, sender: Sender<StreamEvent>) {
	let mut retry_count = 0;
	let max_retries = 5;
	let base_delay = Duration::from_secs(1);
	loop {
		match connect_and_stream(&url, &sender) {
			Ok(()) => {
				// Clean disconnect, stop streaming
				break;
			}
			Err(e) => {
				retry_count += 1;
				if retry_count > max_retries {
					let _ = sender
						.send(StreamEvent::Error(format!("Streaming failed after {} retries: {}", max_retries, e)));
					break;
				}
				let _ = sender.send(StreamEvent::Disconnected);
				let delay = base_delay * 2u32.pow(retry_count - 1);
				thread::sleep(delay);
			}
		}
	}
}

fn connect_and_stream(url: &Url, sender: &Sender<StreamEvent>) -> Result<(), String> {
	let (mut socket, _response) = connect(url.as_str()).map_err(|e| format!("WebSocket connection failed: {}", e))?;
	let _ = sender.send(StreamEvent::Connected);
	loop {
		match socket.read() {
			Ok(Message::Text(text)) => {
				if let Some(event) = parse_stream_message(&text)
					&& sender.send(event).is_err()
				{
					return Ok(());
				}
			}
			Ok(Message::Ping(data)) => {
				let _ = socket.send(Message::Pong(data));
			}
			Ok(Message::Close(_)) => {
				return Ok(());
			}
			Ok(_) => {
				// Ignore other message types (Binary, Pong, Frame)
			}
			Err(tungstenite::Error::Io(e)) if e.kind() == std::io::ErrorKind::WouldBlock => {
				continue;
			}
			Err(e) => {
				return Err(format!("WebSocket error: {}", e));
			}
		}
	}
}

#[derive(Debug, Deserialize)]
struct StreamMessage {
	event: String,
	payload: Option<String>,
}

fn parse_stream_message(text: &str) -> Option<StreamEvent> {
	let msg: StreamMessage = serde_json::from_str(text).ok()?;
	match msg.event.as_str() {
		"update" => {
			let payload = msg.payload?;
			let status: Status = serde_json::from_str(&payload).ok()?;
			Some(StreamEvent::Update(Box::new(status)))
		}
		"delete" => {
			let status_id = msg.payload?;
			Some(StreamEvent::Delete(status_id))
		}
		_ => None,
	}
}
