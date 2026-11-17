//! @mention and #hashtag autocomplete in the compose dialog, shaped like an editor's completion
//! list: typing "@" offers the people in the current timeline and "#" its hashtags, typing on
//! narrows it and asks the instance for more, Up/Down pick, Enter or Tab insert "@user@domain " or
//! "#tag ", Escape dismisses and Ctrl+Space brings the list back.
//!
//! Focus never leaves the post text - a list that took focus would break typing - so the
//! highlighted suggestion is spoken, and the list under the text is only there for sighted
//! users, who can also click a suggestion.

use std::{
	cell::RefCell,
	collections::{HashMap, HashSet},
	rc::Rc,
	sync::mpsc::{self, Receiver, Sender},
	thread,
	time::{Duration, Instant},
};

use wxdragon::prelude::*;

use crate::{
	AppState,
	mastodon::{Account, MastodonClient, SearchType, Status, Tag},
	speech,
	timeline::TimelineEntry,
	ui::{commands::get_selected_entry, keys},
};

/// A handle can be long ("someone@some.very.long.instance.example"), but a run of handle
/// characters longer than this after an "@" isn't one.
const MAX_QUERY_CHARS: usize = 80;
/// How many suggestions the list holds at most.
const MAX_SUGGESTIONS: usize = 50;
/// How long typing has to pause before the instance is asked for matches.
const SEARCH_DELAY: Duration = Duration::from_millis(300);
const SEARCH_LIMIT: u32 = 10;
/// How often search results are collected and a due search is started.
const POLL_INTERVAL_MS: i32 = 100;

/// What's being completed, by the character that starts it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Kind {
	Mention,
	Hashtag,
}

impl Kind {
	const fn sigil(self) -> char {
		match self {
			Self::Mention => '@',
			Self::Hashtag => '#',
		}
	}

	const fn of(c: char) -> Option<Self> {
		match c {
			'@' => Some(Self::Mention),
			'#' => Some(Self::Hashtag),
			_ => None,
		}
	}
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Suggestion {
	kind: Kind,
	/// What goes after the "@" or "#": the full handle, "user@domain", or the tag's name.
	pub text: String,
	/// The display name, or for a hashtag how many people use it. Empty when only the handle or name is known.
	detail: String,
}

impl Suggestion {
	fn from_account(account: &Account) -> Self {
		Self { kind: Kind::Mention, text: account.full_acct(), detail: account.display_name.trim().to_string() }
	}

	fn from_tag(tag: &Tag) -> Self {
		Self { kind: Kind::Hashtag, text: tag.name.clone(), detail: recent_users(tag).unwrap_or_default() }
	}

	fn inserted(&self) -> String {
		format!("{}{}", self.kind.sigil(), self.text)
	}

	fn label(&self) -> String {
		match (self.kind, self.detail.is_empty()) {
			(_, true) => self.inserted(),
			(Kind::Mention, false) => format!("{} ({})", self.detail, self.inserted()),
			(Kind::Hashtag, false) => format!("{} ({})", self.inserted(), self.detail),
		}
	}

