//! Hashtags.

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct Tag {
	pub name: String,
	pub url: String,
	#[serde(default)]
	pub following: bool,
	#[serde(default)]
	pub muted: bool,
	#[serde(default)]
	pub history: Vec<TagHistory>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct TagHistory {
	pub day: String,
	pub uses: String,
	pub accounts: String,
}
