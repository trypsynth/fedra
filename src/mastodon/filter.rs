//! Server-side content filters.

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct FilterResult {
	pub filter: Filter,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct Filter {
	pub id: String,
	pub title: String,
	pub context: Vec<FilterContext>,
	#[serde(rename = "filter_action")]
	pub action: FilterAction,
	#[serde(default)]
	pub keywords: Vec<FilterKeyword>,
	pub expires_at: Option<String>,
}

impl Filter {
	/// Mirrors Mastodon's server-side keyword matching, for statuses fetched from servers that don't know our filters.
	/// `text` must already be lowercased.
	pub fn matches(&self, text: &str) -> bool {
		let expired = self
			.expires_at
			.as_deref()
			.and_then(|at| at.parse::<DateTime<Utc>>().ok())
			.is_some_and(|at| at <= Utc::now());
		!expired && self.keywords.iter().any(|kw| kw.matches(text))
	}
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct FilterKeyword {
	pub id: String,
	pub keyword: String,
	pub whole_word: bool,
}

impl FilterKeyword {
	fn matches(&self, text: &str) -> bool {
		let keyword = self.keyword.trim().to_lowercase();
		if keyword.is_empty() {
			return false;
		}
		if !self.whole_word {
			return text.contains(&keyword);
		}
		// Like Mastodon, only enforce a boundary on sides where the keyword itself ends in a word character, so "#tag" still matches.
		let check_start = keyword.starts_with(is_word_char);
		let check_end = keyword.ends_with(is_word_char);
		text.match_indices(&keyword).any(|(start, _)| {
			(!check_start || !text[..start].ends_with(is_word_char))
				&& (!check_end || !text[start + keyword.len()..].starts_with(is_word_char))
		})
	}
}

fn is_word_char(c: char) -> bool {
	c.is_alphanumeric() || c == '_'
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FilterContext {
	Home,
	Notifications,
	Public,
	Thread,
	Account,
	#[serde(other)]
	Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilterAction {
	Warn,
	Hide,
	Blur,
	Other(String),
}

impl<'de> serde::Deserialize<'de> for FilterAction {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: serde::Deserializer<'de>,
	{
		let s = String::deserialize(deserializer)?;
		match s.as_str() {
			"warn" => Ok(Self::Warn),
			"hide" => Ok(Self::Hide),
			"blur" => Ok(Self::Blur),
			_ => Ok(Self::Other(s)),
		}
	}
}

impl std::fmt::Display for FilterAction {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::Warn => write!(f, "Warn"),
			Self::Hide => write!(f, "Hide"),
			Self::Blur => write!(f, "Blur"),
			Self::Other(s) => write!(f, "{s}"),
		}
	}
}

impl std::fmt::Display for FilterContext {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::Home => write!(f, "Home and lists"),
			Self::Notifications => write!(f, "Notifications"),
			Self::Public => write!(f, "Public timelines"),
			Self::Thread => write!(f, "Conversations"),
			Self::Account => write!(f, "Profiles"),
			Self::Unknown => write!(f, "Unknown"),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::{Filter, FilterAction, FilterKeyword};

	fn filter(keyword: &str, whole_word: bool, expires_at: Option<&str>) -> Filter {
		Filter {
			id: "1".to_string(),
			title: "Test".to_string(),
			context: Vec::new(),
			action: FilterAction::Hide,
			keywords: vec![FilterKeyword { id: "1".to_string(), keyword: keyword.to_string(), whole_word }],
			expires_at: expires_at.map(str::to_string),
		}
	}

	#[test]
	fn matches_hashtags_case_insensitively() {
		let f = filter("#FreightFateRuns", true, None);
		assert!(f.matches("day 3 of #freightfateruns!"));
		assert!(!f.matches("day 3 of #freightfateruns2"));
		assert!(!f.matches("freightfateruns without the hash"));
	}

	#[test]
	fn respects_whole_word() {
		assert!(!filter("cat", true, None).matches("concatenate"));
		assert!(filter("cat", true, None).matches("a cat, sleeping"));
		assert!(filter("cat", false, None).matches("concatenate"));
	}

	#[test]
	fn ignores_expired_filters() {
		assert!(!filter("cat", false, Some("2000-01-01T00:00:00Z")).matches("cat"));
		assert!(filter("cat", false, Some("2999-01-01T00:00:00Z")).matches("cat"));
	}
}
