//! Who follows the user back and who doesn't, worked out from the whole of both lists at once, since a page of
//! either says nothing about who's missing from the other.

use std::collections::HashSet;

use anyhow::Result;
use chrono::{Days, NaiveDate};
use wx_utils::progress::Progress;

use crate::mastodon::{Account, MastodonClient};

/// The views, in the order the dialog's choice lists them.
pub const VIEWS: [&str; 4] = [
	"People who follow you but you don't follow",
	"People you follow who don't follow you",
	"People you follow who follow you too",
	"People you follow who haven't posted in a year",
];

#[derive(Debug, Clone)]
pub struct FollowAnalysis {
	/// One list per entry of [`VIEWS`].
	pub views: [Vec<Account>; 4],
}

impl FollowAnalysis {
	/// Splits `followers` and `following` into the views. `today` decides what counts as a year without posting, and
	/// someone who has never posted counts too.
	pub fn new(followers: &[Account], following: &[Account], today: NaiveDate) -> Self {
		let follower_ids: HashSet<&str> = followers.iter().map(|a| a.id.as_str()).collect();
		let following_ids: HashSet<&str> = following.iter().map(|a| a.id.as_str()).collect();
		let year_ago = today.checked_sub_days(Days::new(365)).unwrap_or(today);
		let quiet = |account: &Account| {
			account
				.last_status_at
				.as_deref()
				.and_then(|day| NaiveDate::parse_from_str(day, "%Y-%m-%d").ok())
				.is_none_or(|day| day < year_ago)
		};
		let pick = |accounts: &[Account], keep: &dyn Fn(&Account) -> bool| -> Vec<Account> {
			accounts.iter().filter(|a| keep(a)).cloned().collect()
		};
		Self {
			views: [
				pick(followers, &|a| !following_ids.contains(a.id.as_str())),
				pick(following, &|a| !follower_ids.contains(a.id.as_str())),
				pick(following, &|a| follower_ids.contains(a.id.as_str())),
				pick(following, &quiet),
			],
		}
	}
}

/// Loads all of the user's followers, then everyone they follow, moving `progress` along as each page comes in.
/// Gives back the pair, or `None` once the user cancels.
pub fn load_follows(
	client: &MastodonClient,
	token: &str,
	progress: &Progress,
) -> Result<Option<(Vec<Account>, Vec<Account>)>> {
	let me = client.verify_credentials(token)?;
	let total = me.followers_count + me.following_count;
	let total = (total > 0).then_some(total);
	let mut done = 0;
	let mut lists = [Vec::new(), Vec::new()];
	for (i, message) in ["Loading your followers...", "Loading the people you follow..."].into_iter().enumerate() {
		progress.set_message(message);
		let mut max_id: Option<String> = None;
		loop {
			if progress.is_cancelled() {
				return Ok(None);
			}
			let (page, next) = if i == 0 {
				client.get_followers_page(token, &me.id, max_id.as_deref())?
			} else {
				client.get_following_page(token, &me.id, max_id.as_deref())?
			};
			done += page.len() as u64;
			progress.set(done, total);
			let finished = page.is_empty() || next.is_none();
			lists[i].extend(page);
			if finished {
				break;
			}
			max_id = next;
		}
	}
	Ok(Some(lists.into()))
}

#[cfg(test)]
mod tests {
	use super::*;

	fn account(id: &str, last_post: Option<&str>) -> Account {
		Account { id: id.to_string(), last_status_at: last_post.map(str::to_string), ..Account::default() }
	}

	fn ids(accounts: &[Account]) -> Vec<&str> {
		accounts.iter().map(|a| a.id.as_str()).collect()
	}

	#[test]
	fn splits_followers_and_follows_into_each_view() {
		let today = NaiveDate::from_ymd_opt(2026, 11, 17).unwrap();
		let followers = [account("fan", Some("2026-11-01")), account("friend", Some("2026-11-16"))];
		let following = [
			account("friend", Some("2026-11-16")),
			account("idol", Some("2026-10-01")),
			account("gone", Some("2025-06-01")),
			account("silent", None),
		];
		let analysis = FollowAnalysis::new(&followers, &following, today);
		assert_eq!(ids(&analysis.views[0]), ["fan"]);
		assert_eq!(ids(&analysis.views[1]), ["idol", "gone", "silent"]);
		assert_eq!(ids(&analysis.views[2]), ["friend"]);
		assert_eq!(ids(&analysis.views[3]), ["gone", "silent"]);
	}
}
