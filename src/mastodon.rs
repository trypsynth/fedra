use reqwest::{Url, blocking::Client};
use serde::Deserialize;

use crate::error::{Context, Result};

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

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct Status {
	pub id: String,
	pub content: String,
	pub created_at: String,
	pub account: Account,
	pub spoiler_text: String,
	pub reblog: Option<Box<Status>>,
	pub visibility: String,
	pub reblogs_count: u64,
	pub favourites_count: u64,
	pub replies_count: u64,
}

impl Status {
	pub fn display_text(&self) -> String {
		strip_html(&self.content)
	}

	pub fn summary(&self) -> String {
		let author = &self.account.display_name_or_username();
		let text = self.display_text();
		let preview: String = text.chars().take(100).collect();
		if self.reblog.is_some() {
			format!("{} boosted: {}", author, preview)
		} else {
			format!("{}: {}", author, preview)
		}
	}
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct Account {
	pub id: String,
	pub username: String,
	pub acct: String,
	pub display_name: String,
	pub url: String,
}

impl Account {
	pub fn display_name_or_username(&self) -> &str {
		if self.display_name.is_empty() { &self.username } else { &self.display_name }
	}
}

fn strip_html(html: &str) -> String {
	let mut result = String::with_capacity(html.len());
	let mut in_tag = false;
	let mut chars = html.chars().peekable();
	while let Some(c) = chars.next() {
		match c {
			'<' => {
				in_tag = true;
				let tag_start: String = chars.clone().take(3).collect();
				if (tag_start.starts_with("br") || tag_start.starts_with("p>") || tag_start.starts_with("p "))
					&& !result.ends_with('\n')
					&& !result.is_empty()
				{
					result.push('\n');
				}
			}
			'>' => in_tag = false,
			'&' if !in_tag => {
				let entity: String = chars.clone().take_while(|&c| c != ';').collect();
				let skip = entity.len() + 1; // +1 for semicolon
				match entity.as_str() {
					"amp" => result.push('&'),
					"lt" => result.push('<'),
					"gt" => result.push('>'),
					"quot" => result.push('"'),
					"apos" => result.push('\''),
					"nbsp" => result.push(' '),
					_ => result.push('&'), // Unknown entity, keep as-is
				}
				if entity.as_str() != "amp" || chars.clone().next() == Some(';') {
					for _ in 0..skip {
						chars.next();
					}
				}
			}
			_ if !in_tag => result.push(c),
			_ => {}
		}
	}
	result.trim().to_string()
}

impl MastodonClient {
	pub fn new(base_url: Url) -> Result<Self> {
		let http = Client::builder().user_agent("Fedra/0.1").build().context("Failed to create HTTP client")?;
		Ok(Self { base_url, http })
	}

	#[allow(dead_code)]
	pub fn base_url(&self) -> &Url {
		&self.base_url
	}

	pub fn register_app(&self, app_name: &str, redirect_uri: &str) -> Result<AppCredentials> {
		let url = self.base_url.join("api/v1/apps")?;
		let response = self
			.http
			.post(url)
			.form(&[
				("client_name", app_name),
				("redirect_uris", redirect_uri),
				("scopes", DEFAULT_SCOPES),
				("website", ""),
			])
			.send()
			.context("Failed to register app with instance")?
			.error_for_status()
			.context("Instance rejected app registration")?;
		let payload: RegisterAppResponse = response.json().context("Invalid response from instance")?;
		Ok(AppCredentials { client_id: payload.client_id, client_secret: payload.client_secret })
	}

	pub fn build_authorize_url(&self, credentials: &AppCredentials, redirect_uri: &str) -> Result<Url> {
		let mut url = self.base_url.join("oauth/authorize")?;
		url.query_pairs_mut()
			.append_pair("client_id", &credentials.client_id)
			.append_pair("redirect_uri", redirect_uri)
			.append_pair("response_type", "code")
			.append_pair("scope", DEFAULT_SCOPES);
		Ok(url)
	}

	pub fn exchange_token(&self, credentials: &AppCredentials, code: &str, redirect_uri: &str) -> Result<String> {
		let url = self.base_url.join("oauth/token")?;
		let response = self
			.http
			.post(url)
			.form(&[
				("client_id", credentials.client_id.as_str()),
				("client_secret", credentials.client_secret.as_str()),
				("redirect_uri", redirect_uri),
				("grant_type", "authorization_code"),
				("code", code),
				("scope", DEFAULT_SCOPES),
			])
			.send()
			.context("Failed to exchange token")?
			.error_for_status()
			.context("Instance rejected token exchange")?;
		let payload: TokenResponse = response.json().context("Invalid token response")?;
		Ok(payload.access_token)
	}

	pub fn post_status(&self, access_token: &str, status: &str) -> Result<()> {
		let url = self.base_url.join("api/v1/statuses")?;
		self.http
			.post(url)
			.bearer_auth(access_token)
			.form(&[("status", status)])
			.send()
			.context("Failed to post status")?
			.error_for_status()
			.context("Instance rejected status post")?;
		Ok(())
	}

	pub fn get_home_timeline(&self, access_token: &str, limit: Option<u32>) -> Result<Vec<Status>> {
		let mut url = self.base_url.join("api/v1/timelines/home")?;
		if let Some(limit) = limit {
			url.query_pairs_mut().append_pair("limit", &limit.to_string());
		}
		let response = self
			.http
			.get(url)
			.bearer_auth(access_token)
			.send()
			.context("Failed to fetch home timeline")?
			.error_for_status()
			.context("Instance rejected timeline request")?;
		let statuses: Vec<Status> = response.json().context("Invalid timeline response")?;
		Ok(statuses)
	}

	pub fn streaming_url(&self, access_token: &str) -> Result<Url> {
		let mut url = self.base_url.join("api/v1/streaming")?;
		let scheme = if self.base_url.scheme() == "https" { "wss" } else { "ws" };
		url.set_scheme(scheme).map_err(|_| anyhow::anyhow!("Failed to set WebSocket scheme"))?;
		url.query_pairs_mut().append_pair("access_token", access_token).append_pair("stream", "user");
		Ok(url)
	}
}

#[derive(Debug, Deserialize)]
struct RegisterAppResponse {
	client_id: String,
	client_secret: String,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
	access_token: String,
}
