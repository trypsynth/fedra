//! Server-side content filters.

use anyhow::Result;
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

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct FilterKeyword {
	pub id: String,
	pub keyword: String,
	pub whole_word: bool,
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
