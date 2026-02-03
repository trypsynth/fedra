use anyhow::{Context, Result};
use chrono::{DateTime, Local, Utc};
use chrono_humanize::HumanTime;
use reqwest::{
	Url,
	blocking::{Client, multipart},
};
use serde::Deserialize;

use crate::{
	config::{ContentWarningDisplay, TimestampFormat},
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
	pub url: Option<String>,
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
	#[serde(default)]
	pub bookmarked: bool,
	pub in_reply_to_id: Option<String>,
	pub in_reply_to_account_id: Option<String>,
	#[serde(default)]
	pub mentions: Vec<Mention>,
	#[serde(default)]
	pub tags: Vec<Tag>,
	pub poll: Option<Poll>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct Poll {
	pub id: String,
	pub expires_at: Option<String>,
	pub expired: bool,
	pub multiple: bool,
	pub votes_count: u64,
	pub voters_count: Option<u64>,
	pub options: Vec<PollOption>,
	pub voted: Option<bool>,
	pub own_votes: Option<Vec<u32>>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct PollOption {
	pub title: String,
	pub votes_count: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct Mention {
	pub id: String,
	pub username: String,
	pub acct: String,
	pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct Tag {
	pub name: String,
	pub url: String,
	#[serde(default)]
	pub following: bool,
	#[serde(default)]
	pub history: Vec<TagHistory>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct TagHistory {
	pub day: String,
	pub uses: String,
	pub accounts: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SearchResults {
	pub accounts: Vec<Account>,
	pub statuses: Vec<Status>,
	pub hashtags: Vec<Tag>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SearchType {
	#[default]
	All,
	Accounts,
	Hashtags,
	Statuses,
}

impl SearchType {
	pub fn as_api_str(&self) -> Option<&'static str> {
		match self {
			SearchType::All => None,
			SearchType::Accounts => Some("accounts"),
			SearchType::Hashtags => Some("hashtags"),
			SearchType::Statuses => Some("statuses"),
		}
	}
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct Relationship {
	pub id: String,
	pub following: bool,
	pub showing_reblogs: bool,
	pub notifying: bool,
	pub followed_by: bool,
	pub blocking: bool,
	pub muting: bool,
	pub muting_notifications: bool,
	pub requested: bool,
	pub domain_blocking: bool,
	pub endorsed: bool,
	pub note: String,
}

impl Status {
	pub fn display_text(&self) -> String {
		strip_html(&self.content)
	}

	pub fn timeline_display(
		&self,
		timestamp_format: TimestampFormat,
		cw_display: ContentWarningDisplay,
		cw_expanded: bool,
	) -> String {
		match &self.reblog {
			Some(boosted) => {
				let booster = self.account.display_name_or_username();
				format!("{} boosted {}", booster, boosted.base_display(timestamp_format, cw_display, cw_expanded))
			}
			None => self.base_display(timestamp_format, cw_display, cw_expanded),
		}
	}

	pub fn details_display(
		&self,
		timestamp_format: TimestampFormat,
		cw_display: ContentWarningDisplay,
		cw_expanded: bool,
	) -> String {
		self.base_display(timestamp_format, cw_display, cw_expanded)
	}

	fn base_display(
		&self,
		timestamp_format: TimestampFormat,
		cw_display: ContentWarningDisplay,
		cw_expanded: bool,
	) -> String {
		let mut out = String::new();
		let author = self.account.display_name_or_username();
		out.push_str(author);
		out.push_str(": ");
		let content = self.content_with_cw(cw_display, cw_expanded);
		if !content.is_empty() {
			out.push_str(&content);
		}
		if let Some(media) = self.media_summary() {
			out.push_str(&media);
		}
		if let Some(poll_text) = self.poll_summary() {
			out.push_str(&format!(" {}", poll_text));
		}
		// Metadata line: time, visibility, client
		let mut meta = Vec::new();
		if let Some(when) = friendly_time(&self.created_at, timestamp_format) {
			meta.push(when);
		}
		meta.push(self.visibility_display());
		meta.push(format!("{} replies", self.replies_count));
		meta.push(format!("{} boosts", self.reblogs_count));
		meta.push(format!("{} favorites", self.favourites_count));
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
			"direct" => "Direct".to_string(),
			other => other.to_string(),
		}
	}

	fn content_with_cw(&self, cw_display: ContentWarningDisplay, cw_expanded: bool) -> String {
		let content = self.display_text();
		let spoiler = self.spoiler_text.trim();
		if spoiler.is_empty() {
			return content;
		}
		match cw_display {
			ContentWarningDisplay::Inline => format!("Content warning: {} - {}", spoiler, content),
			ContentWarningDisplay::Hidden => content,
			ContentWarningDisplay::WarningOnly => {
				if !cw_expanded {
					format!("Content warning: {}", spoiler)
				} else {
					content.to_string()
				}
			}
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

	fn poll_summary(&self) -> Option<String> {
		let poll = self.poll.as_ref()?;
		let show_results = poll.voted.unwrap_or(false) || poll.expired;

		if show_results {
			let total = poll.votes_count.max(1) as f64;
			let options: Vec<String> = poll
				.options
				.iter()
				.map(|opt| {
					let votes = opt.votes_count.unwrap_or(0);
					let pct = (votes as f64 / total * 100.0).round() as u64;
					format!("{}: {}%", opt.title, pct)
				})
				.collect();
			Some(format!("[Poll Results: {}]", options.join(", ")))
		} else {
			let options: Vec<String> = poll.options.iter().map(|opt| opt.title.clone()).collect();
			Some(format!("[Poll: {}]", options.join(", ")))
		}
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
pub struct Report {
	pub id: String,
	#[serde(default)]
	pub category: String,
	#[serde(default)]
	pub comment: String,
	pub target_account: Option<Account>,
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
	pub report: Option<Report>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StatusContext {
	pub ancestors: Vec<Status>,
	pub descendants: Vec<Status>,
}

impl Notification {
	pub fn timeline_display(
		&self,
		timestamp_format: TimestampFormat,
		cw_display: ContentWarningDisplay,
		cw_expanded: bool,
	) -> String {
		let actor = self.account.display_name_or_username();
		match self.kind.as_str() {
			"mention" | "status" => self.status_text(timestamp_format, cw_display, cw_expanded).to_string(),
			"reblog" => format!("{} boosted {}", actor, self.status_text(timestamp_format, cw_display, cw_expanded)),
			"favourite" => {
				format!("{} favorited {}", actor, self.status_text(timestamp_format, cw_display, cw_expanded))
			}
			"follow" => format!("{} followed you", actor),
			"follow_request" => format!("{} requested to follow you", actor),
			"poll" => format!("Poll ended: {}", self.status_text(timestamp_format, cw_display, cw_expanded)),
			"update" => {
				format!("{} edited {}", actor, self.status_text(timestamp_format, cw_display, cw_expanded))
			}
			"admin.sign_up" => format!("{} signed up", actor),
			"admin.report" => self.format_admin_report(actor),
			"severed_relationships" => "Some of your follow relationships have been severed".to_string(),
			"moderation_warning" => "You have received a moderation warning".to_string(),
			_ => match self.status_text_if_any(timestamp_format, cw_display, cw_expanded) {
				Some(text) => format!("{} {}: {}", actor, self.kind, text),
				None => format!("{} {}", actor, self.kind),
			},
		}
	}

	fn format_admin_report(&self, reporter: &str) -> String {
		match &self.report {
			Some(report) => {
				let target =
					report.target_account.as_ref().map(|a| a.display_name_or_username()).unwrap_or("unknown user");
				let category = match report.category.as_str() {
					"spam" => "spam",
					"legal" => "legal issue",
					"violation" => "rule violation",
					"other" => "other reason",
					"" => "unspecified reason",
					cat => cat,
				};
				if report.comment.is_empty() {
					format!("{} reported {} for {}", reporter, target, category)
				} else {
					format!("{} reported {} for {}: {}", reporter, target, category, report.comment)
				}
			}
			None => format!("{} filed a report", reporter),
		}
	}

	fn status_text(
		&self,
		timestamp_format: TimestampFormat,
		cw_display: ContentWarningDisplay,
		cw_expanded: bool,
	) -> String {
		self.status_text_if_any(timestamp_format, cw_display, cw_expanded)
			.unwrap_or_else(|| "No status content".to_string())
	}

	fn status_text_if_any(
		&self,
		timestamp_format: TimestampFormat,
		cw_display: ContentWarningDisplay,
		cw_expanded: bool,
	) -> Option<String> {
		self.status.as_ref().map(|status| status.details_display(timestamp_format, cw_display, cw_expanded))
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
	#[serde(default)]
	pub discoverable: Option<bool>,
	#[serde(default)]
	pub source: Option<Source>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Source {
	pub privacy: Option<String>,
	pub sensitive: Option<bool>,
	pub language: Option<String>,
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
		if !self.created_at.is_empty()
			&& let Some(date) = friendly_date(&self.created_at)
		{
			lines.push(format!("Joined: {}", date));
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
		in_reply_to_id: Option<&str>,
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
		if let Some(in_reply_to_id) = in_reply_to_id
			&& !in_reply_to_id.trim().is_empty()
		{
			params.push(("in_reply_to_id".to_string(), in_reply_to_id.to_string()));
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
		max_id: Option<&str>,
	) -> Result<Vec<Status>> {
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

	pub fn get_notifications(
		&self,
		access_token: &str,
		limit: Option<u32>,
		max_id: Option<&str>,
	) -> Result<Vec<Notification>> {
		let mut url = self.base_url.join("api/v1/notifications")?;
		{
			let mut query = url.query_pairs_mut();
			if let Some(limit) = limit {
				query.append_pair("limit", &limit.to_string());
			}
			if let Some(max_id) = max_id {
				query.append_pair("max_id", max_id);
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

	pub fn get_account(&self, access_token: &str, account_id: &str) -> Result<Account> {
		let url = self.base_url.join(&format!("api/v1/accounts/{}", account_id))?;
		let response = self
			.http
			.get(url)
			.bearer_auth(access_token)
			.send()
			.context("Failed to fetch account")?
			.error_for_status()
			.context("Instance rejected account request")?;
		let account: Account = response.json().context("Invalid account response")?;
		Ok(account)
	}

	pub fn get_status(&self, access_token: &str, status_id: &str) -> Result<Status> {
		let url = self.base_url.join(&format!("api/v1/statuses/{}", status_id))?;
		let response = self
			.http
			.get(url)
			.bearer_auth(access_token)
			.send()
			.context("Failed to fetch status")?
			.error_for_status()
			.context("Instance rejected status request")?;
		let status: Status = response.json().context("Invalid status response")?;
		Ok(status)
	}

	pub fn lookup_account(&self, access_token: &str, acct: &str) -> Result<Account> {
		let mut url = self.base_url.join("api/v1/accounts/lookup")?;
		url.query_pairs_mut().append_pair("acct", acct);
		let response = self
			.http
			.get(url)
			.bearer_auth(access_token)
			.send()
			.context("Failed to lookup account")?
			.error_for_status()
			.context("Instance rejected account lookup")?;
		let account: Account = response.json().context("Invalid account response")?;
		Ok(account)
	}

	pub fn favorite(&self, access_token: &str, status_id: &str) -> Result<Status> {
		let url = self.base_url.join(&format!("api/v1/statuses/{}/favourite", status_id))?;
		let response = self
			.http
			.post(url)
			.bearer_auth(access_token)
			.send()
			.context("Failed to favorite status")?
			.error_for_status()
			.context("Instance rejected favorite request")?;
		let status: Status = response.json().context("Invalid favorite response")?;
		Ok(status)
	}

	pub fn bookmark(&self, access_token: &str, status_id: &str) -> Result<Status> {
		let url = self.base_url.join(&format!("api/v1/statuses/{}/bookmark", status_id))?;
		let response = self
			.http
			.post(url)
			.bearer_auth(access_token)
			.send()
			.context("Failed to bookmark status")?
			.error_for_status()
			.context("Instance rejected bookmark request")?;
		let status: Status = response.json().context("Invalid bookmark response")?;
		Ok(status)
	}

	pub fn unfavorite(&self, access_token: &str, status_id: &str) -> Result<Status> {
		let url = self.base_url.join(&format!("api/v1/statuses/{}/unfavourite", status_id))?;
		let response = self
			.http
			.post(url)
			.bearer_auth(access_token)
			.send()
			.context("Failed to unfavorite status")?
			.error_for_status()
			.context("Instance rejected unfavorite request")?;
		let status: Status = response.json().context("Invalid unfavorite response")?;
		Ok(status)
	}

	pub fn unbookmark(&self, access_token: &str, status_id: &str) -> Result<Status> {
		let url = self.base_url.join(&format!("api/v1/statuses/{}/unbookmark", status_id))?;
		let response = self
			.http
			.post(url)
			.bearer_auth(access_token)
			.send()
			.context("Failed to unbookmark status")?
			.error_for_status()
			.context("Instance rejected unbookmark request")?;
		let status: Status = response.json().context("Invalid unbookmark response")?;
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

	pub fn get_status_context(&self, access_token: &str, status_id: &str) -> Result<StatusContext> {
		let url = self.base_url.join(&format!("api/v1/statuses/{}/context", status_id))?;
		let response = self
			.http
			.get(url)
			.bearer_auth(access_token)
			.send()
			.context("Failed to fetch status context")?
			.error_for_status()
			.context("Instance rejected status context request")?;
		let context: StatusContext = response.json().context("Invalid status context response")?;
		Ok(context)
	}

	pub fn follow_tag(&self, access_token: &str, tag_name: &str) -> Result<Tag> {
		let url = self.base_url.join(&format!("api/v1/tags/{}/follow", tag_name))?;
		let response = self
			.http
			.post(url)
			.bearer_auth(access_token)
			.send()
			.context("Failed to follow tag")?
			.error_for_status()
			.context("Instance rejected tag follow request")?;
		let tag: Tag = response.json().context("Invalid tag response")?;
		Ok(tag)
	}

	pub fn unfollow_tag(&self, access_token: &str, tag_name: &str) -> Result<Tag> {
		let url = self.base_url.join(&format!("api/v1/tags/{}/unfollow", tag_name))?;
		let response = self
			.http
			.post(url)
			.bearer_auth(access_token)
			.send()
			.context("Failed to unfollow tag")?
			.error_for_status()
			.context("Instance rejected tag unfollow request")?;
		let tag: Tag = response.json().context("Invalid tag response")?;
		Ok(tag)
	}

	pub fn get_tag(&self, access_token: &str, tag_name: &str) -> Result<Tag> {
		let url = self.base_url.join(&format!("api/v1/tags/{}", tag_name))?;
		let response = self
			.http
			.get(url)
			.bearer_auth(access_token)
			.send()
			.context("Failed to fetch tag info")?
			.error_for_status()
			.context("Instance rejected tag info request")?;
		let tag: Tag = response.json().context("Invalid tag response")?;
		Ok(tag)
	}

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
		let response = self
			.http
			.get(url)
			.bearer_auth(access_token)
			.send()
			.context("Failed to perform search")?
			.error_for_status()
			.context("Instance rejected search request")?;
		let results: SearchResults = response.json().context("Invalid search response")?;
		Ok(results)
	}

	pub fn get_relationships(&self, access_token: &str, account_ids: &[String]) -> Result<Vec<Relationship>> {
		let mut url = self.base_url.join("api/v1/accounts/relationships")?;
		{
			let mut query = url.query_pairs_mut();
			for id in account_ids {
				query.append_pair("id[]", id);
			}
		}
		let response = self
			.http
			.get(url)
			.bearer_auth(access_token)
			.send()
			.context("Failed to fetch relationships")?
			.error_for_status()
			.context("Instance rejected relationships request")?;
		let relationships: Vec<Relationship> = response.json().context("Invalid relationships response")?;
		Ok(relationships)
	}

	#[allow(dead_code)]
	pub fn follow_account(&self, access_token: &str, account_id: &str) -> Result<Relationship> {
		self.follow_account_with_options(access_token, account_id, true)
	}

	pub fn follow_account_with_options(
		&self,
		access_token: &str,
		account_id: &str,
		reblogs: bool,
	) -> Result<Relationship> {
		let url = self.base_url.join(&format!("api/v1/accounts/{}/follow", account_id))?;
		let response = self
			.http
			.post(url)
			.bearer_auth(access_token)
			.form(&[("reblogs", if reblogs { "true" } else { "false" })])
			.send()
			.context("Failed to follow account")?
			.error_for_status()
			.context("Instance rejected follow request")?;
		let relationship: Relationship = response.json().context("Invalid relationship response")?;
		Ok(relationship)
	}

	pub fn unfollow_account(&self, access_token: &str, account_id: &str) -> Result<Relationship> {
		let url = self.base_url.join(&format!("api/v1/accounts/{}/unfollow", account_id))?;
		let response = self
			.http
			.post(url)
			.bearer_auth(access_token)
			.send()
			.context("Failed to unfollow account")?
			.error_for_status()
			.context("Instance rejected unfollow request")?;
		let relationship: Relationship = response.json().context("Invalid relationship response")?;
		Ok(relationship)
	}

	pub fn block_account(&self, access_token: &str, account_id: &str) -> Result<Relationship> {
		let url = self.base_url.join(&format!("api/v1/accounts/{}/block", account_id))?;
		let response = self
			.http
			.post(url)
			.bearer_auth(access_token)
			.send()
			.context("Failed to block account")?
			.error_for_status()
			.context("Instance rejected block request")?;
		let relationship: Relationship = response.json().context("Invalid relationship response")?;
		Ok(relationship)
	}

	pub fn unblock_account(&self, access_token: &str, account_id: &str) -> Result<Relationship> {
		let url = self.base_url.join(&format!("api/v1/accounts/{}/unblock", account_id))?;
		let response = self
			.http
			.post(url)
			.bearer_auth(access_token)
			.send()
			.context("Failed to unblock account")?
			.error_for_status()
			.context("Instance rejected unblock request")?;
		let relationship: Relationship = response.json().context("Invalid relationship response")?;
		Ok(relationship)
	}

	pub fn mute_account(&self, access_token: &str, account_id: &str) -> Result<Relationship> {
		let url = self.base_url.join(&format!("api/v1/accounts/{}/mute", account_id))?;
		let response = self
			.http
			.post(url)
			.bearer_auth(access_token)
			.send()
			.context("Failed to mute account")?
			.error_for_status()
			.context("Instance rejected mute request")?;
		let relationship: Relationship = response.json().context("Invalid relationship response")?;
		Ok(relationship)
	}

	pub fn unmute_account(&self, access_token: &str, account_id: &str) -> Result<Relationship> {
		let url = self.base_url.join(&format!("api/v1/accounts/{}/unmute", account_id))?;
		let response = self
			.http
			.post(url)
			.bearer_auth(access_token)
			.send()
			.context("Failed to unmute account")?
			.error_for_status()
			.context("Instance rejected unmute request")?;
		let relationship: Relationship = response.json().context("Invalid relationship response")?;
		Ok(relationship)
	}

	pub fn vote_poll(&self, access_token: &str, poll_id: &str, choices: &[usize]) -> Result<Poll> {
		let url = self.base_url.join(&format!("api/v1/polls/{}/votes", poll_id))?;
		let mut params = Vec::new();
		for choice in choices {
			params.push(("choices[]", choice.to_string()));
		}
		let response = self
			.http
			.post(url)
			.bearer_auth(access_token)
			.form(&params)
			.send()
			.context("Failed to vote on poll")?
			.error_for_status()
			.context("Instance rejected vote request")?;
		let poll: Poll = response.json().context("Invalid poll response")?;
		Ok(poll)
	}

	pub fn delete_status(&self, access_token: &str, status_id: &str) -> Result<()> {
		let url = self.base_url.join(&format!("api/v1/statuses/{}", status_id))?;
		let _ = self
			.http
			.delete(url)
			.bearer_auth(access_token)
			.send()
			.context("Failed to delete status")?
			.error_for_status()
			.context("Instance rejected delete request")?;
		Ok(())
	}

	pub fn update_credentials(
		&self,
		access_token: &str,
		display_name: Option<&str>,
		note: Option<&str>,
		avatar: Option<&str>,
		header: Option<&str>,
		locked: Option<bool>,
		bot: Option<bool>,
		discoverable: Option<bool>,
		fields_attributes: Option<&[(String, String)]>,
		source_privacy: Option<&str>,
		source_sensitive: Option<bool>,
		source_language: Option<&str>,
	) -> Result<Account> {
		let url = self.base_url.join("api/v1/accounts/update_credentials")?;
		let mut form = multipart::Form::new();

		if let Some(v) = display_name {
			form = form.text("display_name", v.to_string());
		}
		if let Some(v) = note {
			form = form.text("note", v.to_string());
		}
		if let Some(v) = avatar {
			let part = multipart::Part::file(v).context("Failed to read avatar file")?;
			form = form.part("avatar", part);
		}
		if let Some(v) = header {
			let part = multipart::Part::file(v).context("Failed to read header file")?;
			form = form.part("header", part);
		}
		if let Some(v) = locked {
			form = form.text("locked", v.to_string());
		}
		if let Some(v) = bot {
			form = form.text("bot", v.to_string());
		}
		if let Some(v) = discoverable {
			form = form.text("discoverable", v.to_string());
		}
		if let Some(fields) = fields_attributes {
			for (i, (name, value)) in fields.iter().enumerate() {
				form = form.text(format!("fields_attributes[{}][name]", i), name.to_string());
				form = form.text(format!("fields_attributes[{}][value]", i), value.to_string());
			}
		}
		if let Some(v) = source_privacy {
			form = form.text("source[privacy]", v.to_string());
		}
		if let Some(v) = source_sensitive {
			form = form.text("source[sensitive]", v.to_string());
		}
		if let Some(v) = source_language {
			form = form.text("source[language]", v.to_string());
		}

		let response = self
			.http
			.patch(url)
			.bearer_auth(access_token)
			.multipart(form)
			.send()
			.context("Failed to update credentials")?
			.error_for_status()
			.context("Instance rejected credentials update")?;
		let account: Account = response.json().context("Invalid account response")?;
		Ok(account)
	}

	pub fn edit_status(
		&self,
		access_token: &str,
		status_id: &str,
		status: &str,
		spoiler_text: Option<&str>,
		media_ids: &[String],
		poll: Option<&crate::network::PollData>,
	) -> Result<Status> {
		let url = self.base_url.join(&format!("api/v1/statuses/{}", status_id))?;
		let mut params = vec![("status".to_string(), status.to_string())];
		if let Some(spoiler) = spoiler_text {
			params.push(("spoiler_text".to_string(), spoiler.to_string()));
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
		let response = self
			.http
			.put(url)
			.bearer_auth(access_token)
			.form(&params)
			.send()
			.context("Failed to edit status")?
			.error_for_status()
			.context("Instance rejected edit request")?;
		let status: Status = response.json().context("Invalid edit response")?;
		Ok(status)
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
