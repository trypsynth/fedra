//! Following and inspecting hashtags.

use anyhow::Result;

use crate::mastodon::{MastodonClient, Tag};

impl MastodonClient {
	pub fn follow_tag(&self, access_token: &str, tag_name: &str) -> Result<Tag> {
		let url = self.base_url.join(&format!("api/v1/tags/{tag_name}/follow"))?;
		self.post_json(access_token, url, "follow tag")
	}

	pub fn unfollow_tag(&self, access_token: &str, tag_name: &str) -> Result<Tag> {
		let url = self.base_url.join(&format!("api/v1/tags/{tag_name}/unfollow"))?;
		self.post_json(access_token, url, "unfollow tag")
	}

	pub fn get_tag(&self, access_token: &str, tag_name: &str) -> Result<Tag> {
		let url = self.base_url.join(&format!("api/v1/tags/{tag_name}"))?;
		self.get_json(access_token, url, "fetch tag info")
	}
}
