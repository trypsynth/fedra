use crate::{mastodon::Status, streaming::StreamHandle};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimelineType {
	Home,
	Local,
	Federated,
}

impl TimelineType {
	pub fn display_name(&self) -> &str {
		match self {
			TimelineType::Home => "Home",
			TimelineType::Local => "Local",
			TimelineType::Federated => "Federated",
		}
	}

	pub fn api_path(&self) -> &str {
		match self {
			TimelineType::Home => "api/v1/timelines/home",
			TimelineType::Local | TimelineType::Federated => "api/v1/timelines/public",
		}
	}

	pub fn api_query_params(&self) -> Vec<(&str, &str)> {
		match self {
			TimelineType::Local => vec![("local", "true")],
			TimelineType::Home | TimelineType::Federated => vec![],
		}
	}

	pub fn stream_params(&self) -> Option<&str> {
		match self {
			TimelineType::Home => Some("user"),
			TimelineType::Local => Some("public:local"),
			TimelineType::Federated => Some("public"),
		}
	}

	pub fn is_closeable(&self) -> bool {
		!matches!(self, TimelineType::Home)
	}

	pub fn id(&self) -> String {
		match self {
			TimelineType::Home => "home".to_string(),
			TimelineType::Local => "local".to_string(),
			TimelineType::Federated => "federated".to_string(),
		}
	}
}

pub struct Timeline {
	pub timeline_type: TimelineType,
	pub statuses: Vec<Status>,
	pub stream_handle: Option<StreamHandle>,
	pub selected_index: Option<usize>,
}

impl Timeline {
	pub fn new(timeline_type: TimelineType) -> Self {
		Self { timeline_type, statuses: Vec::new(), stream_handle: None, selected_index: None }
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

	pub fn get(&self, timeline_type: &TimelineType) -> Option<&Timeline> {
		self.timelines.iter().find(|t| t.timeline_type == *timeline_type)
	}

	pub fn get_mut(&mut self, timeline_type: &TimelineType) -> Option<&mut Timeline> {
		self.timelines.iter_mut().find(|t| t.timeline_type == *timeline_type)
	}

	pub fn iter(&self) -> impl Iterator<Item = &Timeline> {
		self.timelines.iter()
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

	pub fn is_empty(&self) -> bool {
		self.timelines.is_empty()
	}
}

impl Default for TimelineManager {
	fn default() -> Self {
		Self::new()
	}
}