	fn spoken(&self) -> String {
		match (self.kind, self.detail.is_empty()) {
			(_, true) => self.inserted(),
			(Kind::Mention, false) => format!("{}, {}", self.detail, self.inserted()),
			(Kind::Hashtag, false) => format!("{}, {}", self.inserted(), self.detail),
		}
	}
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
		Self { people: people.list, tags: tags.list, self_acct: self_acct(state), search }
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
	timeline_suggestions(state).0.list
}

/// The active timeline's people, leaving us out, and its hashtags, each starting with the selected entry's.
fn timeline_suggestions(state: &AppState) -> (Pool, Pool) {
	let mut people = Pool::default();
	let mut tags = Pool::default();
	if let Some(entry) = get_selected_entry(state) {
		add_entry(&mut people, &mut tags, entry);
	}
	for entry in state.timeline_manager.active().map(|t| t.entries.as_slice()).unwrap_or_default() {
		add_entry(&mut people, &mut tags, entry);
	}
	let me = self_acct(state);
	people.list.retain(|c| me.as_deref().is_none_or(|me| !c.text.eq_ignore_ascii_case(me)));
	(people, tags)
}

fn add_entry(people: &mut Pool, tags: &mut Pool, entry: &TimelineEntry) {
	match entry {
		TimelineEntry::Status(status) => add_status(people, tags, status),
		TimelineEntry::Notification(notification) => {
			people.add(Suggestion::from_account(&notification.account));
			if let Some(status) = &notification.status {
				add_status(people, tags, status);
			}
		}
		TimelineEntry::Account(account) => people.add(Suggestion::from_account(account)),
		TimelineEntry::Hashtag(tag) => tags.add(Suggestion::from_tag(tag)),
	}
}

fn add_status(people: &mut Pool, tags: &mut Pool, status: &Status) {
	people.add(Suggestion::from_account(&status.account));
	if let Some(reblog) = &status.reblog {
		add_status(people, tags, reblog);
	}
	for mention in &status.mentions {
		people.add(Suggestion { kind: Kind::Mention, text: mention.full_acct(), detail: String::new() });
	}
	for tag in &status.tags {
		tags.add(Suggestion::from_tag(tag));
	}
}

/// Suggestions without repeats, in the order first seen.
#[derive(Default)]
struct Pool {
	list: Vec<Suggestion>,
	index: HashMap<String, usize>,
}

impl Pool {
	fn add(&mut self, suggestion: Suggestion) {
		let key = suggestion.text.to_lowercase();
		if let Some(&i) = self.index.get(&key) {
			// A mention only gave the handle and a post only the tag's name; an author or a search seen later gives more.
			if self.list[i].detail.is_empty() {
				self.list[i].detail = suggestion.detail;
			}
			return;
		}
		self.index.insert(key, self.list.len());
		self.list.push(suggestion);
	}
}

struct Popup {
	kind: Kind,
	/// Char index of the "@" or "#" being completed.
	at: usize,
	matches: Vec<Suggestion>,
	selected: usize,
}

type Answer = (Kind, String, Vec<Suggestion>);

struct Inner {
	/// The timeline's people plus everyone a search has turned up.
	people: Pool,
	/// The timeline's hashtags plus every one a search has turned up.
	tags: Pool,
	self_acct: Option<String>,
	search: Option<(MastodonClient, String)>,
	/// The instance's answers, by kind and lowercased query.
	results: HashMap<(Kind, String), Vec<Suggestion>>,
	requested: HashSet<(Kind, String)>,
	/// A search to start once typing pauses.
	due: Option<(Kind, String, Instant)>,
	popup: Option<Popup>,
	/// The "@" or "#" whose suggestions were dismissed with Escape or just accepted, so they stay
	/// closed while the caret is still on it.
	dismissed_at: Option<usize>,
	results_tx: Sender<Answer>,
	results_rx: Receiver<Answer>,
}

impl Inner {
	fn is_self(&self, suggestion: &Suggestion) -> bool {
		suggestion.kind == Kind::Mention
			&& self.self_acct.as_deref().is_some_and(|me| suggestion.text.eq_ignore_ascii_case(me))
	}

