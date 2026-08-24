//! Reading, writing, and acting on statuses.

use std::{cmp, thread, time::Duration};

use anyhow::{Context, Result};
use reqwest::{StatusCode, blocking::multipart};
use serde::Deserialize;
use serde_json::Value;

use crate::mastodon::{Account, MastodonClient, Poll, PostSubmission, Status, StatusContext, StatusSource};

#[derive(Debug, Deserialize)]
struct MediaResponse {
	id: String,
}

impl MastodonClient {
	pub fn post_status_with_media(
		&self,
		access_token: &str,
		status: &str,
		visibility: &str,
		sensitive: bool,
		spoiler_text: Option<&str>,
		media_ids: &[String],
		content_type: Option<&str>,
		language: Option<&str>,
		poll: Option<&crate::network::PollData>,
		in_reply_to_id: Option<&str>,
		quote_id: Option<&str>,
		scheduled_at: Option<&str>,
	) -> Result<PostSubmission> {
		let url = self.base_url.join("api/v1/statuses")?;
		let mut params =
			vec![("status".to_string(), status.to_string()), ("visibility".to_string(), visibility.to_string())];
		params.push(("sensitive".to_string(), sensitive.to_string()));
		if let Some(spoiler) = spoiler_text
			&& !spoiler.trim().is_empty()
		{
			params.push(("spoiler_text".to_string(), spoiler.to_string()));
		}
		if let Some(content_type) = content_type
			&& !content_type.trim().is_empty()
		{
			params.push(("content_type".to_string(), content_type.to_string()));
		}
		if let Some(language) = language
			&& !language.trim().is_empty()
		{
			params.push(("language".to_string(), language.to_string()));
		}
		if let Some(in_reply_to_id) = in_reply_to_id
			&& !in_reply_to_id.trim().is_empty()
		{
			params.push(("in_reply_to_id".to_string(), in_reply_to_id.to_string()));
		}
		if let Some(quote_id) = quote_id
			&& !quote_id.trim().is_empty()
		{
			params.push(("quoted_status_id".to_string(), quote_id.to_string()));
		}
		if let Some(scheduled_at) = scheduled_at
			&& !scheduled_at.trim().is_empty()
		{
			params.push(("scheduled_at".to_string(), scheduled_at.to_string()));
		}
		for media_id in media_ids {
			params.push(("media_ids[]".to_string(), media_id.clone()));
		}
		if let Some(poll) = poll {
			for option in &poll.options {
				params.push(("poll[options][]".to_string(), option.clone()));
			}
			params.push(("poll[expires_in]".to_string(), poll.expires_in.to_string()));
			params.push(("poll[multiple]".to_string(), poll.multiple.to_string()));
			params.push(("poll[hide_totals]".to_string(), poll.hide_totals.to_string()));
		}
		let response =
			self.http.post(url).bearer_auth(access_token).form(&params).send().context("Failed to post status")?;
		let status = response.status();
		if !status.is_success() {
			let body = response.text().unwrap_or_default();
			let detail = serde_json::from_str::<Value>(&body)
				.ok()
				.and_then(|json| {
					json.get("error")
						.and_then(Value::as_str)
						.or_else(|| json.get("error_description").and_then(Value::as_str))
						.map(std::string::ToString::to_string)
				})
				.unwrap_or_else(|| body.trim().to_string());
			let detail = if detail.is_empty() {
				format!("HTTP status {status}")
			} else {
				format!("HTTP status {status}: {detail}")
			};
			anyhow::bail!("Instance rejected status post ({detail})");
		}
		let submission: PostSubmission = response.json().context("Invalid status response")?;
		Ok(submission)
	}

	pub fn upload_media(&self, access_token: &str, path: &str, description: Option<&str>) -> Result<String> {
		let url = self.base_url.join("api/v2/media")?;
		let part = multipart::Part::file(path).context("Failed to read media file")?;
		let mut form = multipart::Form::new().part("file", part);
		if let Some(description) = description
			&& !description.trim().is_empty()
		{
			form = form.text("description", description.to_string());
		}
		let response =
			self.http.post(url).bearer_auth(access_token).multipart(form).send().context("Failed to upload media")?;
		let status = response.status();
		let response = response.error_for_status().context("Instance rejected media upload")?;
		let payload: MediaResponse = response.json().context("Invalid media upload response")?;
		// v2/media returns 202 when the media is still processing asynchronously.
		if status == reqwest::StatusCode::ACCEPTED {
			self.wait_for_media_processing(access_token, &payload.id)?;
		}
		Ok(payload.id)
	}

	fn wait_for_media_processing(&self, access_token: &str, media_id: &str) -> Result<()> {
		let url = self.base_url.join(&format!("api/v1/media/{media_id}"))?;
		for attempt in 0..60 {
			let delay = cmp::min(1 + attempt, 5);
			thread::sleep(Duration::from_secs(delay));
			let response = self
				.http
				.get(url.clone())
				.bearer_auth(access_token)
				.send()
				.context("Failed to check media processing status")?;
			match response.status() {
				StatusCode::OK => return Ok(()),
				StatusCode::PARTIAL_CONTENT => {}
				status => {
					anyhow::bail!("Media processing failed with status {status}");
				}
			}
		}
		anyhow::bail!("Media processing timed out")
	}

	pub fn get_pinned_statuses(&self, access_token: &str, account_id: &str) -> Result<Vec<Status>> {
		let mut url = self.base_url.join(&format!("api/v1/accounts/{account_id}/statuses"))?;
		url.query_pairs_mut().append_pair("pinned", "true");
		self.get_json(access_token, url, "fetch pinned statuses")
	}

