use std::{
	net::{TcpStream, ToSocketAddrs},
	sync::mpsc::{self, Receiver, Sender},
	thread::{self, JoinHandle},
	time::{Duration, Instant},
};

use serde::Deserialize;
use tungstenite::{Message, client::IntoClientRequest, client_tls_with_config, http::Uri};
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

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const READ_TIMEOUT: Duration = Duration::from_secs(15);
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(20);
const IDLE_TIMEOUT: Duration = Duration::from_secs(45);

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
	let stream_params = timeline_type.stream_params()?;
	let mut streaming_url = base_url.join("api/v1/streaming").ok()?;
	let scheme = if base_url.scheme() == "https" || base_url.scheme() == "wss" { "wss" } else { "ws" };
	if streaming_url.set_scheme(scheme) == Err(()) {
		return None;
	}
	let mut query = streaming_url.query_pairs_mut();
	query.append_pair("access_token", access_token);
	for (key, value) in stream_params {
		query.append_pair(key, &value);
	}
	drop(query);
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
	let request = url.as_str().into_client_request().map_err(|e| format!("WebSocket request failed: {e}"))?;
	let uri = request.uri().clone();
	let tcp = connect_tcp_stream(&uri)?;
	let (mut socket, _response) =
		client_tls_with_config(request, tcp, None, None).map_err(|e| format!("WebSocket connection failed: {e}"))?;
	let mut last_message_at = Instant::now();
	let mut last_ping_at = Instant::now() - HEARTBEAT_INTERVAL;
	if !send_event(sender, ui_waker, StreamEvent::Connected(timeline_type.clone())) {
		return Ok(());
	}
	loop {
		if last_ping_at.elapsed() >= HEARTBEAT_INTERVAL {
			socket.send(Message::Ping(Vec::new().into())).map_err(|e| format!("WebSocket ping failed: {e}"))?;
			last_ping_at = Instant::now();
		}
		match socket.read() {
			Ok(Message::Text(text)) => {
				last_message_at = Instant::now();
				if let Some(event) = parse_stream_message(&text, timeline_type)
					&& !send_event(sender, ui_waker, event)
				{
					return Ok(());
				}
			}
			Ok(Message::Ping(data)) => {
				last_message_at = Instant::now();
				let _ = socket.send(Message::Pong(data));
			}
			Ok(Message::Pong(_)) => {
				last_message_at = Instant::now();
			}
			Ok(Message::Close(_)) => {
				return Err("WebSocket closed".to_string());
			}
			Ok(_) => {
				last_message_at = Instant::now();
				// Ignore other message types (Binary, Pong, Frame)
			}
			Err(tungstenite::Error::Io(e))
				if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) =>
			{
				if last_message_at.elapsed() >= IDLE_TIMEOUT {
					return Err(format!(
						"WebSocket heartbeat timed out after {} seconds of inactivity",
						IDLE_TIMEOUT.as_secs()
					));
				}
			}
			Err(e) => {
				return Err(format!("WebSocket error: {e}"));
			}
		}
	}
}

fn connect_tcp_stream(uri: &Uri) -> Result<TcpStream, String> {
	let host = uri.host().ok_or_else(|| "WebSocket URL missing host".to_string())?;
	let host = if host.starts_with('[') { &host[1..host.len() - 1] } else { host };
	let port = uri.port_u16().unwrap_or_else(|| if uri.scheme_str() == Some("wss") { 443 } else { 80 });
	let addrs = (host, port).to_socket_addrs().map_err(|e| format!("Failed to resolve WebSocket host: {e}"))?;
	let mut last_err = None;
	for addr in addrs {
		match TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT) {
			Ok(stream) => {
				configure_tcp_timeouts(&stream).map_err(|e| format!("Failed to configure WebSocket timeouts: {e}"))?;
				return Ok(stream);
			}
			Err(err) => last_err = Some(err),
		}
	}
	match last_err {
		Some(err) => Err(format!("Timed out connecting to WebSocket endpoint: {err}")),
		None => Err("WebSocket host resolved to no socket addresses".to_string()),
	}
}

fn configure_tcp_timeouts(stream: &TcpStream) -> std::io::Result<()> {
	stream.set_read_timeout(Some(READ_TIMEOUT))?;
	stream.set_write_timeout(Some(READ_TIMEOUT))?;
	Ok(())
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
			if matches!(timeline_type, TimelineType::Notifications | TimelineType::Mentions | TimelineType::Direct) {
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