	/// The timeline's matches for `query` first, then whatever the instance found for it.
	fn matches(&self, kind: Kind, query: &str) -> Vec<Suggestion> {
		let pool = match kind {
			Kind::Mention => &self.people,
			Kind::Hashtag => &self.tags,
		};
		let mut seen = HashSet::new();
		let exact = self.results.get(&(kind, query.to_string())).into_iter().flatten();
		pool.list
			.iter()
			.filter(|c| matches_query(c, query))
			.chain(exact)
			.filter(|c| !self.is_self(c) && seen.insert(c.text.to_lowercase()))
			.take(MAX_SUGGESTIONS)
			.cloned()
			.collect()
	}
}

/// The "@" or "#" the caret is completing, as (what it starts, its char index, text typed after
/// it). It has to start a word, so an email address or "C#" doesn't count, and neither holds a
/// space.
fn find_query(text: &str, caret: usize) -> Option<(Kind, usize, String)> {
	let chars: Vec<char> = text.chars().collect();
	let caret = caret.min(chars.len());
	let mut i = caret;
	while i > 0 {
		i -= 1;
		let starts_word = i == 0 || !is_handle_char(chars[i - 1]);
		match Kind::of(chars[i]) {
			// The "@" in "user@domain" follows a handle character; keep looking for the first.
			Some(Kind::Mention) if !starts_word => {}
			Some(kind) => {
				let query: String = chars[i + 1..caret].iter().collect();
				let fits = kind == Kind::Mention || (starts_word && query.chars().all(is_tag_char));
				return fits.then_some((kind, i, query));
			}
			None if !is_handle_char(chars[i]) || caret - i > MAX_QUERY_CHARS => return None,
			None => {}
		}
	}
	None
}

fn is_handle_char(c: char) -> bool {
	c.is_alphanumeric() || matches!(c, '_' | '.' | '-')
}

fn is_tag_char(c: char) -> bool {
	c.is_alphanumeric() || c == '_'
}

fn matches_query(suggestion: &Suggestion, query: &str) -> bool {
	if query.is_empty() || suggestion.text.to_lowercase().starts_with(query) {
		return true;
	}
	if suggestion.kind == Kind::Hashtag || query.contains('@') {
		return false;
	}
	let name = suggestion.detail.to_lowercase();
	name.starts_with(query) || name.split_whitespace().any(|word| word.starts_with(query))
}

/// How many positions a char takes in the native Windows edit control: UTF-16 units, with a
/// line break counting as two ("\r\n") though `get_value` gives "\n".
fn wx_width(c: char) -> i64 {
	if c == '\n' && cfg!(windows) { 2 } else { i64::from(c.len_utf16() == 2) + 1 }
}

fn wx_pos_to_char(text: &str, pos: i64) -> usize {
	let mut at = 0;
	for (i, c) in text.chars().enumerate() {
		if at >= pos {
			return i;
		}
		at += wx_width(c);
	}
	text.chars().count()
}

fn char_to_wx_pos(text: &str, index: usize) -> i64 {
	text.chars().take(index).map(wx_width).sum()
}

fn describe(popup: &Popup) -> String {
	let entry = &popup.matches[popup.selected];
	format!("{}, {} of {}", entry.spoken(), popup.selected + 1, popup.matches.len())
}

/// The suggestions for one compose dialog's post text.
#[derive(Clone)]
pub struct Completer {
	inner: Rc<RefCell<Inner>>,
	field: TextCtrl,
	list: ListBox,
	panel: Panel,
}

impl Completer {
	/// Wires the suggestions to `field` and `list`. The caller forwards text changes to
	/// [`Self::refresh`] and the dialog's keys to [`Self::handle_key`], and stops the returned
	/// timer once the dialog closes.
	pub fn new(
		dialog: Dialog,
		panel: Panel,
		field: TextCtrl,
		list: ListBox,
		source: SuggestionSource,
	) -> (Self, Timer<Dialog>) {
		let (results_tx, results_rx) = mpsc::channel();
		let mut people = Pool::default();
		for suggestion in source.people {
			people.add(suggestion);
		}
		let mut tags = Pool::default();
		for suggestion in source.tags {
			tags.add(suggestion);
		}
		let inner = Inner {
			people,
			tags,
			self_acct: source.self_acct,
			search: source.search,
			results: HashMap::new(),
			requested: HashSet::new(),
			due: None,
			popup: None,
			dismissed_at: None,
			results_tx,
			results_rx,
		};
		let completer = Self { inner: Rc::new(RefCell::new(inner)), field, list, panel };
		list.show(false);
		let clicked = completer.clone();
		list.on_selection_changed(move |_| {
			if let Some(index) = clicked.list.get_selection() {
				clicked.pick(index as usize);
			}
		});
		let timer = Timer::new(&dialog);
		let ticked = completer.clone();
		timer.on_tick(move |_| ticked.poll());
		timer.start(POLL_INTERVAL_MS, false);
		(completer, timer)
	}

