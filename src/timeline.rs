use crate::{
	config::TimestampFormat,
	mastodon::{Notification, Status},
	streaming::StreamHandle,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimelineType {
	Home,
	Notifications,
	Local,
	Federated,
}

impl TimelineType {
	pub fn display_name(&self) -> &str {
		match self {
			TimelineType::Home => "Home",
			TimelineType::Notifications => "Notifications",
			TimelineType::Local => "Local",
			TimelineType::Federated => "Federated",
		}
	}

	pub fn api_path(&self) -> &str {
		match self {
			TimelineType::Home => "api/v1/timelines/home",
			TimelineType::Notifications => "api/v1/notifications",
			TimelineType::Local | TimelineType::Federated => "api/v1/timelines/public",
		}
	}

	pub fn api_query_params(&self) -> Vec<(&str, &str)> {
		match self {
			TimelineType::Local => vec![("local", "true")],
			TimelineType::Home | TimelineType::Federated | TimelineType::Notifications => vec![],
		}
	}

	pub fn stream_params(&self) -> Option<&str> {
		match self {
			TimelineType::Home => Some("user"),
			TimelineType::Notifications => Some("user"),
			TimelineType::Local => Some("public:local"),
			TimelineType::Federated => Some("public"),
		}
	}

	pub fn is_closeable(&self) -> bool {
		!matches!(self, TimelineType::Home | TimelineType::Notifications)
	}
}

#[derive(Debug, Clone)]
pub enum TimelineEntry {
	Status(Status),
	Notification(Notification),
}

impl TimelineEntry {
	pub fn display_text(&self, timestamp_format: TimestampFormat) -> String {
		match self {
			TimelineEntry::Status(status) => status.timeline_display(timestamp_format),
			TimelineEntry::Notification(notification) => notification.timeline_display(timestamp_format),
		}
	}

	pub fn as_status(&self) -> Option<&Status> {
		match self {
			TimelineEntry::Status(status) => Some(status),
			TimelineEntry::Notification(notification) => notification.status.as_deref(),
		}
	}

	pub fn as_status_mut(&mut self) -> Option<&mut Status> {
		match self {
			TimelineEntry::Status(status) => Some(status),
			TimelineEntry::Notification(notification) => notification.status.as_deref_mut(),
		}
	}
}

pub struct Timeline {
	pub timeline_type: TimelineType,
	pub entries: Vec<TimelineEntry>,
	pub stream_handle: Option<StreamHandle>,
	pub selected_index: Option<usize>,
}

impl Timeline {
	pub fn new(timeline_type: TimelineType) -> Self {
		Self { timeline_type, entries: Vec::new(), stream_handle: None, selected_index: None }
	}
}

pub struct TimelineManager {
	timelines: Vec<Timeline>,
	active_index: usize,
}

impl TimelineManager {
	pub fn new() -> Self {
		Self { timelines: Vec::new(), active_index: 0 }
	}

	pub fn open(&mut self, timeline_type: TimelineType) -> bool {
		if self.timelines.iter().any(|t| t.timeline_type == timeline_type) {
			return false;
		}
		self.timelines.push(Timeline::new(timeline_type));
		true
	}

	pub fn close(&mut self, timeline_type: &TimelineType) -> bool {
		if !timeline_type.is_closeable() {
			return false;
		}
		if let Some(index) = self.timelines.iter().position(|t| t.timeline_type == *timeline_type) {
			self.timelines.remove(index);
			if self.active_index >= self.timelines.len() && !self.timelines.is_empty() {
				self.active_index = self.timelines.len() - 1;
			}
			return true;
		}
		false
	}

	pub fn active(&self) -> Option<&Timeline> {
		self.timelines.get(self.active_index)
	}

	pub fn active_mut(&mut self) -> Option<&mut Timeline> {
		self.timelines.get_mut(self.active_index)
	}

	pub fn set_active(&mut self, index: usize) {
		if index < self.timelines.len() {
			self.active_index = index;
		}
	}

	pub fn get_mut(&mut self, timeline_type: &TimelineType) -> Option<&mut Timeline> {
		self.timelines.iter_mut().find(|t| t.timeline_type == *timeline_type)
	}

	pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Timeline> {
		self.timelines.iter_mut()
	}

	pub fn display_names(&self) -> Vec<String> {
		self.timelines.iter().map(|t| t.timeline_type.display_name().to_string()).collect()
	}

	pub fn active_index(&self) -> usize {
		self.active_index
	}

	pub fn len(&self) -> usize {
		self.timelines.len()
	}
}

impl Default for TimelineManager {
	fn default() -> Self {
		Self::new()
	}
}
