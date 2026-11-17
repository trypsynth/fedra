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
pub use client::{AppCredentials, DEFAULT_SCOPES, Markers, MastodonClient};
pub use filter::{Filter, FilterAction, FilterContext, FilterKeyword, FilterResult};
pub use instance::{DEFAULT_MAX_MEDIA_DESCRIPTION_CHARS, InstanceInfo};
pub use list::List;
pub use notification::{Notification, NotificationPolicy, NotificationRequest};
pub use poll::{Poll, PollLimits};
pub use search::{SearchResults, SearchType};
pub use status::{
	Conversation, MediaAttachment, Mention, PostSubmission, ScheduledStatus, Status, StatusAction, StatusContext,
	StatusSource, StatusVersion,
};
pub use tag::Tag;
pub use time::friendly_time_local;

/// Sent with every request Fedra makes. Some firewalls in front of instances, such as Prot.li,
/// turn away a short one, or none at all.
pub const USER_AGENT: &str = concat!("Fedra/", env!("CARGO_PKG_VERSION"), " (+https://github.com/trypsynth/fedra)");