	fn hide_list(&self) {
		if self.list.is_shown() {
			self.list.show(false);
			self.panel.layout();
		}
	}

	/// Re-reads the text and opens, narrows or closes the suggestions. Runs on every text
	/// change, including the ones made here while the state is borrowed - those are never
	/// typing, so a busy state just means "not now".
	pub fn refresh(&self) {
		let text = self.field.get_value();
		let caret = wx_pos_to_char(&text, self.field.get_insertion_point());
		let focused = self.field.has_focus();
		let Ok(mut s) = self.inner.try_borrow_mut() else { return };
		let Some((kind, at, query)) = find_query(&text, caret).filter(|_| focused) else {
			s.dismissed_at = None;
			s.due = None;
			let was_open = s.popup.take().is_some();
			drop(s);
			if was_open {
				self.hide_list();
			}
			return;
		};
		if s.dismissed_at == Some(at) {
			return;
		}
		s.dismissed_at = None;
		let query = query.to_lowercase();
		let key = (kind, query.clone());
		if query.is_empty() || s.search.is_none() || s.requested.contains(&key) {
			s.due = None;
		} else if s.due.as_ref().is_none_or(|(due_kind, due, _)| (*due_kind, due) != (kind, &query)) {
			s.due = Some((kind, query.clone(), Instant::now()));
		}
		let matches = s.matches(kind, &query);
		if matches.is_empty() {
			let was_open = s.popup.take().is_some();
			drop(s);
			if was_open {
				self.hide_list();
			}
			return;
		}
		let previous = s.popup.take().filter(|p| p.kind == kind);
		let opening = previous.is_none();
		let changed = previous.as_ref().is_none_or(|p| p.matches != matches);
		// Keep the highlight on the same suggestion while the list narrows or grows around it.
		let previous_entry = previous.as_ref().and_then(|p| p.matches.get(p.selected));
		let selected = previous_entry.and_then(|e| matches.iter().position(|m| m.text == e.text)).unwrap_or(0);
		let moved = previous_entry.is_none_or(|e| *e != matches[selected]);
		let popup = Popup { kind, at, matches, selected };
		let announcement = describe(&popup);
		let labels: Vec<String> = popup.matches.iter().map(Suggestion::label).collect();
		s.popup = Some(popup);
		drop(s);

		if changed {
			self.list.clear();
			for label in &labels {
				self.list.append(label);
			}
			self.list.set_selection(u32::try_from(selected).unwrap_or(0), true);
			if !self.list.is_shown() {
				self.list.show(true);
				self.panel.layout();
			}
		}
		// Queued behind the echo of the key just typed, and only when the highlighted suggestion
		// changes, so a list that just grows doesn't repeat itself on every keystroke.
		if opening {
			let what = match kind {
				Kind::Mention => "Mention",
				Kind::Hashtag => "Hashtag",
			};
			speech::speak_queued(&format!("{what} suggestions. {announcement}"));
		} else if moved {
			speech::speak_queued(&announcement);
		}
	}

