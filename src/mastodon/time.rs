//! Timestamp formatting shared by the API types.

use chrono::{DateTime, Local, Utc};
use chrono_humanize::HumanTime;

use crate::{clock::time_of_day, config::TimestampFormat};

pub(super) fn friendly_date(iso_time: &str) -> Option<String> {
	let trimmed = iso_time.trim();
	if trimmed.is_empty() {
		return None;
	}
	let parsed: DateTime<Utc> = trimmed.parse().ok()?;
	Some(parsed.format("%B %Y").to_string())
}

/// A day such as "2026-11-15", as "Nov 15, 2026".
pub(super) fn friendly_day(day: &str) -> Option<String> {
	let parsed = chrono::NaiveDate::parse_from_str(day.trim(), "%Y-%m-%d").ok()?;
	Some(parsed.format("%b %-d, %Y").to_string())
}

pub(super) fn friendly_time(iso_time: &str, format: TimestampFormat) -> Option<String> {
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
			Some(format!("{} at {}", local.format("%b %-d, %Y"), time_of_day(local)))
		}
	}
}

pub fn friendly_time_local(iso_time: &str) -> String {
	friendly_time(iso_time, TimestampFormat::Absolute).unwrap_or_else(|| iso_time.to_string())
}
