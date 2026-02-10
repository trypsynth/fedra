use std::time::Instant;

use crate::{
	config::{ContentWarningDisplay, TimestampFormat},
	mastodon::{Account, Notification, SearchType, Status, Tag},
	streaming::StreamHandle,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimelineType {
	Home,
	Notifications,
	Direct,
	Local,
	Federated,
	Bookmarks,
	Favorites,
	User { id: String, name: String },
	Thread { id: String, name: String },
	Search { query: String, search_type: SearchType },
	Hashtag { name: String },
}

impl TimelineType {
	pub fn display_name(&self) -> String {
		match self {
			Self::Home => "Home".to_string(),
			Self::Notifications => "Notifications".to_string(),
			Self::Direct => "Direct Messages".to_string(),
			Self::Local => "Local".to_string(),
			Self::Federated => "Federated".to_string(),
			Self::Bookmarks => "Bookmarks".to_string(),
			Self::Favorites => "Favorites".to_string(),
			Self::User { name, .. } | Self::Thread { name, .. } => name.clone(),
			Self::Search { query, .. } => format!("Search: {query}"),
			Self::Hashtag { name } => format!("#{name}"),
		}
	}

	pub fn api_path(&self) -> String {
		match self {
			Self::Home => "api/v1/timelines/home".to_string(),
			Self::Notifications => "api/v1/notifications".to_string(),
			Self::Direct => "api/v1/conversations".to_string(),
			Self::Local | Self::Federated => "api/v1/timelines/public".to_string(),
			Self::Bookmarks => "api/v1/bookmarks".to_string(),
			Self::Favorites => "api/v1/favourites".to_string(),
			Self::User { id, .. } => format!("api/v1/accounts/{id}/statuses"),
			Self::Thread { id, .. } => format!("api/v1/statuses/{id}/context"),
			Self::Search { .. } => "api/v2/search".to_string(),
			Self::Hashtag { name } => format!("api/v1/timelines/tag/{name}"),
		}
	}

	pub fn api_query_params(&self) -> Vec<(&str, &str)> {
		match self {
			Self::Local => vec![("local", "true")],
			_ => vec![],
		}
	}

	pub const fn stream_params(&self) -> Option<&str> {
		match self {
			Self::Home | Self::Notifications => Some("user"),
			Self::Direct => Some("direct"),
			Self::Local => Some("public:local"),
			Self::Federated => Some("public"),
			Self::Bookmarks
			| Self::Favorites
			| Self::User { .. }
			| Self::Thread { .. }
			| Self::Search { .. }
			| Self::Hashtag { .. } => None,
		}
	}

	pub const fn is_closeable(&self) -> bool {
		!matches!(self, Self::Home | Self::Notifications)
	}

	pub const fn supports_paging(&self) -> bool {
		!matches!(self, Self::Thread { .. })
	}
}

#[derive(Debug, Clone)]
pub enum TimelineEntry {
	Status(Status),
	Notification(Notification),
	Account(Account),
	Hashtag(Tag),
}

impl TimelineEntry {
	pub const fn id(&self) -> &str {
		match self {
			Self::Status(status) => status.id.as_str(),
			Self::Notification(notification) => notification.id.as_str(),
			Self::Account(account) => account.id.as_str(),
			Self::Hashtag(tag) => tag.name.as_str(),
		}
	}

	pub fn display_text(
		&self,
		timestamp_format: TimestampFormat,
		cw_display: ContentWarningDisplay,
		cw_expanded: bool,
	) -> String {
		match self {
			Self::Status(status) => status.timeline_display(timestamp_format, cw_display, cw_expanded),
			Self::Notification(notification) => {
				notification.timeline_display(timestamp_format, cw_display, cw_expanded)
			}
			Self::Account(account) => {
				format!(
					"[Account] {} (@{}) - {} followers",
					account.display_name_or_username(),
					account.acct,
					account.followers_count
				)
			}
			Self::Hashtag(tag) => {
				let following_str = if tag.following { "following" } else { "not following" };
				format!("[Hashtag] #{} ({})", tag.name, following_str)
			}
		}
	}

	pub fn as_status(&self) -> Option<&Status> {
		match self {
			Self::Status(status) => Some(status),
			Self::Notification(notification) => notification.status.as_deref(),
			Self::Account(_) | Self::Hashtag(_) => None,
		}
	}

	pub fn as_status_mut(&mut self) -> Option<&mut Status> {
		match self {
			Self::Status(status) => Some(status),
			Self::Notification(notification) => notification.status.as_deref_mut(),
			Self::Account(_) | Self::Hashtag(_) => None,
		}
	}
}

pub struct Timeline {
	pub timeline_type: TimelineType,
	pub entries: Vec<TimelineEntry>,
	pub stream_handle: Option<StreamHandle>,
	pub selected_index: Option<usize>,
	pub selected_id: Option<String>,
	pub loading_more: bool,
	pub last_load_attempt: Option<Instant>,
	pub next_max_id: Option<String>,
}

impl Timeline {
	pub const fn new(timeline_type: TimelineType) -> Self {
		Self {
			timeline_type,
			entries: Vec::new(),
			stream_handle: None,
			selected_index: None,
			selected_id: None,
			loading_more: false,
			last_load_attempt: None,
			next_max_id: None,
		}
	}
}

pub struct TimelineManager {
	timelines: Vec<Timeline>,
	active_index: usize,
	history: Vec<TimelineType>,
	last_focused: Option<TimelineType>,
}

impl TimelineManager {
	pub const fn new() -> Self {
		Self { timelines: Vec::new(), active_index: 0, history: Vec::new(), last_focused: None }
	}

	pub fn open(&mut self, timeline_type: TimelineType) -> bool {
		if self.timelines.iter().any(|t| t.timeline_type == timeline_type) {
			return false;
		}
		self.timelines.push(Timeline::new(timeline_type));
		true
	}

	pub fn close(&mut self, timeline_type: &TimelineType, use_history: bool) -> bool {
		if !timeline_type.is_closeable() {
			return false;
		}

		if use_history {
			let can_go_back = self.history.iter().rev().any(|hist_type| {
				hist_type != timeline_type && self.timelines.iter().any(|t| t.timeline_type == *hist_type)
			});

			if !can_go_back {
				return false;
			}
		}

		if let Some(index) = self.timelines.iter().position(|t| t.timeline_type == *timeline_type) {
			let closing_active = index == self.active_index;

			self.timelines.remove(index);
			self.history.retain(|t| t != timeline_type);
			if self.last_focused.as_ref() == Some(timeline_type) {
				self.last_focused = None;
			}

			if closing_active {
				let mut handled = if use_history { self.go_back() } else { false };

				if !handled {
					handled = self.focus_last_focused();
				}

				if !handled {
					if self.active_index >= self.timelines.len() && !self.timelines.is_empty() {
						self.active_index = self.timelines.len() - 1;
					} else if self.active_index > 0 {
						self.active_index -= 1;
					}
				}
			} else if index < self.active_index {
				self.active_index -= 1;
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
		if index < self.timelines.len() && index != self.active_index {
			if let Some(current) = self.timelines.get(self.active_index) {
				self.last_focused = Some(current.timeline_type.clone());
			}
			self.active_index = index;
		}
	}

	pub fn snapshot_active_to_history(&mut self) {
		if let Some(current) = self.timelines.get(self.active_index) {
			self.history.push(current.timeline_type.clone());
		}
	}

	pub fn get_mut(&mut self, timeline_type: &TimelineType) -> Option<&mut Timeline> {
		self.timelines.iter_mut().find(|t| t.timeline_type == *timeline_type)
	}

	pub fn index_of(&self, timeline_type: &TimelineType) -> Option<usize> {
		self.timelines.iter().position(|t| t.timeline_type == *timeline_type)
	}

	pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Timeline> {
		self.timelines.iter_mut()
	}

	pub fn display_names(&self) -> Vec<String> {
		self.timelines.iter().map(|t| t.timeline_type.display_name()).collect()
	}

	pub const fn active_index(&self) -> usize {
		self.active_index
	}

	pub const fn len(&self) -> usize {
		self.timelines.len()
	}

	pub fn go_back(&mut self) -> bool {
		while let Some(last_type) = self.history.pop() {
			if let Some(index) = self.timelines.iter().position(|t| t.timeline_type == last_type) {
				self.active_index = index;
				return true;
			}
		}
		false
	}

	fn focus_last_focused(&mut self) -> bool {
		let last_type = match self.last_focused.as_ref() {
			Some(last) => last.clone(),
			None => return false,
		};
		if let Some(index) = self.timelines.iter().position(|t| t.timeline_type == last_type) {
			self.active_index = index;
			return true;
		}
		false
	}
}

impl Default for TimelineManager {
	fn default() -> Self {
		Self::new()
	}
}