	/// Keys pressed in the post text. Returns true when the key was used up here.
	pub fn handle_key(&self, code: Option<i32>, shift: bool, ctrl: bool, alt: bool) -> bool {
		if alt {
			return false;
		}
		if ctrl && code == Some(keys::SPACE) {
			if let Ok(mut s) = self.inner.try_borrow_mut() {
				s.dismissed_at = None;
			}
			self.refresh();
			let open = self.inner.try_borrow().is_ok_and(|s| s.popup.is_some());
			if !open {
				speech::speak("No suggestions");
			}
			return true;
		}
		let Ok(mut s) = self.inner.try_borrow_mut() else { return false };
		let Some(popup) = s.popup.as_mut() else { return false };
		match code {
			Some(keys::UP | keys::DOWN) if !shift && !ctrl => {
				let count = popup.matches.len();
				popup.selected = if code == Some(keys::DOWN) {
					(popup.selected + 1) % count
				} else {
					(popup.selected + count - 1) % count
				};
				let selected = popup.selected;
				let announcement = describe(popup);
				drop(s);
				self.list.set_selection(u32::try_from(selected).unwrap_or(0), true);
				speech::speak(&announcement);
				true
			}
			Some(keys::RETURN | keys::TAB) if !shift && !ctrl => {
				drop(s);
				self.accept();
				true
			}
			Some(keys::ESCAPE) => {
				s.dismissed_at = Some(popup.at);
				s.popup = None;
				drop(s);
				self.hide_list();
				speech::speak("Suggestions closed");
				true
			}
			// Moving the caret sideways leaves the handle or tag being typed.
			Some(keys::LEFT | keys::RIGHT | keys::HOME | keys::END) => {
				s.popup = None;
				drop(s);
				self.hide_list();
				false
			}
			_ => false,
		}
	}

	/// Replaces "@query" or "#query" with the highlighted suggestion, and a space after it.
	fn accept(&self) {
		let text = self.field.get_value();
		let caret = wx_pos_to_char(&text, self.field.get_insertion_point());
		let Ok(mut s) = self.inner.try_borrow_mut() else { return };
		let Some(popup) = s.popup.take() else { return };
		let Some(entry) = popup.matches.get(popup.selected).cloned() else { return };
		// Set before the edit: the text change it fires must not reopen the list here.
		s.dismissed_at = Some(popup.at);
		s.due = None;
		drop(s);

		self.hide_list();
		let insert = format!("{} ", entry.inserted());
		let from = char_to_wx_pos(&text, popup.at);
		let to = char_to_wx_pos(&text, caret.max(popup.at));
		self.field.replace(from, to, &insert);
		self.field.set_insertion_point(from + insert.chars().map(wx_width).sum::<i64>());
		speech::speak(&match entry.kind {
			Kind::Mention if !entry.detail.is_empty() => format!("{} mentioned", entry.detail),
			Kind::Mention => format!("{} mentioned", entry.inserted()),
			Kind::Hashtag => format!("{} added", entry.inserted()),
		});
	}

	/// A suggestion clicked in the list: picked as if highlighted and accepted.
	fn pick(&self, index: usize) {
		{
			let Ok(mut s) = self.inner.try_borrow_mut() else { return };
			match s.popup.as_mut() {
				Some(popup) if index < popup.matches.len() => popup.selected = index,
				_ => return,
			}
		}
		self.field.set_focus();
		self.accept();
	}

