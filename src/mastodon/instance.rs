//! Instance metadata and the capabilities it advertises.

use serde::{Deserialize, Serialize};

use crate::mastodon::PollLimits;

#[derive(Debug, Deserialize)]
pub(super) struct InstanceResponse {
	#[serde(default)]
	pub(super) configuration: Option<InstanceConfiguration>,
	#[serde(default)]
	pub(super) urls: Option<InstanceUrls>,
	/// Pleroma and Akkoma report their profile field limit here.
	#[serde(default)]
	pub(super) pleroma: Option<PleromaInstance>,
}

#[derive(Debug, Deserialize)]
pub(super) struct PleromaInstance {
	#[serde(default)]
	pub(super) metadata: Option<PleromaMetadata>,
}

#[derive(Debug, Deserialize)]
pub(super) struct PleromaMetadata {
	#[serde(default)]
	pub(super) fields_limits: Option<PleromaFieldsLimits>,
}

#[derive(Debug, Deserialize)]
pub(super) struct PleromaFieldsLimits {
	#[serde(default)]
	pub(super) max_fields: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub(super) struct InstanceV2Response {
	#[serde(default)]
	pub(super) configuration: Option<InstanceConfiguration>,
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
	#[serde(default)]
	pub(super) accounts: Option<AccountConfiguration>,
	#[serde(default)]
	pub(super) media_attachments: Option<MediaConfiguration>,
}

#[derive(Debug, Deserialize)]
pub(super) struct MediaConfiguration {
	#[serde(default)]
	pub(super) description_limit: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub(super) struct AccountConfiguration {
	#[serde(default)]
	pub(super) max_profile_fields: Option<u32>,
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

pub const DEFAULT_MAX_MEDIA_DESCRIPTION_CHARS: usize = 1500;

/// The media description limit, preferring the v2 instance configuration over v1.
pub(super) fn media_description_limit(v2: Option<&InstanceConfiguration>, v1: Option<&InstanceConfiguration>) -> usize {
	let limit_of = |c: &InstanceConfiguration| c.media_attachments.as_ref()?.description_limit;
	v2.and_then(limit_of).or_else(|| v1.and_then(limit_of)).map_or(DEFAULT_MAX_MEDIA_DESCRIPTION_CHARS, |l| l as usize)
}

/// A language the instance lets posts be written in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Language {
	/// Such as "en".
	pub code: String,
	/// Such as "English".
	pub name: String,
}

#[derive(Debug, Clone)]
pub struct InstanceInfo {
	pub max_post_chars: usize,
	pub max_media_description_chars: usize,
	pub poll_limits: PollLimits,
	pub streaming_url: Option<String>,
	/// By name. Empty when the instance doesn't list them, as before Mastodon 4.2.
	pub languages: Vec<Language>,
}

impl Default for InstanceInfo {
	fn default() -> Self {
		Self {
			max_post_chars: 500,
			max_media_description_chars: DEFAULT_MAX_MEDIA_DESCRIPTION_CHARS,
			poll_limits: PollLimits::default(),
			streaming_url: None,
			languages: Vec::new(),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn config(json: &str) -> InstanceConfiguration {
		serde_json::from_str::<InstanceV2Response>(json).unwrap().configuration.unwrap()
	}

	#[test]
	fn prefers_v2_media_description_limit() {
		let v2 = config(r#"{"configuration": {"media_attachments": {"description_limit": 5000}}}"#);
		let v1 = config(r#"{"configuration": {"media_attachments": {"description_limit": 1000}}}"#);
		assert_eq!(media_description_limit(Some(&v2), Some(&v1)), 5000);
	}

	#[test]
	fn falls_back_to_v1_media_description_limit() {
		let v2 = config(r#"{"configuration": {}}"#);
		let v1 = config(r#"{"configuration": {"media_attachments": {"description_limit": 1000}}}"#);
		assert_eq!(media_description_limit(Some(&v2), Some(&v1)), 1000);
	}

	#[test]
	fn defaults_media_description_limit_when_missing() {
		let v1 = config(r#"{"configuration": {}}"#);
		assert_eq!(media_description_limit(None, Some(&v1)), DEFAULT_MAX_MEDIA_DESCRIPTION_CHARS);
	}
}
