//! Search results.

use serde::{Deserialize, Serialize};

use crate::mastodon::{Account, Status, Tag};

#[derive(Debug, Clone, Deserialize)]
pub struct SearchResults {
	pub accounts: Vec<Account>,
	pub statuses: Vec<Status>,
	pub hashtags: Vec<Tag>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SearchType {
	#[default]
	All,
	Accounts,
	Hashtags,
	Statuses,
}

impl SearchType {
	pub const fn as_api_str(self) -> Option<&'static str> {
		match self {
			Self::All => None,
			Self::Accounts => Some("accounts"),
			Self::Hashtags => Some("hashtags"),
			Self::Statuses => Some("statuses"),
		}
	}
}
