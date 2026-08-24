//! Types and HTTP client for the Mastodon API.

mod account;
mod client;
mod filter;
mod instance;
mod list;
mod notification;
mod poll;
mod search;
mod serde_util;
mod status;
mod tag;
mod time;

pub use account::{Account, Relationship, Source};
pub use client::{AppCredentials, DEFAULT_SCOPES, MastodonClient};
pub use filter::{Filter, FilterAction, FilterContext, FilterKeyword, FilterResult};
pub use instance::InstanceInfo;
pub use list::List;
pub use notification::Notification;
pub use poll::{Poll, PollLimits};
pub use search::{SearchResults, SearchType};
pub use status::{Conversation, Mention, PostSubmission, Status, StatusContext, StatusSource};
pub use tag::Tag;
pub use time::friendly_time_local;
