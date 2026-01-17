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

	pub fn timeline_display(&self) -> String {
		match &self.reblog {
			Some(boosted) => {
				let booster = self.account.display_name_or_username();
				let author = boosted.account.display_name_or_username();
				let content = strip_html(&boosted.content);
				format!("{} boosted {}: {}", booster, author, content)
			}
			None => {
				let author = self.account.display_name_or_username();
				format!("{}: {}", author, self.display_text())
			}
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
	html2text::from_read(html.as_bytes(), usize::MAX)
		.unwrap_or_else(|_| html.to_string())
		.trim()
		.to_string()
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
