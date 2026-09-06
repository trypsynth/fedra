use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BuildTiming {
	pub started_at: i64,
	pub completed_at: Option<i64>,
	pub started_on_resume: bool,
	#[serde(default)]
	pub requests: Option<RequestCount>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RequestCount {
	pub attempts: u64,
	pub complete: bool,
}

impl BuildTiming {
	pub const fn new(started_at: i64, started_on_resume: bool) -> Self {
		Self {
			started_at,
			completed_at: None,
			started_on_resume,
			requests: Some(RequestCount { attempts: 0, complete: !started_on_resume }),
		}
	}

	pub fn record_request(&mut self) {
		let requests = self.requests.get_or_insert(RequestCount { attempts: 0, complete: false });
		requests.attempts = requests.attempts.saturating_add(1);
	}

	pub fn start_message(&self, account: &str) -> String {
		format!(
			"{} Autocomplete cache build {} for account {account:?}.{}",
			timestamp(self.started_at),
			if self.started_on_resume { "resumed" } else { "started" },
			if self.started_on_resume { " Original start time is unknown; timing begins at this resume." } else { "" }
		)
	}

	pub fn finish(&mut self, account: &str, completed_at: i64) -> Option<String> {
		if self.completed_at.is_some() {
			return None;
		}
		self.completed_at = Some(completed_at);
		let elapsed = completed_at.saturating_sub(self.started_at);
		let duration = if elapsed < 0 {
			"duration unavailable because the system clock moved backwards".into()
		} else {
			format!(
				"{} {}",
				if self.started_on_resume { "time since resume:" } else { "total duration:" },
				human_duration(elapsed.unsigned_abs())
			)
		};
		let requests = self.requests.as_ref().map_or_else(
			|| "API request count unavailable".into(),
			|requests| {
				if requests.complete {
					format!("API requests: {}", requests.attempts)
				} else {
					format!("API requests since tracking began: {} (earlier requests unknown)", requests.attempts)
				}
			},
		);
		Some(format!(
			"{} Autocomplete cache build completed for account {account:?}; started {}; {duration} (elapsed time includes pauses and retries); {requests} (includes failed attempts, retries, and relationship confirmations).",
			timestamp(completed_at),
			timestamp(self.started_at)
		))
	}
}

fn timestamp(unix: i64) -> String {
	chrono::DateTime::from_timestamp(unix, 0).map_or_else(
		|| format!("Unix timestamp {unix}"),
		|time| time.with_timezone(&chrono::Local).to_rfc3339_opts(chrono::SecondsFormat::Secs, false),
	)
}

fn human_duration(mut seconds: u64) -> String {
	let mut parts = Vec::new();
	for (unit, size) in [("day", 86400), ("hour", 3600), ("minute", 60), ("second", 1)] {
		let amount = seconds / size;
		seconds %= size;
		if amount > 0 {
			parts.push(format!("{amount} {unit}{}", if amount == 1 { "" } else { "s" }));
		}
	}
	if parts.is_empty() { "less than 1 second".into() } else { parts.join(" ") }
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn readable_durations_and_clock_changes() {
		for (seconds, expected) in [
			(0, "less than 1 second"),
			(1, "1 second"),
			(134, "2 minutes 14 seconds"),
			(3600, "1 hour"),
			(90061, "1 day 1 hour 1 minute 1 second"),
		] {
			assert_eq!(human_duration(seconds), expected);
		}
		let mut timing = BuildTiming::new(200, false);
		assert!(timing.finish("a", 199).unwrap().contains("system clock moved backwards"));
		assert!(timing.finish("a", 201).is_none());
	}

	#[test]
	fn older_timing_records_cannot_claim_a_full_request_count() {
		let mut timing: BuildTiming = serde_json::from_value(serde_json::json!({
			"started_at": 100, "completed_at": null, "started_on_resume": false
		}))
		.unwrap();
		timing.record_request();
		let saved = serde_json::to_vec(&timing).unwrap();
		let mut timing: BuildTiming = serde_json::from_slice(&saved).unwrap();
		timing.record_request();
		assert!(
			timing
				.finish("a", 234)
				.unwrap()
				.contains("API requests since tracking began: 2 (earlier requests unknown)")
		);
	}
}
