//! Accounts and the relationships between them.

use serde::Deserialize;

use crate::{
	config::{DisplayNameEmojiMode, TimestampFormat},
	html::strip_html,
	mastodon::{
		serde_util::deserialize_u64_or_zero,
		time::{friendly_date, friendly_day, friendly_time},
	},
	text::strip_display_name_emojis,
	timeline::TimelineTextOptions,
};

#[derive(Debug, Clone, Deserialize)]
#[allow(clippy::struct_excessive_bools)]
pub struct Relationship {
	pub id: String,
	pub following: bool,
	pub showing_reblogs: bool,
	pub notifying: bool,
	pub followed_by: bool,
	pub blocking: bool,
	pub muting: bool,
	pub requested: bool,
	#[serde(default)]
	pub requested_by: bool,
	pub domain_blocking: bool,
	/// Whether the mute also hides their notifications.
	#[serde(default)]
	pub muting_notifications: bool,
	#[serde(default)]
	pub blocked_by: bool,
	/// Whether the user features them on their own profile.
	#[serde(default)]
	pub endorsed: bool,
	pub note: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[allow(clippy::struct_excessive_bools, reason = "mirrors the fields the Mastodon API sends")]
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
	/// The roles the account's server shows on it, such as Moderator or Admin.
	#[serde(default)]
	pub roles: Vec<AccountRole>,
	#[serde(default)]
	pub group: bool,
	/// Kept in memory of someone who has died.
	#[serde(default)]
	pub memorial: bool,
	#[serde(default)]
	pub suspended: bool,
	/// Hidden by the moderators of the user's own server.
	#[serde(default)]
	pub limited: bool,
	/// The account it moved to, if it has.
	#[serde(default)]
	pub moved: Option<Box<Self>>,
	/// The day of its latest post, such as "2026-11-15".
	#[serde(default)]
	pub last_status_at: Option<String>,
	/// When the user's mute of it ends. Only the list of mutes gives it, and only for a mute that ends.
	#[serde(default)]
	pub mute_expires_at: Option<String>,
	/// The profile picture's alt text. Empty when there's none, or the server has no such thing.
	#[serde(default)]
	pub avatar_description: String,
	/// The header image's alt text.
	#[serde(default)]
	pub header_description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AccountRole {
	pub name: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct Source {
	pub privacy: Option<String>,
	pub sensitive: Option<bool>,
	pub language: Option<String>,
	/// Profile fields as the user typed them, rather than the rendered HTML in
	/// [`Account::fields`], where links get shortened for display.
	#[serde(default)]
	pub fields: Vec<AccountField>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AccountField {
	pub name: String,
	pub value: String,
	/// When the server confirmed that the link in the value links back to this profile.
	#[serde(default)]
	pub verified_at: Option<String>,
}

impl Account {
	pub fn display_name_or_username(&self) -> &str {
		if self.display_name.is_empty() { &self.username } else { &self.display_name }
	}

	pub fn full_acct(&self) -> String {
		if !self.acct.contains('@')
			&& let Ok(url) = reqwest::Url::parse(&self.url)
			&& let Some(host) = url.host_str()
		{
			return format!("{}@{}", self.acct, host);
		}
		self.acct.clone()
	}

	pub fn alias_key(&self) -> String {
		self.full_acct().to_lowercase()
	}

	pub fn timeline_display_name(&self, options: &TimelineTextOptions) -> String {
		if !options.user_aliases.is_empty()
			&& let Some(alias) = options.user_aliases.get(&self.alias_key())
		{
			return alias.clone();
		}
		let mode = options.display_name_emoji_mode;
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
		lines.extend(self.badges());
		lines.push(format!("Direct Profile URL: {}", self.url));
		lines.push(format!("Posts: {}", self.statuses_count));
		lines.push(format!("Following: {}", self.following_count));
		lines.push(format!("Followers: {}", self.followers_count));
		if !self.note.is_empty() {
			let bio = strip_html(&self.note);
			if !bio.trim().is_empty() {
				lines.push(format!("Bio: {bio}"));
			}
		}
		for (image, description) in
			[("Profile picture", &self.avatar_description), ("Header image", &self.header_description)]
		{
			if !description.trim().is_empty() {
				lines.push(format!("{image}: {}", description.trim()));
			}
		}
		if !self.fields.is_empty() {
			lines.push("Fields:".to_string());
			for field in &self.fields {
				let value = strip_html(&field.value);
				let verified = if field.verified_at.is_some() { " (verified)" } else { "" };
				lines.push(format!("\t{}: {value}{verified}", field.name));
			}
		}
		if !self.created_at.is_empty()
			&& let Some(date) = friendly_date(&self.created_at)
		{
			lines.push(format!("Joined: {date}"));
		}
		if let Some(date) = self.last_status_at.as_deref().and_then(friendly_day) {
			lines.push(format!("Last post: {date}"));
		}
		if let Some(until) = self.mute_expires_at.as_deref().and_then(|at| friendly_time(at, TimestampFormat::Absolute))
		{
			lines.push(format!("Muted until {until}"));
		}
		lines.join("\r\n")
	}

	/// What the web app shows beside the name: the account's roles, and whether it's a bot, a group, moved, a
	/// memorial, or held back by moderators. Up top, since they change how the rest reads.
	fn badges(&self) -> Vec<String> {
		let mut lines = Vec::new();
		let roles: Vec<&str> = self.roles.iter().map(|role| role.name.trim()).filter(|name| !name.is_empty()).collect();
		match roles.as_slice() {
			[] => {}
			[role] => lines.push(format!("Role: {role}")),
			roles => lines.push(format!("Roles: {}", roles.join(", "))),
		}
		if let Some(moved) = &self.moved {
			lines.push(format!("This account has moved to @{}.", moved.acct));
		}
		if self.memorial {
			lines.push("In memoriam.".to_string());
		}
		if self.suspended {
			lines.push("This account is suspended.".to_string());
		}
		if self.limited {
			lines.push("Your server's moderators have limited this account.".to_string());
		}
		if self.group {
			lines.push("This account is a group.".to_string());
		}
		if self.bot {
			lines.push("This account is a bot.".to_string());
		}
		if self.locked {
			lines.push("This account requires follow approval.".to_string());
		}
		lines
	}
}

#[cfg(test)]
mod tests {
	use super::Account;

	#[test]
	fn the_profile_shows_roles_moves_and_verified_links_up_top() {
		let account: Account = serde_json::from_value(serde_json::json!({
			"id": "1",
			"username": "jan",
			"acct": "jan",
			"display_name": "Jan",
			"url": "https://example.social/@jan",
			"roles": [{ "id": "3", "name": "Moderator", "color": "" }],
			"moved": { "id": "2", "username": "jan", "acct": "jan@new.example", "display_name": "", "url": "https://new.example/@jan" },
			"bot": true,
			"fields": [{ "name": "Site", "value": "<a href=\"https://jan.example\">jan.example</a>", "verified_at": "2026-01-01T00:00:00Z" }],
			"last_status_at": "2026-11-15",
		}))
		.unwrap();
		let profile = account.profile_display();
		let lines: Vec<&str> = profile.lines().collect();
		assert_eq!(
			&lines[..5],
			[
				"Name: Jan",
				"Username: @jan",
				"Role: Moderator",
				"This account has moved to @jan@new.example.",
				"This account is a bot."
			]
		);
		assert!(lines.contains(&"\tSite: jan.example (verified)"), "{profile}");
		assert!(lines.contains(&"Last post: Nov 15, 2026"), "{profile}");
	}
}
