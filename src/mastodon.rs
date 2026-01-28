use chrono::{DateTime, Local, Utc};
use chrono_humanize::HumanTime;
use reqwest::{
	Url,
	blocking::{Client, multipart},
};
use serde::Deserialize;

use crate::{
	config::TimestampFormat,
	error::{Context, Result},
	html::strip_html,
	timeline::TimelineType,
};

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
	#[serde(default)]
	pub media_attachments: Vec<MediaAttachment>,
	pub application: Option<Application>,
	pub visibility: String,
	pub reblogs_count: u64,
	pub favourites_count: u64,
	pub replies_count: u64,
	#[serde(default)]
	pub favourited: bool,
	#[serde(default)]
	pub reblogged: bool,
	pub in_reply_to_id: Option<String>,
	pub in_reply_to_account_id: Option<String>,
	#[serde(default)]
	pub mentions: Vec<Mention>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct Mention {
	pub id: String,
	pub username: String,
	pub acct: String,
	pub url: String,
}

impl Status {
	pub fn display_text(&self) -> String {
		strip_html(&self.content)
	}

	pub fn timeline_display(&self, timestamp_format: TimestampFormat) -> String {
		match &self.reblog {
			Some(boosted) => {
				let booster = self.account.display_name_or_username();
				format!("{} boosted {}", booster, boosted.base_display(timestamp_format))
			}
			None => self.base_display(timestamp_format),
		}
	}

	pub fn details_display(&self, timestamp_format: TimestampFormat) -> String {
		self.base_display(timestamp_format)
	}

	fn base_display(&self, timestamp_format: TimestampFormat) -> String {
		let mut out = String::new();
		let author = self.account.display_name_or_username();
		out.push_str(author);
		out.push_str(": ");
		let content = self.content_with_cw();
		if !content.is_empty() {
			out.push_str(&content);
		}
		if let Some(media) = self.media_summary() {
			out.push_str(&media);
		}
		// Metadata line: time, visibility, client
		let mut meta = Vec::new();
		if let Some(when) = friendly_time(&self.created_at, timestamp_format) {
			meta.push(when);
		}
		meta.push(self.visibility_display());
		if let Some(client) = self.client_name() {
			meta.push(format!("via {}", client));
		}
		if !meta.is_empty() {
			out.push_str(" - ");
			out.push_str(&meta.join(", "));
		}
		out
	}

	fn visibility_display(&self) -> String {
		match self.visibility.as_str() {
			"public" => "Public".to_string(),
			"unlisted" => "Unlisted".to_string(),
			"private" => "Followers only".to_string(),
			"direct" => "Mentioned only".to_string(),
			other => other.to_string(),
		}
	}

	fn content_with_cw(&self) -> String {
		let content = self.display_text();
		if self.spoiler_text.trim().is_empty() {
			content
		} else if content.is_empty() {
			format!("Content warning: {}", self.spoiler_text.trim())
		} else {
			format!("Content warning: {} - {}", self.spoiler_text.trim(), content)
		}
	}

	fn client_name(&self) -> Option<String> {
		self.application
			.as_ref()
			.map(|app| app.name.as_str())
			.filter(|name| !name.trim().is_empty())
			.map(|name| name.to_string())
	}

