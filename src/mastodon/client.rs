//! The HTTP client and the request plumbing every endpoint shares.

mod accounts;
mod auth;
mod filters;
mod instance;
mod lists;
mod search;
mod statuses;
mod tags;
mod timelines;

use anyhow::{Context, Result};
use reqwest::{
	Url,
	blocking::{Client, RequestBuilder, Response},
};
use serde::de::DeserializeOwned;

pub const DEFAULT_SCOPES: &str = "read write follow";

#[derive(Debug, Clone)]
pub struct MastodonClient {
	base_url: Url,
	http: Client,
}

#[derive(Debug, Clone)]
pub struct AppCredentials {
	pub client_id: String,
	pub client_secret: String,
}

impl MastodonClient {
	pub fn new(base_url: Url) -> Result<Self> {
		let http = Client::builder().user_agent("Fedra/0.1").build().context("Failed to create HTTP client")?;
		Ok(Self { base_url, http })
	}

	#[allow(dead_code)]
	pub const fn base_url(&self) -> &Url {
		&self.base_url
	}

	/// Sends `request`, checks the status, and deserializes the JSON body.
	///
	/// `what` names the operation as a verb phrase, e.g. `"favorite status"`, and is
	/// used to build the error context for each stage of the request.
	fn send_json<T: DeserializeOwned>(request: RequestBuilder, what: &str) -> Result<T> {
		let response = request
			.send()
			.with_context(|| format!("Failed to {what}"))?
			.error_for_status()
			.with_context(|| format!("Instance rejected request to {what}"))?;
		response.json().with_context(|| format!("Invalid response while trying to {what}"))
	}

	/// Like [`Self::send_json`], but also returns the `max_id` of the next page, if any.
	fn send_json_paged<T: DeserializeOwned>(request: RequestBuilder, what: &str) -> Result<(T, Option<String>)> {
		let response = request
			.send()
			.with_context(|| format!("Failed to {what}"))?
			.error_for_status()
			.with_context(|| format!("Instance rejected request to {what}"))?;
		let next_max_id = Self::next_max_id(&response);
		let payload = response.json().with_context(|| format!("Invalid response while trying to {what}"))?;
		Ok((payload, next_max_id))
	}

	/// Sends `request` and discards the body, for endpoints that return no content.
	fn send_empty(request: RequestBuilder, what: &str) -> Result<()> {
		request
			.send()
			.with_context(|| format!("Failed to {what}"))?
			.error_for_status()
			.with_context(|| format!("Instance rejected request to {what}"))?;
		Ok(())
	}

	fn get_json<T: DeserializeOwned>(&self, access_token: &str, url: Url, what: &str) -> Result<T> {
		Self::send_json(self.http.get(url).bearer_auth(access_token), what)
	}

	fn post_json<T: DeserializeOwned>(&self, access_token: &str, url: Url, what: &str) -> Result<T> {
		Self::send_json(self.http.post(url).bearer_auth(access_token), what)
	}

	fn delete_empty(&self, access_token: &str, url: Url, what: &str) -> Result<()> {
		Self::send_empty(self.http.delete(url).bearer_auth(access_token), what)
	}

	/// Extracts the `max_id` of the `next` link from a paginated response.
	fn next_max_id(response: &Response) -> Option<String> {
		response.headers().get("link").and_then(|h| h.to_str().ok()).and_then(Self::parse_link_header)
	}

	fn parse_link_header(header: &str) -> Option<String> {
		for link in header.split(',') {
			let parts: Vec<&str> = link.split(';').collect();
			if parts.len() < 2 {
				continue;
			}
			let url_part = parts[0].trim().trim_start_matches('<').trim_end_matches('>');
			let rel_part = parts[1].trim();

			if rel_part.contains("rel=\"next\"")
				&& let Ok(url) = Url::parse(url_part)
				&& let Some((_, value)) = url.query_pairs().find(|(key, _)| key == "max_id")
			{
				return Some(value.to_string());
			}
		}
		None
	}
}
