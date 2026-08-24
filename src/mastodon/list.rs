//! Mastodon lists.

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct List {
	pub id: String,
	pub title: String,
	pub replies_policy: Option<String>,
	#[serde(default)]
	pub exclusive: bool,
}

/// Builds the shared form body for list creation and updates.
pub(super) fn list_form<'a>(title: &'a str, replies_policy: &'a str, exclusive: bool) -> [(&'static str, &'a str); 3] {
	[("title", title), ("replies_policy", replies_policy), ("exclusive", if exclusive { "true" } else { "false" })]
}
