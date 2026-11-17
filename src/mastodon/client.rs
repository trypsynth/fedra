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

use std::time::Instant;

use anyhow::{Context, Result};
use reqwest::{
	Url,
	blocking::{Client, RequestBuilder, Response},
};
use serde::de::DeserializeOwned;
pub use timelines::Markers;

use super::USER_AGENT;

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
		let http = Client::builder().user_agent(USER_AGENT).build().context("Failed to create HTTP client")?;
		Ok(Self { base_url, http })
	}

	/// Every request goes through here, so each is logged with how it went. Only the path is logged, since the query can hold what the user searched for.
	fn send(request: RequestBuilder, what: &str) -> Result<Response> {
		let (client, request) = request.build_split();
		let request = request.with_context(|| format!("Failed to {what}"))?;
		let method = request.method().clone();
		let host = request.url().host_str().unwrap_or_default().to_string();
		let path = request.url().path().to_string();
		let started = Instant::now();
		match client.execute(request) {
			Ok(response) => {
				let status = response.status().as_u16();
				let ms = started.elapsed().as_millis();
				if response.status().is_success() {
					tracing::debug!(%method, host, path, status, ms, "{what}");
				} else {
					tracing::warn!(%method, host, path, status, ms, "instance refused to {what}");
				}
				Ok(response)
			}
			Err(e) => {
				tracing::warn!(%method, host, path, error = %e, "couldn't reach the instance to {what}");
				Err(e).with_context(|| format!("Failed to {what}"))
			}
		}
	}

	/// Sends `request`, checks the status, and deserializes the JSON body.
	///
	/// `what` names the operation as a verb phrase, e.g. `"favorite status"`, and is
	/// used to build the error context for each stage of the request.
	fn send_json<T: DeserializeOwned>(request: RequestBuilder, what: &str) -> Result<T> {
		let response = Self::send(request, what)?
			.error_for_status()
			.with_context(|| format!("Instance rejected request to {what}"))?;
		response.json().with_context(|| format!("Invalid response while trying to {what}")).inspect_err(|e| {
			tracing::warn!(error = format!("{e:#}"), "unexpected response");
		})
	}

	/// Like [`Self::send_json`], but also returns the `max_id` of the next page, if any.
	fn send_json_paged<T: DeserializeOwned>(request: RequestBuilder, what: &str) -> Result<(T, Option<String>)> {
		let response = Self::send(request, what)?
			.error_for_status()
			.with_context(|| format!("Instance rejected request to {what}"))?;
		let next_max_id = Self::next_max_id(&response);
		let payload = response
			.json()
			.with_context(|| format!("Invalid response while trying to {what}"))
			.inspect_err(|e| tracing::warn!(error = format!("{e:#}"), "unexpected response"))?;
		Ok((payload, next_max_id))
	}

	/// Sends `request` and discards the body, for endpoints that return no content.
	fn send_empty(request: RequestBuilder, what: &str) -> Result<()> {
		Self::send(request, what)?
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