	pub fn get_status(&self, access_token: &str, status_id: &str) -> Result<Status> {
		let url = self.base_url.join(&format!("api/v1/statuses/{status_id}"))?;
		self.get_json(access_token, url, "fetch status")
	}

	pub fn favorite(&self, access_token: &str, status_id: &str) -> Result<Status> {
		let url = self.base_url.join(&format!("api/v1/statuses/{status_id}/favourite"))?;
		self.post_json(access_token, url, "favorite status")
	}

	pub fn bookmark(&self, access_token: &str, status_id: &str) -> Result<Status> {
		let url = self.base_url.join(&format!("api/v1/statuses/{status_id}/bookmark"))?;
		self.post_json(access_token, url, "bookmark status")
	}

	pub fn unfavorite(&self, access_token: &str, status_id: &str) -> Result<Status> {
		let url = self.base_url.join(&format!("api/v1/statuses/{status_id}/unfavourite"))?;
		self.post_json(access_token, url, "unfavorite status")
	}

	pub fn unbookmark(&self, access_token: &str, status_id: &str) -> Result<Status> {
		let url = self.base_url.join(&format!("api/v1/statuses/{status_id}/unbookmark"))?;
		self.post_json(access_token, url, "unbookmark status")
	}

	pub fn pin_status(&self, access_token: &str, status_id: &str) -> Result<Status> {
		let url = self.base_url.join(&format!("api/v1/statuses/{status_id}/pin"))?;
		self.post_json(access_token, url, "pin status")
	}

	pub fn unpin_status(&self, access_token: &str, status_id: &str) -> Result<Status> {
		let url = self.base_url.join(&format!("api/v1/statuses/{status_id}/unpin"))?;
		self.post_json(access_token, url, "unpin status")
	}

	pub fn reblog(&self, access_token: &str, status_id: &str) -> Result<Status> {
		let url = self.base_url.join(&format!("api/v1/statuses/{status_id}/reblog"))?;
		self.post_json(access_token, url, "boost status")
	}

	pub fn unreblog(&self, access_token: &str, status_id: &str) -> Result<Status> {
		let url = self.base_url.join(&format!("api/v1/statuses/{status_id}/unreblog"))?;
		self.post_json(access_token, url, "unboost status")
	}

	pub fn get_status_context(&self, access_token: &str, status_id: &str) -> Result<StatusContext> {
		let url = self.base_url.join(&format!("api/v1/statuses/{status_id}/context"))?;
		self.get_json(access_token, url, "fetch status context")
	}

	pub fn get_reblogged_by(&self, access_token: &str, status_id: &str) -> Result<Vec<Account>> {
		let url = self.base_url.join(&format!("api/v1/statuses/{status_id}/reblogged_by"))?;
		self.get_json(access_token, url, "fetch boosts")
	}

	pub fn get_favourited_by(&self, access_token: &str, status_id: &str) -> Result<Vec<Account>> {
		let url = self.base_url.join(&format!("api/v1/statuses/{status_id}/favourited_by"))?;
		self.get_json(access_token, url, "fetch favorites")
	}

	pub fn vote_poll(&self, access_token: &str, poll_id: &str, choices: &[usize]) -> Result<Poll> {
		let url = self.base_url.join(&format!("api/v1/polls/{poll_id}/votes"))?;
		let mut params = Vec::new();
		for choice in choices {
			params.push(("choices[]", choice.to_string()));
		}
		Self::send_json(self.http.post(url).bearer_auth(access_token).form(&params), "vote on poll")
	}

	pub fn delete_status(&self, access_token: &str, status_id: &str) -> Result<()> {
		let url = self.base_url.join(&format!("api/v1/statuses/{status_id}"))?;
		self.delete_empty(access_token, url, "delete status")
	}

	pub fn fetch_status_source(&self, access_token: &str, status_id: &str) -> Result<StatusSource> {
		let url = self.base_url.join(&format!("api/v1/statuses/{status_id}/source"))?;
		self.get_json(access_token, url, "fetch status source")
	}

	pub fn edit_status(
		&self,
		access_token: &str,
		status_id: &str,
		status: &str,
		sensitive: bool,
		spoiler_text: Option<&str>,
		language: Option<&str>,
		media_ids: &[String],
		poll: Option<&crate::network::PollData>,
	) -> Result<Status> {
		let url = self.base_url.join(&format!("api/v1/statuses/{status_id}"))?;
		let mut params = vec![("status".to_string(), status.to_string())];
		params.push(("sensitive".to_string(), sensitive.to_string()));
		if let Some(spoiler) = spoiler_text {
			params.push(("spoiler_text".to_string(), spoiler.to_string()));
		}
		if let Some(language) = language
			&& !language.trim().is_empty()
		{
			params.push(("language".to_string(), language.to_string()));
		}
		for media_id in media_ids {
			params.push(("media_ids[]".to_string(), media_id.clone()));
		}
		if let Some(poll) = poll {
			for option in &poll.options {
				params.push(("poll[options][]".to_string(), option.clone()));
			}
			params.push(("poll[expires_in]".to_string(), poll.expires_in.to_string()));
			params.push(("poll[multiple]".to_string(), poll.multiple.to_string()));
			params.push(("poll[hide_totals]".to_string(), poll.hide_totals.to_string()));
		}
		Self::send_json(self.http.put(url).bearer_auth(access_token).form(&params), "edit status")
	}
}