	fn media_summary(&self) -> Option<String> {
		if self.media_attachments.is_empty() {
			return None;
		}
		let count = self.media_attachments.len();
		let types = self
			.media_attachments
			.iter()
			.map(|media| media.kind.as_str())
			.filter(|kind| !kind.trim().is_empty())
			.collect::<Vec<_>>()
			.join(", ");
		let alt_texts = self
			.media_attachments
			.iter()
			.enumerate()
			.map(|(index, media)| match media.description.as_deref().map(str::trim) {
				Some(text) if !text.is_empty() => format!("alt {}: {}", index + 1, text),
				_ => format!("alt {}: (missing)", index + 1),
			})
			.collect::<Vec<_>>()
			.join("; ");
		let mut summary = format!("media {}", count);
		if !types.is_empty() {
			summary.push_str(&format!(" ({})", types));
		}
		if !alt_texts.is_empty() {
			summary.push_str(&format!(" [{}]", alt_texts));
		}
		Some(summary)
	}
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct Application {
	pub name: String,
	pub website: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct MediaAttachment {
	pub id: String,
	#[serde(rename = "type")]
	pub kind: String,
	pub url: String,
	#[serde(default)]
	pub preview_url: Option<String>,
	#[serde(default)]
	pub description: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct Notification {
	pub id: String,
	#[serde(rename = "type")]
	pub kind: String,
	pub created_at: String,
	pub account: Account,
	pub status: Option<Box<Status>>,
}

impl Notification {
	pub fn timeline_display(&self, timestamp_format: TimestampFormat) -> String {
		let actor = self.account.display_name_or_username();
		match self.kind.as_str() {
			"mention" | "status" => self.status_text(timestamp_format).to_string(),
			"reblog" => format!("{} boosted {}", actor, self.status_text(timestamp_format)),
			"favourite" => format!("{} favourited {}", actor, self.status_text(timestamp_format)),
			"follow" => format!("{} followed you", actor),
			"follow_request" => format!("{} requested to follow you", actor),
			"poll" => format!("Poll ended: {}", self.status_text(timestamp_format)),
			_ => match self.status_text_if_any(timestamp_format) {
				Some(text) => format!("{} {}: {}", actor, self.kind, text),
				None => format!("{} {}", actor, self.kind),
			},
		}
	}

	fn status_text(&self, timestamp_format: TimestampFormat) -> String {
		self.status_text_if_any(timestamp_format).unwrap_or_else(|| "No status content".to_string())
	}

	fn status_text_if_any(&self, timestamp_format: TimestampFormat) -> Option<String> {
		self.status.as_ref().map(|status| status.details_display(timestamp_format))
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
	#[serde(default)]
	pub note: String,
	#[serde(default)]
	pub followers_count: u64,
	#[serde(default)]
	pub following_count: u64,
	#[serde(default)]
	pub statuses_count: u64,
	#[serde(default)]
	pub fields: Vec<AccountField>,
	#[serde(default)]
	pub created_at: String,
	#[serde(default)]
	pub locked: bool,
	#[serde(default)]
	pub bot: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AccountField {
	pub name: String,
	pub value: String,
}

impl Account {
	pub fn display_name_or_username(&self) -> &str {
		if self.display_name.is_empty() { &self.username } else { &self.display_name }
	}

	pub fn profile_display(&self) -> String {
		let mut lines = Vec::new();
		let name = self.display_name_or_username();
		lines.push(format!("Name: {}", name));
		lines.push(format!("Username: @{}", self.acct));
		lines.push(format!("Direct Profile URL: {}", self.url));
		lines.push(format!("Posts: {}", self.statuses_count));
		lines.push(format!("Following: {}", self.following_count));
		lines.push(format!("Followers: {}", self.followers_count));
		if self.bot || self.locked {
			if self.bot {
				lines.push("This account is a bot.".to_string());
			}
			if self.locked {
				lines.push("This account requires follow approval.".to_string());
			}
		}
		if !self.note.is_empty() {
			let bio = strip_html(&self.note);
			if !bio.trim().is_empty() {
				lines.push(format!("Bio: {}", bio));
			}
		}
		if !self.fields.is_empty() {
			lines.push("Fields:".to_string());
			for field in &self.fields {
				let value = strip_html(&field.value);
				lines.push(format!("\t{}: {}", field.name, value));
			}
		}
		if !self.created_at.is_empty() {
			if let Some(date) = friendly_date(&self.created_at) {
				lines.push(format!("Joined: {}", date));
			}
		}
		lines.join("\r\n")
	}
}

fn friendly_date(iso_time: &str) -> Option<String> {
	let trimmed = iso_time.trim();
	if trimmed.is_empty() {
		return None;
	}
	let parsed: DateTime<Utc> = trimmed.parse().ok()?;
	Some(parsed.format("%B %Y").to_string())
}

fn friendly_time(iso_time: &str, format: TimestampFormat) -> Option<String> {
	let trimmed = iso_time.trim();
	if trimmed.is_empty() {
		return None;
	}
	let parsed: DateTime<Utc> = trimmed.parse().ok()?;
	match format {
		TimestampFormat::Relative => {
			let human = HumanTime::from(parsed);
			Some(human.to_string())
		}
		TimestampFormat::Absolute => {
			let local: DateTime<Local> = parsed.into();
			Some(local.format("%b %d, %Y at %l:%M %p").to_string())
		}
	}
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

	pub fn post_status_with_media(
		&self,
		access_token: &str,
		status: &str,
		visibility: &str,
		spoiler_text: Option<&str>,
		media_ids: &[String],
		content_type: Option<&str>,
		poll: Option<&crate::network::PollData>,
	) -> Result<()> {
		let url = self.base_url.join("api/v1/statuses")?;
		let mut params =
			vec![("status".to_string(), status.to_string()), ("visibility".to_string(), visibility.to_string())];
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
		for media_id in media_ids {
			params.push(("media_ids[]".to_string(), media_id.clone()));
		}
		if let Some(poll) = poll {
			for option in &poll.options {
				params.push(("poll[options][]".to_string(), option.clone()));
			}
			params.push(("poll[expires_in]".to_string(), poll.expires_in.to_string()));
			params.push(("poll[multiple]".to_string(), poll.multiple.to_string()));
		}
		self.http
			.post(url)
			.bearer_auth(access_token)
			.form(&params)
			.send()
			.context("Failed to post status")?
			.error_for_status()
			.context("Instance rejected status post")?;
		Ok(())
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
		let response = self
			.http
			.post(url)
			.bearer_auth(access_token)
			.multipart(form)
			.send()
			.context("Failed to upload media")?
			.error_for_status()
			.context("Instance rejected media upload")?;
		let payload: MediaResponse = response.json().context("Invalid media upload response")?;
		Ok(payload.id)
	}

	pub fn get_timeline(
		&self,
		access_token: &str,
		timeline_type: &TimelineType,
		limit: Option<u32>,
	) -> Result<Vec<Status>> {
		let mut url = self.base_url.join(timeline_type.api_path())?;
		{
			let mut query = url.query_pairs_mut();
			for (key, value) in timeline_type.api_query_params() {
				query.append_pair(key, value);
			}
			if let Some(limit) = limit {
				query.append_pair("limit", &limit.to_string());
			}
		}
		let response = self
			.http
			.get(url)
			.bearer_auth(access_token)
			.send()
			.context("Failed to fetch timeline")?
			.error_for_status()
			.context("Instance rejected timeline request")?;
		let statuses: Vec<Status> = response.json().context("Invalid timeline response")?;
		Ok(statuses)
	}

	pub fn get_notifications(&self, access_token: &str, limit: Option<u32>) -> Result<Vec<Notification>> {
		let mut url = self.base_url.join("api/v1/notifications")?;
		{
			let mut query = url.query_pairs_mut();
			if let Some(limit) = limit {
				query.append_pair("limit", &limit.to_string());
			}
		}
		let response = self
			.http
			.get(url)
			.bearer_auth(access_token)
			.send()
			.context("Failed to fetch notifications")?
			.error_for_status()
			.context("Instance rejected notifications request")?;
		let notifications: Vec<Notification> = response.json().context("Invalid notifications response")?;
		Ok(notifications)
	}

	pub fn verify_credentials(&self, access_token: &str) -> Result<Account> {
		let url = self.base_url.join("api/v1/accounts/verify_credentials")?;
		let response = self
			.http
			.get(url)
			.bearer_auth(access_token)
			.send()
			.context("Failed to verify credentials")?
			.error_for_status()
			.context("Instance rejected credential verification")?;
		let account: Account = response.json().context("Invalid credentials response")?;
		Ok(account)
	}

	pub fn favourite(&self, access_token: &str, status_id: &str) -> Result<Status> {
		let url = self.base_url.join(&format!("api/v1/statuses/{}/favourite", status_id))?;
		let response = self
			.http
			.post(url)
			.bearer_auth(access_token)
			.send()
			.context("Failed to favourite status")?
			.error_for_status()
			.context("Instance rejected favourite request")?;
		let status: Status = response.json().context("Invalid favourite response")?;
		Ok(status)
	}

	pub fn unfavourite(&self, access_token: &str, status_id: &str) -> Result<Status> {
		let url = self.base_url.join(&format!("api/v1/statuses/{}/unfavourite", status_id))?;
		let response = self
			.http
			.post(url)
			.bearer_auth(access_token)
			.send()
			.context("Failed to unfavourite status")?
			.error_for_status()
			.context("Instance rejected unfavourite request")?;
		let status: Status = response.json().context("Invalid unfavourite response")?;
		Ok(status)
	}

	pub fn reblog(&self, access_token: &str, status_id: &str) -> Result<Status> {
		let url = self.base_url.join(&format!("api/v1/statuses/{}/reblog", status_id))?;
		let response = self
			.http
			.post(url)
			.bearer_auth(access_token)
			.send()
			.context("Failed to boost status")?
			.error_for_status()
			.context("Instance rejected boost request")?;
		let status: Status = response.json().context("Invalid boost response")?;
		Ok(status)
	}

	pub fn unreblog(&self, access_token: &str, status_id: &str) -> Result<Status> {
		let url = self.base_url.join(&format!("api/v1/statuses/{}/unreblog", status_id))?;
		let response = self
			.http
			.post(url)
			.bearer_auth(access_token)
			.send()
			.context("Failed to unboost status")?
			.error_for_status()
			.context("Instance rejected unboost request")?;
		let status: Status = response.json().context("Invalid unboost response")?;
		Ok(status)
	}

	pub fn reply(
		&self,
		access_token: &str,
		in_reply_to_id: &str,
		content: &str,
		visibility: &str,
		spoiler_text: Option<&str>,
	) -> Result<()> {
		let url = self.base_url.join("api/v1/statuses")?;
		let mut params = vec![
			("status".to_string(), content.to_string()),
			("visibility".to_string(), visibility.to_string()),
			("in_reply_to_id".to_string(), in_reply_to_id.to_string()),
		];
		if let Some(spoiler) = spoiler_text
			&& !spoiler.trim().is_empty()
		{
			params.push(("spoiler_text".to_string(), spoiler.to_string()));
		}
		self.http
			.post(url)
			.bearer_auth(access_token)
			.form(&params)
			.send()
			.context("Failed to post reply")?
			.error_for_status()
			.context("Instance rejected reply")?;
		Ok(())
	}

	pub fn get_instance_info(&self) -> Result<InstanceInfo> {
		let url = self.base_url.join("api/v1/instance")?;
		let response = self
			.http
			.get(url)
			.send()
			.context("Failed to fetch instance info")?
			.error_for_status()
			.context("Instance rejected info request")?;
		let info: InstanceResponse = response.json().context("Invalid instance response")?;
		let max_chars =
			info.configuration.as_ref().and_then(|c| c.statuses.as_ref()).and_then(|s| s.max_characters).unwrap_or(500)
				as usize;
		let poll_limits =
			info.configuration.as_ref().and_then(|c| c.polls.as_ref()).map(PollLimits::from_config).unwrap_or_default();
		Ok(InstanceInfo { max_post_chars: max_chars, poll_limits })
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

#[derive(Debug, Deserialize)]
struct MediaResponse {
	id: String,
}

#[derive(Debug, Deserialize)]
struct InstanceResponse {
	#[serde(default)]
	configuration: Option<InstanceConfiguration>,
}

#[derive(Debug, Deserialize)]
struct InstanceConfiguration {
	#[serde(default)]
	statuses: Option<StatusConfiguration>,
	#[serde(default)]
	polls: Option<PollConfiguration>,
}

#[derive(Debug, Deserialize)]
struct StatusConfiguration {
	#[serde(default)]
	max_characters: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct PollConfiguration {
	#[serde(default)]
	max_options: Option<u32>,
	#[serde(default)]
	max_option_chars: Option<u32>,
	#[serde(default)]
	min_expiration: Option<u32>,
	#[serde(default)]
	max_expiration: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct PollLimits {
	pub max_options: usize,
	pub max_option_chars: usize,
	pub min_expiration: u32,
	pub max_expiration: u32,
}

impl PollLimits {
	fn from_config(config: &PollConfiguration) -> Self {
		Self {
			max_options: config.max_options.unwrap_or(4) as usize,
			max_option_chars: config.max_option_chars.unwrap_or(50) as usize,
			min_expiration: config.min_expiration.unwrap_or(300),
			max_expiration: config.max_expiration.unwrap_or(2_629_746),
		}
	}
}

impl Default for PollLimits {
	fn default() -> Self {
		Self { max_options: 4, max_option_chars: 50, min_expiration: 300, max_expiration: 2_629_746 }
	}
}

#[derive(Debug, Clone)]
pub struct InstanceInfo {
	pub max_post_chars: usize,
	pub poll_limits: PollLimits,
}

impl Default for InstanceInfo {
	fn default() -> Self {
		Self { max_post_chars: 500, poll_limits: PollLimits::default() }
	}
}
