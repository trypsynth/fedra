use std::{
	sync::mpsc::{self, Receiver, Sender},
	thread::{self, JoinHandle},
	time::Duration,
};

use serde::Deserialize;
use tungstenite::{Message, connect};
use url::Url;

use crate::{
	mastodon::{Conversation, Notification, Status},
	timeline::TimelineType,
	ui_wake::UiWaker,
};

#[derive(Debug, Clone)]
pub enum StreamEvent {
	Update { timeline_type: TimelineType, status: Box<Status> },
	Delete { timeline_type: TimelineType, id: String },
	Notification { timeline_type: TimelineType, notification: Box<Notification> },
	Conversation { timeline_type: TimelineType, conversation: Box<Conversation> },
	Connected(TimelineType),
	Disconnected(TimelineType),
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

pub fn start_streaming(
	base_url: &Url,
	access_token: &str,
	timeline_type: TimelineType,
	ui_waker: UiWaker,
) -> Option<StreamHandle> {
	let stream_param = timeline_type.stream_params()?;
	let mut streaming_url = base_url.join("api/v1/streaming").ok()?;
	let scheme = if base_url.scheme() == "https" { "wss" } else { "ws" };
	streaming_url.set_scheme(scheme).ok()?;
	streaming_url.query_pairs_mut().append_pair("access_token", access_token).append_pair("stream", stream_param);
	let (sender, receiver) = mpsc::channel();
	let thread = thread::spawn(move || {
		streaming_loop(&streaming_url, &timeline_type, &sender, &ui_waker);
	});
	Some(StreamHandle { receiver, _thread: thread })
}

fn send_event(sender: &Sender<StreamEvent>, ui_waker: &UiWaker, event: StreamEvent) -> bool {
	if sender.send(event).is_err() {
		return false;
	}
	ui_waker.wake();
	true
}

fn streaming_loop(url: &Url, timeline_type: &TimelineType, sender: &Sender<StreamEvent>, ui_waker: &UiWaker) {
	let mut retry_count: u32 = 0;
	let base_delay = Duration::from_secs(1);
	let max_delay = Duration::from_secs(60);
	loop {
		if connect_and_stream(url, timeline_type, sender, ui_waker) == Ok(()) {
			// Receiver dropped or intentional shutdown.
			break;
		}
		retry_count += 1;
		if !send_event(sender, ui_waker, StreamEvent::Disconnected(timeline_type.clone())) {
			break;
		}
		let exp = retry_count.saturating_sub(1).min(6);
		let delay = (base_delay * 2u32.pow(exp)).min(max_delay);
		thread::sleep(delay);
	}
}

fn connect_and_stream(
	url: &Url,
	timeline_type: &TimelineType,
	sender: &Sender<StreamEvent>,
	ui_waker: &UiWaker,
) -> Result<(), String> {
	let (mut socket, _response) = connect(url.as_str()).map_err(|e| format!("WebSocket connection failed: {e}"))?;
	if !send_event(sender, ui_waker, StreamEvent::Connected(timeline_type.clone())) {
		return Ok(());
	}
	loop {
		match socket.read() {
			Ok(Message::Text(text)) => {
				if let Some(event) = parse_stream_message(&text, timeline_type)
					&& !send_event(sender, ui_waker, event)
				{
					return Ok(());
				}
			}
			Ok(Message::Ping(data)) => {
				let _ = socket.send(Message::Pong(data));
			}
			Ok(Message::Close(_)) => {
				return Err("WebSocket closed".to_string());
			}
			Ok(_) => {
				// Ignore other message types (Binary, Pong, Frame)
			}
			Err(tungstenite::Error::Io(e)) if e.kind() == std::io::ErrorKind::WouldBlock => {}
			Err(e) => {
				return Err(format!("WebSocket error: {e}"));
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
		"conversation" => {
			if *timeline_type != TimelineType::Direct {
				return None;
			}
			let payload = msg.payload?;
			let conversation: Conversation = serde_json::from_str(&payload).ok()?;
			Some(StreamEvent::Conversation {
				timeline_type: timeline_type.clone(),
				conversation: Box::new(conversation),
			})
		}
		_ => None,
	}
}
