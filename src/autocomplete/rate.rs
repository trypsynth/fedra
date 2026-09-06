//! Shared request budget for foreground clients and the background cache worker.
use std::{
	collections::HashMap,
	sync::{
		Mutex, OnceLock,
		atomic::{AtomicUsize, Ordering},
	},
	time::{Duration, Instant},
};

use reqwest::blocking::Response;

pub static FOREGROUND: AtomicUsize = AtomicUsize::new(0);
static BUDGETS: OnceLock<Mutex<HashMap<String, Budget>>> = OnceLock::new();

#[derive(Clone, Copy, Default)]
struct Budget {
	limit: Option<u64>,
	remaining: Option<u64>,
	reset: Option<Instant>,
	retry: Option<Instant>,
	retry_explicit: bool,
}

pub struct ForegroundGuard;
pub fn foreground() -> ForegroundGuard {
	FOREGROUND.fetch_add(1, Ordering::SeqCst);
	ForegroundGuard
}
impl Drop for ForegroundGuard {
	fn drop(&mut self) {
		FOREGROUND.fetch_sub(1, Ordering::SeqCst);
	}
}

pub fn observe(response: &Response) {
	let mut budgets = BUDGETS.get_or_init(Mutex::default).lock().unwrap();
	let budget = budgets.entry(response.url().origin().ascii_serialization()).or_default();
	let headers = response.headers();
	let value = |name| headers.get(name).and_then(|h| h.to_str().ok());
	if let Some(limit) = value("x-ratelimit-limit").and_then(|s| s.parse().ok()) {
		budget.limit = Some(limit);
	}
	if let Some(remaining) = value("x-ratelimit-remaining").and_then(|s| s.parse().ok()) {
		budget.remaining = Some(remaining);
	}
	if let Some(reset) = value("x-ratelimit-reset").and_then(deadline) {
		budget.reset = Some(reset);
	}
	if response.status().as_u16() == 429 {
		budget.retry = value("retry-after").and_then(|s| {
			s.parse::<u64>()
				.ok()
				.and_then(|n| Instant::now().checked_add(Duration::from_secs(n)))
				.or_else(|| deadline(s))
		});
		budget.retry = budget.retry.max(budget.reset);
		budget.retry_explicit = budget.retry.is_some_and(|d| d > Instant::now());
		budget.retry =
			budget.retry.filter(|d| *d > Instant::now()).or_else(|| Some(Instant::now() + Duration::from_secs(30)));
	}
	drop(budgets);
}

fn deadline(value: &str) -> Option<Instant> {
	let date =
		chrono::DateTime::parse_from_rfc3339(value).or_else(|_| chrono::DateTime::parse_from_rfc2822(value)).ok()?;
	let remaining = date.signed_duration_since(chrono::Utc::now()).to_std().unwrap_or(Duration::ZERO);
	Instant::now().checked_add(remaining)
}

pub fn reported_retry(origin: &str) -> Option<Instant> {
	let budget = *BUDGETS.get_or_init(Mutex::default).lock().unwrap().get(origin)?;
	if budget.retry_explicit { budget.retry } else { None }
}

pub fn defer_until(origin: &str, now: Instant) -> Option<Instant> {
	let budgets = BUDGETS.get_or_init(Mutex::default).lock().unwrap();
	let budget = *budgets.get(origin)?;
	drop(budgets);
	let reserve = budget.limit.map_or(10, |limit| 10.max(limit.div_ceil(10)).min(limit.saturating_sub(1)));
	let low = budget.remaining.is_some_and(|n| n <= reserve);
	budget.retry.filter(|d| *d > now).max(if low { budget.reset.filter(|d| *d > now) } else { None })
}

pub fn backoff(attempt: u32) -> Duration {
	let seconds = 30_u64.saturating_mul(1_u64 << attempt.min(4)).min(300);
	let jitter = u64::from(chrono::Utc::now().timestamp_subsec_millis()) % 4;
	Duration::from_secs((seconds + jitter).min(300))
}
