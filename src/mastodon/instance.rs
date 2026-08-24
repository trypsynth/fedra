//! Instance metadata and the capabilities it advertises.

use serde::Deserialize;

use crate::mastodon::PollLimits;

#[derive(Debug, Deserialize)]
pub(super) struct InstanceResponse {
	#[serde(default)]
	pub(super) configuration: Option<InstanceConfiguration>,
	#[serde(default)]
	pub(super) urls: Option<InstanceUrls>,
}

#[derive(Debug, Deserialize)]
pub(super) struct InstanceUrls {
	#[serde(default)]
	pub(super) streaming_api: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct InstanceConfiguration {
	#[serde(default)]
	pub(super) statuses: Option<StatusConfiguration>,
	#[serde(default)]
	pub(super) polls: Option<PollConfiguration>,
}

#[derive(Debug, Deserialize)]
pub(super) struct StatusConfiguration {
	#[serde(default)]
	pub(super) max_characters: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub(super) struct PollConfiguration {
	#[serde(default)]
	pub(super) max_options: Option<u32>,
	#[serde(default)]
	pub(super) max_option_chars: Option<u32>,
	#[serde(default)]
	pub(super) min_expiration: Option<u32>,
	#[serde(default)]
	pub(super) max_expiration: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct InstanceInfo {
	pub max_post_chars: usize,
	pub poll_limits: PollLimits,
	pub streaming_url: Option<String>,
}

impl Default for InstanceInfo {
	fn default() -> Self {
		Self { max_post_chars: 500, poll_limits: PollLimits::default(), streaming_url: None }
	}
}
