//! Accounts and the relationships between them.

use serde::Deserialize;

use crate::{
	config::DisplayNameEmojiMode,
	html::strip_html,
	mastodon::{serde_util::deserialize_u64_or_zero, time::friendly_date},
	text::strip_display_name_emojis,
};

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
#[allow(clippy::struct_excessive_bools)]
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
	#[serde(default)]
	pub requested_by: bool,
	pub domain_blocking: bool,
	pub endorsed: bool,
	pub note: String,
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
	#[serde(default, deserialize_with = "deserialize_u64_or_zero")]
	pub followers_count: u64,
	#[serde(default, deserialize_with = "deserialize_u64_or_zero")]
	pub following_count: u64,
	#[serde(default, deserialize_with = "deserialize_u64_or_zero")]
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

	pub fn full_acct(&self) -> String {
		if self.acct.contains('@') {
			self.acct.clone()
		} else {
			if let Ok(url) = reqwest::Url::parse(&self.url) {
				if let Some(host) = url.host_str() {
					return format!("{}@{}", self.acct, host);
				}
			}
			self.acct.clone()
		}
	}

	pub fn timeline_display_name(&self, mode: DisplayNameEmojiMode) -> String {
		if mode == DisplayNameEmojiMode::None {
			return self.display_name_or_username().to_string();
		}
		let filtered_display = strip_display_name_emojis(&self.display_name, mode);
		if !filtered_display.is_empty() {
			return filtered_display;
		}
		let filtered_username = strip_display_name_emojis(&self.username, mode);
		if !filtered_username.is_empty() {
			return filtered_username;
		}
		self.display_name_or_username().to_string()
	}

	pub fn profile_display(&self) -> String {
		let mut lines = Vec::new();
		let name = self.display_name_or_username();
		lines.push(format!("Name: {name}"));
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
				lines.push(format!("Bio: {bio}"));
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
			lines.push(format!("Joined: {date}"));
		}
		lines.join("\r\n")
	}
}
