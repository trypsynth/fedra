//! @mention and #hashtag suggestions in the compose dialog: the people and hashtags in the current
//! timeline, then whatever the instance finds. The list itself comes from wordsmith.

use std::{rc::Rc, sync::Arc};

use wordsmith::completion::{Completer, Detail, Pool, Search, Speech, Suggestion, Trigger};
use wxdragon::prelude::*;

use crate::{
	AppState,
	mastodon::{Account, MastodonClient, SearchType, Status, Tag},
	speech,
	timeline::TimelineEntry,
	ui::commands::get_selected_entry,
};

const SEARCH_LIMIT: u32 = 10;

const MENTION: Trigger = Trigger {
	sigil: '@',
	name: "Mention",
	verb: "mentioned",
	is_query_char: is_handle_char,
	repeats: true,
	detail: Detail::Name,
};

const HASHTAG: Trigger = Trigger {
	sigil: '#',
	name: "Hashtag",
	verb: "added",
	is_query_char: |c| c.is_alphanumeric() || c == '_',
	repeats: false,
	detail: Detail::Note,
};

fn is_handle_char(c: char) -> bool {
	c.is_alphanumeric() || matches!(c, '_' | '.' | '-')
}

fn account_suggestion(account: &Account) -> Suggestion {
	Suggestion::new(account.full_acct(), account.display_name.trim())
}

fn tag_suggestion(tag: &Tag) -> Suggestion {
	Suggestion::new(tag.name.clone(), recent_users(tag).unwrap_or_default())
}

/// "12 people in the past 2 days", the count Mastodon itself shows for a tag, from the two newest days of its history.
fn recent_users(tag: &Tag) -> Option<String> {
	if tag.history.is_empty() {
		return None;
	}
	let people: u64 = tag.history.iter().take(2).filter_map(|day| day.accounts.parse::<u64>().ok()).sum();
	Some(if people == 1 {
		"1 person in the past 2 days".to_string()
	} else {
		format!("{people} people in the past 2 days")
	})
}

/// Where a compose dialog's suggestions come from, gathered before the dialog opens.
#[derive(Clone, Default)]
pub struct SuggestionSource {
	people: Vec<Suggestion>,
	tags: Vec<Suggestion>,
	/// Our own handle, lowercased, never suggested.
	self_acct: Option<String>,
	search: Option<(MastodonClient, String)>,
}

impl SuggestionSource {
	/// The people and hashtags in the timeline, and the instance to search for others.
	pub(crate) fn from_state(state: &AppState) -> Self {
		let search = state.client.clone().zip(state.access_token.clone());
		let (people, tags) = timeline_suggestions(state);
		Self { people, tags, self_acct: self_acct(state), search }
	}
}

/// Our own handle, lowercased and with the domain, to leave out of suggestions.
fn self_acct(state: &AppState) -> Option<String> {
	let account = state.active_account()?;
	let acct = account.acct.as_deref()?.trim().trim_start_matches('@');
	if acct.is_empty() {
		return None;
	}
	if acct.contains('@') {
		return Some(acct.to_lowercase());
	}
	let host = reqwest::Url::parse(&account.instance).ok()?.host_str()?.to_string();
	Some(format!("{acct}@{host}").to_lowercase())
}

/// Everyone in the active timeline but us - the selected entry's people first, then authors,
/// boosters and people mentioned, newest first. Also the user lookup dialog's suggestions.
pub fn timeline_people(state: &AppState) -> Vec<Suggestion> {
	timeline_suggestions(state).0
}

/// The active timeline's people, leaving us out, and its hashtags, each starting with the selected entry's.
fn timeline_suggestions(state: &AppState) -> (Vec<Suggestion>, Vec<Suggestion>) {
	let mut people = Pool::default();
	let mut tags = Pool::default();
	if let Some(entry) = get_selected_entry(state) {
		add_entry(&mut people, &mut tags, entry);
	}
	for entry in state.timeline_manager.active().map(|t| t.entries.as_slice()).unwrap_or_default() {
		add_entry(&mut people, &mut tags, entry);
	}
	let me = self_acct(state);
	let mut people = people.into_vec();
	people.retain(|s| me.as_deref().is_none_or(|me| !s.text.eq_ignore_ascii_case(me)));
	(people, tags.into_vec())
}

fn add_entry(people: &mut Pool, tags: &mut Pool, entry: &TimelineEntry) {
	match entry {
		TimelineEntry::Status(status) => add_status(people, tags, status),
		TimelineEntry::Notification(notification) => {
			people.add(account_suggestion(&notification.account));
			if let Some(status) = &notification.status {
				add_status(people, tags, status);
			}
		}
		TimelineEntry::Account(account) => people.add(account_suggestion(account)),
		TimelineEntry::Hashtag(tag) => tags.add(tag_suggestion(tag)),
	}
}

fn add_status(people: &mut Pool, tags: &mut Pool, status: &Status) {
	people.add(account_suggestion(&status.account));
	if let Some(reblog) = &status.reblog {
		add_status(people, tags, reblog);
	}
	for mention in &status.mentions {
		people.add(Suggestion::new(mention.full_acct(), ""));
	}
	for tag in &status.tags {
		tags.add(tag_suggestion(tag));
	}
}

fn speak(text: &str, how: Speech) {
	match how {
		Speech::Interrupt => speech::speak(text),
		Speech::Queue => speech::speak_queued(text),
	}
}

/// Offers mentions and hashtags in `field`. Stop the returned timer once the dialog closes.
pub fn completer(
	dialog: Dialog,
	panel: Panel,
	field: TextCtrl,
	list: ListBox,
	source: SuggestionSource,
) -> (Completer, Timer<Dialog>) {
	let SuggestionSource { people, tags, self_acct, search } = source;
	let search = search.map(|(client, token)| -> Search {
		Arc::new(move |trigger, query| {
			// A failed search just leaves the timeline's suggestions.
			if trigger == 0 {
				let accounts = client.search_accounts(&token, query, SEARCH_LIMIT).unwrap_or_default();
				accounts
					.iter()
					.map(account_suggestion)
					.filter(|s| self_acct.as_deref().is_none_or(|me| !s.text.eq_ignore_ascii_case(me)))
					.collect()
			} else {
				client
					.search(&token, query, SearchType::Hashtags, Some(SEARCH_LIMIT), None)
					.map(|results| results.hashtags.iter().map(tag_suggestion).collect())
					.unwrap_or_default()
			}
		})
	});
	Completer::new(&dialog, panel, field, list, vec![(MENTION, people), (HASHTAG, tags)], search, Rc::new(speak))
}
