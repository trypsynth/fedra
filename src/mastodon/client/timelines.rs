//! Paged timeline, notification, and conversation fetches.

use anyhow::Result;

use crate::{
	mastodon::{Conversation, MastodonClient, Notification, Status},
	timeline::TimelineType,
};

impl MastodonClient {
	pub fn get_timeline(
		&self,
		access_token: &str,
		timeline_type: &TimelineType,
		limit: Option<u32>,
		max_id: Option<&str>,
	) -> Result<(Vec<Status>, Option<String>)> {
		let mut url = self.base_url.join(&timeline_type.api_path())?;
		{
			let mut query = url.query_pairs_mut();
			for (key, value) in timeline_type.api_query_params() {
				query.append_pair(key, value);
			}
			if let Some(limit) = limit {
				query.append_pair("limit", &limit.to_string());
			}
			if let Some(max_id) = max_id {
				query.append_pair("max_id", max_id);
			}
		}
		let mut request = self.http.get(url);
		if timeline_type.requires_auth() {
			request = request.bearer_auth(access_token);
		}
		Self::send_json_paged(request, "fetch timeline")
	}

	pub fn get_notifications(
		&self,
		access_token: &str,
		timeline_type: &TimelineType,
		limit: Option<u32>,
		max_id: Option<&str>,
	) -> Result<(Vec<Notification>, Option<String>)> {
		let mut url = self.base_url.join(&timeline_type.api_path())?;
		{
			let mut query = url.query_pairs_mut();
			for (key, value) in timeline_type.api_query_params() {
				query.append_pair(key, value);
			}
			if let Some(limit) = limit {
				query.append_pair("limit", &limit.to_string());
			}
			if let Some(max_id) = max_id {
				query.append_pair("max_id", max_id);
			}
		}
		Self::send_json_paged(self.http.get(url).bearer_auth(access_token), "fetch notifications")
	}

	pub fn get_conversations(
		&self,
		access_token: &str,
		limit: Option<u32>,
		max_id: Option<&str>,
	) -> Result<(Vec<Conversation>, Option<String>)> {
		let mut url = self.base_url.join("api/v1/conversations")?;
		{
			let mut query = url.query_pairs_mut();
			if let Some(limit) = limit {
				query.append_pair("limit", &limit.to_string());
			}
			if let Some(max_id) = max_id {
				query.append_pair("max_id", max_id);
			}
		}
		Self::send_json_paged(self.http.get(url).bearer_auth(access_token), "fetch conversations")
	}
}
