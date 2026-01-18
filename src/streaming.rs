use std::{
	sync::mpsc::{self, Receiver, Sender},
	thread::{self, JoinHandle},
	time::Duration,
};

use serde::Deserialize;
use tungstenite::{Message, connect};
use url::Url;

use crate::{
	mastodon::{Notification, Status},
	timeline::TimelineType,
};

#[derive(Debug, Clone)]
pub enum StreamEvent {
	Update { timeline_type: TimelineType, status: Box<Status> },
	Delete { timeline_type: TimelineType, id: String },
	Notification { timeline_type: TimelineType, notification: Box<Notification> },
	Connected(TimelineType),
	Disconnected(TimelineType),
	Error { timeline_type: TimelineType, message: String },
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

pub fn start_streaming(base_url: Url, access_token: String, timeline_type: TimelineType) -> Option<StreamHandle> {
	let stream_param = timeline_type.stream_params()?;
	let mut streaming_url = base_url.join("api/v1/streaming").ok()?;
	let scheme = if base_url.scheme() == "https" { "wss" } else { "ws" };
	streaming_url.set_scheme(scheme).ok()?;
	streaming_url.query_pairs_mut().append_pair("access_token", &access_token).append_pair("stream", stream_param);
	let (sender, receiver) = mpsc::channel();
	let thread = thread::spawn(move || {
		streaming_loop(streaming_url, timeline_type, sender);
	});
	Some(StreamHandle { receiver, _thread: thread })
}

fn streaming_loop(url: Url, timeline_type: TimelineType, sender: Sender<StreamEvent>) {
	let mut retry_count = 0;
	let max_retries = 5;
	let base_delay = Duration::from_secs(1);
	loop {
		match connect_and_stream(&url, &timeline_type, &sender) {
			Ok(()) => {
				// Clean disconnect, stop streaming
				break;
			}
			Err(e) => {
				retry_count += 1;
				if retry_count > max_retries {
					let _ = sender.send(StreamEvent::Error {
						timeline_type: timeline_type.clone(),
						message: format!("Streaming failed after {} retries: {}", max_retries, e),
					});
					break;
				}
				let _ = sender.send(StreamEvent::Disconnected(timeline_type.clone()));
				let delay = base_delay * 2u32.pow(retry_count - 1);
				thread::sleep(delay);
			}
		}
	}
}

fn connect_and_stream(url: &Url, timeline_type: &TimelineType, sender: &Sender<StreamEvent>) -> Result<(), String> {
	let (mut socket, _response) = connect(url.as_str()).map_err(|e| format!("WebSocket connection failed: {}", e))?;
	let _ = sender.send(StreamEvent::Connected(timeline_type.clone()));
	loop {
		match socket.read() {
			Ok(Message::Text(text)) => {
				if let Some(event) = parse_stream_message(&text, timeline_type)
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

fn parse_stream_message(text: &str, timeline_type: &TimelineType) -> Option<StreamEvent> {
	let msg: StreamMessage = serde_json::from_str(text).ok()?;
	match msg.event.as_str() {
		"update" => {
			if *timeline_type == TimelineType::Notifications {
				return None;
			}
			let payload = msg.payload?;
			let status: Status = serde_json::from_str(&payload).ok()?;
			Some(StreamEvent::Update { timeline_type: timeline_type.clone(), status: Box::new(status) })
		}
		"delete" => {
			if *timeline_type == TimelineType::Notifications {
				return None;
			}
			let status_id = msg.payload?;
			Some(StreamEvent::Delete { timeline_type: timeline_type.clone(), id: status_id })
		}
		"notification" => {
			if *timeline_type != TimelineType::Notifications {
				return None;
			}
			let payload = msg.payload?;
			let notification: Notification = serde_json::from_str(&payload).ok()?;
			Some(StreamEvent::Notification {
				timeline_type: timeline_type.clone(),
				notification: Box::new(notification),
			})
		}
		_ => None,
	}
}