	/// Starts a search once typing has paused, and takes in any answers.
	fn poll(&self) {
		let mut arrived = false;
		{
			let Ok(mut s) = self.inner.try_borrow_mut() else { return };
			while let Ok((kind, query, found)) = s.results_rx.try_recv() {
				let pool = match kind {
					Kind::Mention => &mut s.people,
					Kind::Hashtag => &mut s.tags,
				};
				for suggestion in &found {
					pool.add(suggestion.clone());
				}
				s.results.insert((kind, query), found);
				arrived = true;
			}
			if let Some((kind, query, _)) = s.due.take_if(|(_, _, since)| since.elapsed() >= SEARCH_DELAY)
				&& let Some((client, token)) = s.search.clone()
			{
				s.requested.insert((kind, query.clone()));
				let tx = s.results_tx.clone();
				thread::spawn(move || {
					// A failed search just leaves the timeline's suggestions.
					let found = match kind {
						Kind::Mention => client
							.search_accounts(&token, &query, SEARCH_LIMIT)
							.map(|accounts| accounts.iter().map(Suggestion::from_account).collect()),
						Kind::Hashtag => client
							.search(&token, &query, SearchType::Hashtags, Some(SEARCH_LIMIT), None)
							.map(|results| results.hashtags.iter().map(Suggestion::from_tag).collect()),
					}
					.unwrap_or_default();
					let _ = tx.send((kind, query, found));
				});
			}
		}
		if arrived {
			self.refresh();
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn person(acct: &str, name: &str) -> Suggestion {
		Suggestion { kind: Kind::Mention, text: acct.into(), detail: name.into() }
	}

	fn tag(name: &str) -> Suggestion {
		Suggestion { kind: Kind::Hashtag, text: name.into(), detail: String::new() }
	}

	#[test]
	fn finds_the_handle_being_typed() {
		assert_eq!(find_query("@", 1), Some((Kind::Mention, 0, String::new())));
		assert_eq!(find_query("hi @jan", 7), Some((Kind::Mention, 3, "jan".into())));
		assert_eq!(find_query("hi @jan@mastodon.soc", 20), Some((Kind::Mention, 3, "jan@mastodon.soc".into())));
		assert_eq!(find_query("(@ja", 4), Some((Kind::Mention, 1, "ja".into())));
		assert_eq!(find_query("mail a@b.com", 12), None);
		assert_eq!(find_query("@jan there", 10), None);
		assert_eq!(find_query("@jan there", 4), Some((Kind::Mention, 0, "jan".into())));
		assert_eq!(find_query("@jan\nx", 6), None);
		assert_eq!(find_query(&format!("@{}", "a".repeat(90)), 91), None);
	}

	#[test]
	fn finds_the_hashtag_being_typed() {
		assert_eq!(find_query("#", 1), Some((Kind::Hashtag, 0, String::new())));
		assert_eq!(find_query("I love #rus", 11), Some((Kind::Hashtag, 7, "rus".into())));
		assert_eq!(find_query("(#Rust_lang", 11), Some((Kind::Hashtag, 1, "Rust_lang".into())));
		assert_eq!(find_query("C#", 2), None);
		assert_eq!(find_query("#rust.lang", 10), None);
		assert_eq!(find_query("#rust done", 10), None);
	}

	#[test]
	fn converts_native_positions() {
		let text = "a\nb😀c";
		let nl = if cfg!(windows) { 2 } else { 1 };
		assert_eq!(char_to_wx_pos(text, 3), 2 + nl);
		assert_eq!(char_to_wx_pos(text, 4), 4 + nl);
		assert_eq!(wx_pos_to_char(text, 4 + nl), 4);
		assert_eq!(wx_pos_to_char(text, 100), 5);
	}

	#[test]
	fn matches_handles_and_name_words() {
		let jane = person("jane@mastodon.social", "Jane Doe");
		assert!(matches_query(&jane, ""));
		assert!(matches_query(&jane, "ja"));
		assert!(matches_query(&jane, "jane@mas"));
		assert!(matches_query(&jane, "doe"));
		assert!(!matches_query(&jane, "doe@"));
		assert!(!matches_query(&jane, "x"));
	}

	#[test]
	fn matches_hashtags_by_their_start_only() {
		let rust = Suggestion { detail: "12 people in the past 2 days".into(), ..tag("RustLang") };
		assert!(matches_query(&rust, "rust"));
		assert!(!matches_query(&rust, "lang"));
		assert!(!matches_query(&rust, "12"));
	}

	#[test]
	fn pool_keeps_first_order_and_fills_in_details() {
		let mut pool = Pool::default();
		pool.add(person("a@x", ""));
		pool.add(person("b@x", "Bee"));
		pool.add(person("A@x", "Ay"));
		assert_eq!(pool.list, vec![person("a@x", "Ay"), person("b@x", "Bee")]);
	}

	#[test]
	fn labels_put_the_name_first_for_people_and_last_for_hashtags() {
		assert_eq!(person("jan@x", "Jan").label(), "Jan (@jan@x)");
		assert_eq!(
			Suggestion { detail: "3 people in the past 2 days".into(), ..tag("rust") }.label(),
			"#rust (3 people in the past 2 days)"
		);
		assert_eq!(tag("rust").label(), "#rust");
	}
}
