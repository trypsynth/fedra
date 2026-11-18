//! Following and inspecting hashtags.

use anyhow::Result;

use crate::mastodon::{FeaturedTag, MastodonClient, Tag};

impl MastodonClient {
	pub fn follow_tag(&self, access_token: &str, tag_name: &str) -> Result<Tag> {
		let url = self.base_url.join(&format!("api/v1/tags/{tag_name}/follow"))?;
		self.post_json(access_token, url, "follow tag")
	}

	pub fn unfollow_tag(&self, access_token: &str, tag_name: &str) -> Result<Tag> {
		let url = self.base_url.join(&format!("api/v1/tags/{tag_name}/unfollow"))?;
		self.post_json(access_token, url, "unfollow tag")
	}

	/// Every hashtag the user follows, in the order the server keeps them.
	pub fn get_followed_tags(&self, access_token: &str) -> Result<Vec<Tag>> {
		let mut all = Vec::new();
		let mut max_id: Option<String> = None;
		loop {
			let mut url = self.base_url.join("api/v1/followed_tags")?;
			{
				let mut query = url.query_pairs_mut();
				query.append_pair("limit", "200");
				if let Some(id) = &max_id {
					query.append_pair("max_id", id);
				}
			}
			let (page, next): (Vec<Tag>, _) =
				Self::send_json_paged(self.http.get(url).bearer_auth(access_token), "fetch followed hashtags")?;
			let done = page.is_empty() || next.is_none();
			all.extend(page);
			if done {
				return Ok(all);
			}
			max_id = next;
		}
	}

	/// The hashtags an account features on its profile.
	pub fn get_featured_tags(&self, access_token: &str, account_id: &str) -> Result<Vec<FeaturedTag>> {
		let url = self.base_url.join(&format!("api/v1/accounts/{account_id}/featured_tags"))?;
		self.get_json(access_token, url, "fetch featured hashtags")
	}

	pub fn get_tag(&self, access_token: &str, tag_name: &str) -> Result<Tag> {
		let url = self.base_url.join(&format!("api/v1/tags/{tag_name}"))?;
		self.get_json(access_token, url, "fetch tag info")
	}
}
