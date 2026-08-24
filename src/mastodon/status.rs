//! Statuses and the values embedded in them.

use std::fmt::Write;

use serde::Deserialize;

use crate::{
	config::{ContentWarningDisplay, TimestampFormat},
	html::strip_html,
	mastodon::{
		Account, FilterAction, FilterContext, FilterResult, Poll, Tag, serde_util::deserialize_u64_or_zero,
		time::friendly_time,
	},
	template::{PostTemplateVars, render_template},
	timeline::TimelineTextOptions,
};

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct Quote {
	pub quoted_status: Option<Box<Status>>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct QuoteApproval {
	pub current_user: String,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct ScheduledStatus {
	pub id: String,
	pub scheduled_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum PostSubmission {
	Published(Box<Status>),
	Scheduled(ScheduledStatus),
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
	pub reblog: Option<Box<Self>>,
	pub quote: Option<Quote>,
	pub quote_approval: Option<QuoteApproval>,
	#[serde(default)]
	pub media_attachments: Vec<MediaAttachment>,
	pub application: Option<Application>,
	pub visibility: String,
	#[serde(default)]
	pub sensitive: bool,
	#[serde(default)]
	pub pinned: bool,
	#[serde(deserialize_with = "deserialize_u64_or_zero")]
	pub reblogs_count: u64,
	#[serde(deserialize_with = "deserialize_u64_or_zero")]
	pub favourites_count: u64,
	#[serde(deserialize_with = "deserialize_u64_or_zero")]
	pub replies_count: u64,
	#[serde(default)]
	pub favourited: bool,
	#[serde(default)]
	pub reblogged: bool,
	#[serde(default)]
	pub bookmarked: bool,
	#[serde(default)]
	pub conversation_id: Option<String>,
	pub in_reply_to_id: Option<String>,
	pub in_reply_to_account_id: Option<String>,
	#[serde(default)]
	pub language: Option<String>,
	#[serde(default)]
	pub mentions: Vec<Mention>,
	#[serde(default)]
	pub tags: Vec<Tag>,
	pub poll: Option<Poll>,
	#[serde(default)]
	pub card: Option<Card>,
	#[serde(default)]
	pub filtered: Vec<FilterResult>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct StatusSource {
	pub id: String,
	#[serde(default)]
	pub text: String,
	#[serde(default)]
	pub spoiler_text: String,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct Card {
	#[serde(default)]
	pub url: Option<String>,
	#[serde(default)]
	pub title: Option<String>,
	#[serde(default)]
	pub description: Option<String>,
	#[serde(default)]
	pub provider_name: Option<String>,
	#[serde(default)]
	pub author_name: Option<String>,
	#[serde(default, rename = "type")]
	pub kind: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct Mention {
	pub id: String,
	pub username: String,
	pub acct: String,
	pub url: String,
}

impl Mention {
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
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct Conversation {
	pub id: String,
	pub accounts: Vec<Account>,
	pub last_status: Option<Status>,
	pub unread: bool,
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
pub struct StatusContext {
	pub ancestors: Vec<Status>,
	pub descendants: Vec<Status>,
}

impl Status {
	pub fn display_text(&self) -> String {
		strip_html(&self.content)
	}

	pub fn simple_display(&self) -> String {
		let mut out = String::new();
		let content = self.content_with_cw(ContentWarningDisplay::Inline, true);
		if !content.is_empty() {
			out.push_str(&content);
		}
		if let Some(media) = self.media_summary(ContentWarningDisplay::Inline, true) {
			if !out.is_empty() {
				out.push(' ');
			}
			out.push_str(&media);
		}
		if let Some(poll_text) = self.poll_summary() {
			if !out.is_empty() {
				out.push(' ');
			}
			out.push_str(&poll_text);
		}
		out
	}

	pub fn should_hide(&self, filter_ctx: &FilterContext) -> bool {
		self.filtered.iter().any(|f| f.filter.action == FilterAction::Hide && f.filter.context.contains(filter_ctx))
	}

	pub fn matches_filter(&self, filter: &crate::config::TimelineFilter, current_user_id: Option<&str>) -> bool {
		let is_own_post = current_user_id.is_some_and(|uid| uid == self.account.id);
		let is_reply = self.in_reply_to_id.is_some();
		let is_boost = self.reblog.is_some();
		let is_quote = self.quote.is_some();
		let has_media = !self.media_attachments.is_empty();

		if is_boost {
			if !filter.boosts {
				return false;
			}
		} else if is_own_post {
			if is_reply {
				if !filter.your_replies {
					return false;
				}
			} else if !filter.your_posts {
				return false;
			}
		} else if is_reply {
			let replying_to_me = current_user_id.is_some_and(|uid| self.in_reply_to_account_id.as_deref() == Some(uid));
			let is_thread = self.in_reply_to_account_id.as_deref() == Some(&self.account.id);

			if replying_to_me {
				if !filter.replies_to_me {
					return false;
				}
			} else if is_thread {
				if !filter.threads {
					return false;
				}
			} else if !filter.replies_to_others {
				return false;
			}
		} else if !filter.original_posts {
			return false;
		}

		if is_quote && !filter.quote_posts {
			return false;
		}

		if has_media {
			if !filter.media_posts {
				return false;
			}
		} else if !filter.text_only_posts {
			return false;
		}

		true
	}

	fn filter_warning(&self, filter_ctx: &FilterContext) -> Option<String> {
		self.filtered
			.iter()
			.find(|f| f.filter.action == FilterAction::Warn && f.filter.context.contains(filter_ctx))
			.map(|f| f.filter.title.clone())
	}

	pub fn timeline_display(
		&self,
		options: &TimelineTextOptions,
		cw_expanded: bool,
		post_template: &str,
		boost_template: &str,
		quote_template: &str,
		filter_ctx: &FilterContext,
	) -> String {
		let text = self.reblog.as_ref().map_or_else(
			|| {
				let vars = self.build_template_vars(options, cw_expanded, filter_ctx);
				if self.quote.as_ref().is_some_and(|q| q.quoted_status.is_some()) {
					render_template(quote_template, &vars)
				} else {
					render_template(post_template, &vars)
				}
			},
			|boosted| {
				let mut vars = boosted.build_template_vars(options, cw_expanded, filter_ctx);
				vars.booster = self.account.timeline_display_name(options.display_name_emoji_mode);
				vars.booster_username = format!("@{}", self.account.acct);
				render_template(boost_template, &vars)
			},
		);
		if self.pinned { format!("Pinned: {text}") } else { text }
	}

	pub(crate) fn base_display(
		&self,
		options: &TimelineTextOptions,
		cw_expanded: bool,
		filter_ctx: &FilterContext,
	) -> String {
		let vars = self.build_template_vars(options, cw_expanded, filter_ctx);
		if self.quote.as_ref().is_some_and(|q| q.quoted_status.is_some()) {
			render_template(&options.quote_template, &vars)
		} else {
			render_template(&options.post_template, &vars)
		}
	}

	pub(crate) fn build_template_vars(
		&self,
		options: &TimelineTextOptions,
		cw_expanded: bool,
		filter_ctx: &FilterContext,
	) -> PostTemplateVars {
		let author = self.account.timeline_display_name(options.display_name_emoji_mode);
		let username = format!("@{}", self.account.acct);

		let filter_cw = self.filter_warning(filter_ctx);
		let (content_warning, is_filtered) =
			filter_cw.map_or_else(|| (self.spoiler_text.trim().to_string(), false), |fw| (fw, true));

		let mut content = if is_filtered {
			self.content_with_spoiler(options.cw_display, cw_expanded, &content_warning)
		} else {
			self.content_with_cw(options.cw_display, cw_expanded)
		};
		if options.show_link_previews
			&& let Some(card) = self.card_summary()
		{
			if !content.is_empty() {
				content.push(' ');
			}
			content.push_str(&card);
		}

		let relative_time = friendly_time(&self.created_at, TimestampFormat::Relative).unwrap_or_default();
		let absolute_time = friendly_time(&self.created_at, TimestampFormat::Absolute).unwrap_or_default();
		let visibility = self.visibility_display();
		let reply_count = count_label(self.replies_count, "reply", "replies");
		let boost_count = count_label(self.reblogs_count, "boost", "boosts");
		let favorite_count = count_label(self.favourites_count, "favorite", "favorites");
		let client = self.client_name().unwrap_or_default();
		let media = self.media_summary(options.cw_display, cw_expanded).unwrap_or_default();
		let poll = self.poll_summary().map_or_else(String::new, |p| format!(" {p}"));

		let (quote_author, quote_username, quote_content, quote_media, quote_poll) =
			self.quote.as_ref().and_then(|q| q.quoted_status.as_ref()).map_or_else(
				|| (String::new(), String::new(), String::new(), String::new(), String::new()),
				|quote| {
					if content.starts_with("RE: http") {
						if let Some(url_end) = content.find('\n') {
							content = content[url_end..].trim().to_string();
						} else if content.split_whitespace().count() <= 2 {
							content.clear();
						}
					}
					let author = quote.account.timeline_display_name(options.display_name_emoji_mode);
					let username = format!("@{}", quote.account.acct);
					let content = quote.content_with_cw(options.cw_display, cw_expanded);
					let media = quote
						.media_summary(options.cw_display, cw_expanded)
						.map(|s| format!(" {s}"))
						.unwrap_or_default();
					let poll = quote.poll_summary().map_or_else(String::new, |p| format!(" {p}"));
					(author, username, content, media, poll)
				},
			);

		PostTemplateVars {
			author,
			username,
			content,
			content_warning,
			relative_time,
			absolute_time,
			visibility,
			reply_count,
			boost_count,
			favorite_count,
			client,
			media,
			poll,
			booster: String::new(),
			booster_username: String::new(),
			quote_author,
			quote_username,
			quote_content,
			quote_media,
			quote_poll,
		}
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

	pub fn content_with_cw(&self, cw_display: ContentWarningDisplay, cw_expanded: bool) -> String {
		self.content_with_spoiler(cw_display, cw_expanded, self.spoiler_text.trim())
	}

	fn content_with_spoiler(&self, cw_display: ContentWarningDisplay, cw_expanded: bool, spoiler: &str) -> String {
		let content = self.display_text();
		if spoiler.is_empty() {
			return content;
		}
		match cw_display {
			ContentWarningDisplay::Inline => format!("Content warning: {spoiler} - {content}"),
			ContentWarningDisplay::Hidden => content,
			ContentWarningDisplay::WarningOnly => {
				if cw_expanded {
					content
				} else {
					format!("Content warning: {spoiler}")
				}
			}
		}
	}

	fn client_name(&self) -> Option<String> {
		self.application
			.as_ref()
			.map(|app| app.name.as_str())
			.filter(|name| !name.trim().is_empty())
			.map(std::string::ToString::to_string)
	}

	fn media_summary(&self, cw_display: ContentWarningDisplay, cw_expanded: bool) -> Option<String> {
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
		let mut summary = format!("media {count}");
		if !types.is_empty() {
			let _ = write!(summary, " ({types})");
		}
		if !alt_texts.is_empty() {
			let _ = write!(summary, " [{alt_texts}]");
		}
		if self.sensitive {
			match cw_display {
				ContentWarningDisplay::Inline => Some(format!("Sensitive media - {summary}")),
				ContentWarningDisplay::Hidden => Some(format!("{summary} (marked as sensitive)")),
				ContentWarningDisplay::WarningOnly => {
					if cw_expanded {
						Some(format!("{summary} (marked as sensitive)"))
					} else {
						Some(format!("{count} sensitive media"))
					}
				}
			}
		} else {
			Some(summary)
		}
	}

	fn poll_summary(&self) -> Option<String> {
		let poll = self.poll.as_ref()?;
		let show_results = poll.voted.unwrap_or(false) || poll.expired;

		if show_results {
			let total = poll.votes_count.max(1);
			let options: Vec<String> = poll
				.options
				.iter()
				.map(|opt| {
					let votes = opt.votes_count.unwrap_or(0);
					let pct = votes.saturating_mul(100).saturating_add(total / 2) / total;
					format!("{}: {}%", opt.title, pct)
				})
				.collect();
			Some(format!("[Poll Results: {}]", options.join(", ")))
		} else {
			let options: Vec<String> = poll.options.iter().map(|opt| opt.title.clone()).collect();
			Some(format!("[Poll: {}]", options.join(", ")))
		}
	}

	fn card_summary(&self) -> Option<String> {
		let card = self.card.as_ref()?;
		let title = card.title.as_deref().map(str::trim).filter(|s| !s.is_empty()).unwrap_or("Link preview");
		let mut summary = format!("[Preview: {title}");
		if let Some(provider) = card.provider_name.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
			let _ = write!(summary, " ({provider})");
		}
		if let Some(description) = card.description.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
			let _ = write!(summary, " - {description}");
		}
		summary.push(']');
		Some(summary)
	}
}

fn count_label(count: u64, singular: &str, plural: &str) -> String {
	if count == 0 {
		String::new()
	} else if count == 1 {
		format!("{count} {singular}")
	} else {
		format!("{count} {plural}")
	}
}
