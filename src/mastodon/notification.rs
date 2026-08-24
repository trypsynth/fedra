//! Notifications and reports.

use serde::Deserialize;

use crate::{
	config::DisplayNameEmojiMode,
	mastodon::{Account, Status},
	template::render_template,
	timeline::TimelineTextOptions,
};

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

impl Notification {
	pub fn simple_display(&self) -> String {
		match self.kind.as_str() {
			"mention" | "status" => {
				self.status.as_ref().map_or_else(|| "No content".to_string(), |s| s.simple_display())
			}
			"reblog" => self.status.as_ref().map_or_else(
				|| "boosted a post".to_string(),
				|status| format!("boosted {}: {}", status.account.display_name_or_username(), status.simple_display()),
			),
			"favourite" => self.status.as_ref().map_or_else(
				|| "favorited a post".to_string(),
				|status| {
					format!("favorited {}: {}", status.account.display_name_or_username(), status.simple_display())
				},
			),
			"follow" => "followed you".to_string(),
			"follow_request" => "requested to follow you".to_string(),
			"poll" => self
				.status
				.as_ref()
				.map_or_else(|| "Poll ended".to_string(), |status| format!("Poll ended: {}", status.simple_display())),
			"update" => "edited a post".to_string(),
			_ => self.kind.clone(),
		}
	}

	pub fn timeline_display(&self, options: &TimelineTextOptions, cw_expanded: bool) -> String {
		let actor = self.account.timeline_display_name(options.display_name_emoji_mode);
		match self.kind.as_str() {
			"mention" | "status" => self.status_text(options, cw_expanded),
			"reblog" => self.status.as_ref().map_or_else(
				|| format!("{actor} boosted a post"),
				|status| {
					let mut vars = status.build_template_vars(options, cw_expanded, &options.filter_context);
					vars.booster.clone_from(&actor);
					vars.booster_username = format!("@{}", self.account.acct);
					render_template(&options.boost_template, &vars)
				},
			),
			"favourite" => {
				format!("{} favorited {}", actor, self.status_text(options, cw_expanded))
			}
			"follow" => format!("{actor} followed you"),
			"follow_request" => format!("{actor} requested to follow you"),
			"poll" => format!("Poll ended: {}", self.status_text(options, cw_expanded)),
			"update" => format!("{} edited {}", actor, self.status_text(options, cw_expanded)),
			"admin.sign_up" => format!("{actor} signed up"),
			"admin.report" => self.format_admin_report(&actor, options.display_name_emoji_mode),
			"severed_relationships" => "Some of your follow relationships have been severed".to_string(),
			"moderation_warning" => "You have received a moderation warning".to_string(),
			_ => self.status_text_if_any(options, cw_expanded).map_or_else(
				|| format!("{} {}", actor, self.kind),
				|text| format!("{} {}: {}", actor, self.kind, text),
			),
		}
	}

	pub fn matches_filter(&self, filter: &crate::config::TimelineFilter, current_user_id: Option<&str>) -> bool {
		match self.kind.as_str() {
			"mention" | "status" => {
				self.status.as_ref().is_none_or(|status| status.matches_filter(filter, current_user_id))
			}
			"reblog" => filter.boosts,
			_ => true,
		}
	}

	fn format_admin_report(&self, reporter: &str, display_name_emoji_mode: DisplayNameEmojiMode) -> String {
		self.report.as_ref().map_or_else(
			|| format!("{reporter} filed a report"),
			|report| {
				let target = report.target_account.as_ref().map_or_else(
					|| "unknown user".to_string(),
					|account| account.timeline_display_name(display_name_emoji_mode),
				);
				let category = match report.category.as_str() {
					"spam" => "spam",
					"legal" => "legal issue",
					"violation" => "rule violation",
					"other" => "other reason",
					"" => "unspecified reason",
					cat => cat,
				};
				if report.comment.is_empty() {
					format!("{reporter} reported {target} for {category}")
				} else {
					format!("{} reported {} for {}: {}", reporter, target, category, report.comment)
				}
			},
		)
	}

	fn status_text(&self, options: &TimelineTextOptions, cw_expanded: bool) -> String {
		self.status_text_if_any(options, cw_expanded).unwrap_or_else(|| "No status content".to_string())
	}

	fn status_text_if_any(&self, options: &TimelineTextOptions, cw_expanded: bool) -> Option<String> {
		self.status.as_ref().map(|status| status.base_display(options, cw_expanded, &options.filter_context))
	}
}
