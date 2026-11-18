//! Hashtags.

use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
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

/// A hashtag an account features on its profile.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct FeaturedTag {
	pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct TagHistory {
	pub day: String,
	pub uses: String,
	pub accounts: String,
}
