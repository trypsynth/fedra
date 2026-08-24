//! The search endpoint.

use anyhow::Result;

use crate::mastodon::{MastodonClient, SearchResults, SearchType};

impl MastodonClient {
	pub fn search(
		&self,
		access_token: &str,
		query: &str,
		search_type: SearchType,
		limit: Option<u32>,
		offset: Option<u32>,
	) -> Result<SearchResults> {
		let mut url = self.base_url.join("api/v2/search")?;
		{
			let mut pairs = url.query_pairs_mut();
			pairs.append_pair("q", query);
			pairs.append_pair("resolve", "true");
			if let Some(type_str) = search_type.as_api_str() {
				pairs.append_pair("type", type_str);
			}
			if let Some(limit) = limit {
				pairs.append_pair("limit", &limit.to_string());
			}
			if let Some(offset) = offset {
				pairs.append_pair("offset", &offset.to_string());
			}
		}
		self.get_json(access_token, url, "perform search")
	}
}
