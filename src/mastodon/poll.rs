//! Polls and the instance limits that apply to them.

use serde::Deserialize;

use crate::mastodon::{
	instance::PollConfiguration,
	serde_util::{deserialize_option_u64_or_zero, deserialize_u64_or_zero},
};

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct Poll {
	pub id: String,
	pub expires_at: Option<String>,
	pub expired: bool,
	pub multiple: bool,
	#[serde(deserialize_with = "deserialize_u64_or_zero")]
	pub votes_count: u64,
	#[serde(default, deserialize_with = "deserialize_option_u64_or_zero")]
	pub voters_count: Option<u64>,
	pub options: Vec<PollOption>,
	pub voted: Option<bool>,
	pub own_votes: Option<Vec<u32>>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct PollOption {
	pub title: String,
	#[serde(default, deserialize_with = "deserialize_option_u64_or_zero")]
	pub votes_count: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct PollLimits {
	pub max_options: usize,
	pub max_option_chars: usize,
	pub min_expiration: u32,
	pub max_expiration: u32,
}

impl PollLimits {
	pub(super) fn from_config(config: &PollConfiguration) -> Self {
		Self {
			max_options: config.max_options.unwrap_or(4) as usize,
			max_option_chars: config.max_option_chars.unwrap_or(50) as usize,
			min_expiration: config.min_expiration.unwrap_or(300),
			max_expiration: config.max_expiration.unwrap_or(2_629_746),
		}
	}
}

impl Default for PollLimits {
	fn default() -> Self {
		Self { max_options: 4, max_option_chars: 50, min_expiration: 300, max_expiration: 2_629_746 }
	}
}
