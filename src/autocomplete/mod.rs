//! Account-bound, incrementally published mention cache. Only the coordinator mutates cache data.
pub mod rate;
mod storage;
#[cfg(test)]
mod tests;
pub mod text;
mod timing;

use std::{
	collections::{HashMap, HashSet},
	ops::Range,
	path::PathBuf,
	sync::{
		Arc, Mutex,
		mpsc::{self, Sender},
	},
	thread,
	time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};

use crate::{
	mastodon::{Account, MastodonClient, Relationship},
	ui_wake::UiWaker,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Entry {
	pub id: String,
	pub address: String,
	pub display_name: String,
	pub revision: u64,
	// Only used while merging API results; unavailable entries are never stored.
	#[serde(skip)]
	pub unavailable: bool,
}

impl Entry {
	fn valid(&self) -> bool {
		let parts: Vec<_> = self.address.split('@').collect();
		!self.id.is_empty()
			&& parts.len() == 3
			&& parts[0].is_empty()
			&& !parts[1].is_empty()
			&& !parts[2].is_empty()
			&& !self.address.chars().any(char::is_whitespace)
	}
	pub fn from_account(account: &Account, server: &url::Url) -> Self {
		let acct = account.acct.trim_start_matches('@');
		let address = if acct.contains('@') {
			format!("@{acct}")
		} else {
			format!("@{acct}@{}", server.host_str().unwrap_or_default())
		};
		Self {
			id: account.id.clone(),
			address,
			display_name: account.display_name.clone(),
			revision: 0,
			unavailable: account.moved.is_some() || account.suspended == Some(true),
		}
	}
	pub fn label(&self) -> String {
		if self.display_name.is_empty() {
			self.address.clone()
		} else {
			format!("{} ({})", self.display_name, self.address)
		}
	}
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct Source {
	next: Option<String>,
	done: bool,
	seen: HashSet<String>,
	#[serde(skip)]
	paused: bool,
	#[serde(skip)]
	attempt: u32,
	#[serde(skip)]
	retry: Option<Instant>,
	#[serde(skip)]
	error: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct AccountCache {
	entries: HashMap<String, Entry>,
	unfollowed: HashSet<String>,
	blocked: HashSet<String>,
	#[serde(default)]
	unavailable: HashSet<String>,
	target_revisions: HashMap<String, u64>,
	sources: [Source; 2],
	following: HashSet<String>,
	scanning_following: HashSet<String>,
	#[serde(default)]
	scan_revision: u64,
	confirm: Vec<String>,
	last_complete: Option<i64>,
	#[serde(default)]
	build_timing: Option<timing::BuildTiming>,
	revision: u64,
}

impl AccountCache {
	fn valid(&self) -> bool {
		self.revision < u64::MAX / 2
			&& self.scan_revision <= self.revision
			&& self.target_revisions.values().all(|r| *r <= self.revision)
			&& self.entries.iter().all(|(id, e)| {
				id == &e.id
					&& e.valid() && e.revision <= self.revision
					&& !self.blocked.contains(id)
					&& !self.unfollowed.contains(id)
					&& !self.unavailable.contains(id)
			})
	}
	fn merge(&mut self, mut entry: Entry, issued: u64, interaction: bool) {
		if entry.id.is_empty() || (!interaction && self.target_revisions.get(&entry.id).is_some_and(|r| *r > issued)) {
			return;
		}
		if entry.unavailable {
			self.entries.remove(&entry.id);
			self.target_revisions.insert(entry.id.clone(), self.revision);
			self.unavailable.insert(entry.id);
			return;
		}
		if !entry.valid() || self.blocked.contains(&entry.id) {
			return;
		}
		// Reply/quote authors can be old snapshots. Only a fresh page clears this exclusion.
		if interaction && self.unavailable.contains(&entry.id) {
			return;
		}
		self.unavailable.remove(&entry.id);
		if interaction {
			self.unfollowed.remove(&entry.id);
		}
		if self.unfollowed.contains(&entry.id) {
			return;
		}
		entry.revision = self.revision;
		self.target_revisions.insert(entry.id.clone(), self.revision);
		self.entries.insert(entry.id.clone(), entry);
	}
	fn relationship(&mut self, id: &str, action: Change) {
		self.target_revisions.insert(id.to_owned(), self.revision);
		match action {
			Change::Unfollow => {
				self.unfollowed.insert(id.to_owned());
				self.entries.remove(id);
				self.following.remove(id);
				self.scanning_following.remove(id);
			}
			Change::Block => {
				self.blocked.insert(id.to_owned());
				self.entries.remove(id);
			}
			Change::Unblock => {
				self.blocked.remove(id);
			}
		}
	}
	fn snapshot(&self, persistence_error: Option<String>) -> Snapshot {
		let mut entries: Vec<_> = self.entries.values().cloned().map(|e| (e.address.to_lowercase(), e)).collect();
		entries.sort_unstable_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.id.cmp(&b.1.id)));
		let mut display_names: Vec<_> = entries
			.iter()
			.enumerate()
			.filter(|(_, (_, entry))| !entry.display_name.trim().is_empty())
			.map(|(index, (_, entry))| (entry.display_name.trim().to_lowercase(), index))
			.collect();
		display_names.sort_unstable();
		let availability = if !entries.is_empty() {
			if self.sources.iter().all(|s| s.done) && self.confirm.is_empty() {
				Availability::Usable
			} else {
				Availability::UsablePartial
			}
		} else if self.last_complete.is_some() {
			Availability::CompletedEmpty
		} else if let Some(error) = self.sources.iter().find_map(|s| s.error.clone()) {
			Availability::Unavailable(error)
		} else {
			Availability::Building
		};
		Snapshot { entries, display_names, revision: self.revision, availability, persistence_error }
	}
}

#[derive(Clone, Debug)]
pub enum Availability {
	Loading,
	Building,
	Usable,
	UsablePartial,
	CompletedEmpty,
	Unavailable(String),
}

#[derive(Debug)]
pub struct Snapshot {
	pub entries: Vec<(String, Entry)>,
	display_names: Vec<(String, usize)>,
	pub revision: u64,
	pub availability: Availability,
	pub persistence_error: Option<String>,
}

pub enum Matches {
	Range(Range<usize>),
	Indices(Vec<usize>),
}

impl Matches {
	pub fn len(&self) -> usize {
		match self {
			Self::Range(range) => range.len(),
			Self::Indices(indices) => indices.len(),
		}
	}
	pub fn get(&self, row: usize) -> Option<usize> {
		match self {
			Self::Range(range) => (row < range.len()).then(|| range.start + row),
			Self::Indices(indices) => indices.get(row).copied(),
		}
	}
}

impl Snapshot {
	pub fn matching(&self, query: &str) -> Matches {
		let query = query.trim().to_lowercase();
		let name_prefix = query.strip_prefix('@').unwrap_or(&query);
		if name_prefix.is_empty() {
			return Matches::Range(0..self.entries.len());
		}
		let prefix = format!("@{name_prefix}");
		let start = self.entries.partition_point(|(key, _)| key < &prefix);
		let end = start + self.entries[start..].partition_point(|(key, _)| key.starts_with(&prefix));
		let name_start = self.display_names.partition_point(|(key, _)| key.as_str() < name_prefix);
		let name_end =
			name_start + self.display_names[name_start..].partition_point(|(key, _)| key.starts_with(name_prefix));
		if name_start == name_end {
			return Matches::Range(start..end);
		}
		// Store row indices only; both indexes refer to the same immutable entries.
		let mut indices: Vec<_> =
			(start..end).chain(self.display_names[name_start..name_end].iter().map(|(_, index)| *index)).collect();
		indices.sort_unstable();
		indices.dedup();
		Matches::Indices(indices)
	}
}

#[derive(Clone, Copy)]
pub enum Change {
	Unfollow,
	Block,
	Unblock,
}

#[derive(Default)]
struct Published {
	snapshots: HashMap<String, Arc<Snapshot>>,
	active: Option<String>,
	removed: HashSet<String>,
	shutdown: Option<Result<(), String>>,
}

#[derive(Clone)]
pub struct Service {
	tx: Sender<Command>,
	published: Arc<Mutex<Published>>,
}

impl std::fmt::Debug for Service {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.write_str("AutocompleteService")
	}
}

#[derive(Clone, Debug)]
pub struct Session {
	pub service: Service,
	pub account: String,
}

impl Session {
	pub fn snapshot(&self) -> Arc<Snapshot> {
		self.service.published.lock().unwrap().snapshots.get(&self.account).cloned().unwrap_or_else(|| {
			Arc::new(Snapshot {
				entries: Vec::new(),
				display_names: Vec::new(),
				revision: 0,
				availability: Availability::Loading,
				persistence_error: None,
			})
		})
	}
	pub fn eligible(&self) -> bool {
		let published = self.service.published.lock().unwrap();
		published.active.as_ref() == Some(&self.account) && !published.removed.contains(&self.account)
	}
	pub fn interaction(&self, entry: Entry) {
		let _ = self.service.tx.send(Command::Interaction(self.account.clone(), entry, false));
	}
	pub fn follow(&self, entry: Entry) {
		let _ = self.service.tx.send(Command::Interaction(self.account.clone(), entry, true));
	}
	pub fn relationship(&self, id: String, change: Change) {
		let _ = self.service.tx.send(Command::Relationship(self.account.clone(), id, change));
	}
	pub fn observed_relationships(&self, relationships: Vec<Relationship>, revision: u64) {
		let _ = self.service.tx.send(Command::ObservedRelationships(self.account.clone(), relationships, revision));
	}
}

#[derive(Clone, PartialEq, Eq)]
struct Credentials {
	base: url::Url,
	token: String,
	user_id: String,
}
enum Command {
	Select(String),
	Activate(String, Credentials),
	Pause,
	Remove(String),
	Interaction(String, Entry, bool),
	Relationship(String, String, Change),
	ObservedRelationships(String, Vec<Relationship>, u64),
	Fetched(Job, Result<Fetched, String>, Option<u16>),
	RequestStarted(String, u64),
	Saved(u64, Result<bool, String>),
	LogFlushed(Result<(), String>),
	Shutdown,
}

enum DiskCommand {
	Save(storage::FileData, u64, u64),
	Log(String),
	FlushLog,
}

#[derive(Clone)]
struct Job {
	owner: String,
	generation: u64,
	revision: u64,
	source: usize,
	next: Option<String>,
	confirm: Vec<String>,
	credentials: Credentials,
}
enum Fetched {
	Page(Vec<Entry>, Option<String>),
	Relationships(Vec<Relationship>),
}

impl Service {
	#[cfg(test)]
	pub(crate) fn select_for_ui_test(&self, id: &str) {
		self.published.lock().unwrap().active = Some(id.into());
	}
	pub fn start(path: PathBuf, configured: HashSet<String>, waker: UiWaker) -> Self {
		let (tx, rx) = mpsc::channel();
		let published = Arc::new(Mutex::new(Published::default()));
		let service = Self { tx, published };
		let worker = service.clone();
		thread::spawn(move || Coordinator::new(path, configured, worker, waker).run(&rx));
		service
	}
	pub fn session(&self, account: String) -> Session {
		Session { service: self.clone(), account }
	}
	pub fn select(&self, account: String) {
		self.published.lock().unwrap().active = Some(account.clone());
		let _ = self.tx.send(Command::Select(account));
	}
	pub fn activate(&self, account: String, base: url::Url, token: String, user_id: String) {
		self.published.lock().unwrap().active = Some(account.clone());
		let _ = self.tx.send(Command::Activate(account, Credentials { base, token, user_id }));
	}
	pub fn pause(&self) {
		self.published.lock().unwrap().active = None;
		let _ = self.tx.send(Command::Pause);
	}
	pub fn remove(&self, account: String) {
		let mut published = self.published.lock().unwrap();
		published.removed.insert(account.clone());
		published.snapshots.remove(&account);
		drop(published);
		let _ = self.tx.send(Command::Remove(account));
	}
	pub fn shutdown(&self) {
		self.published.lock().unwrap().active = None;
		let _ = self.tx.send(Command::Shutdown);
	}
	pub fn shutdown_result(&self) -> Option<Result<(), String>> {
		self.published.lock().unwrap().shutdown.clone()
	}
}

// Fetch, save, persistence availability and shutdown are independent state axes.
#[allow(clippy::struct_excessive_bools)]
struct Coordinator {
	accounts: HashMap<String, AccountCache>,
	credentials: HashMap<String, Credentials>,
	generations: HashMap<String, u64>,
	active: Option<String>,
	service: Service,
	waker: UiWaker,
	gate: Arc<Mutex<u64>>,
	fetch: Sender<Job>,
	disk: Sender<DiskCommand>,
	log_flushed: bool,
	log_error: Option<String>,
	in_flight: bool,
	saving: bool,
	revision: u64,
	saved: u64,
	next_fetch: Instant,
	next_save: Instant,
	save_attempt: u32,
	persistence_error: Option<String>,
	writable: bool,
	closing: bool,
	final_attempt: bool,
}

impl Coordinator {
	fn new(path: PathBuf, configured: HashSet<String>, service: Service, waker: UiWaker) -> Self {
		// Loading happens on this thread before any queued startup commands are applied.
		let loaded = storage::load(&path, &configured);
		let gate = Arc::new(Mutex::new(0));
		let (disk, writes) = mpsc::channel::<DiskCommand>();
		let disk_tx = service.tx.clone();
		let disk_gate = gate.clone();
		thread::spawn(move || {
			let mut log_error = None;
			while let Ok(command) = writes.recv() {
				match command {
					DiskCommand::Save(data, revision, epoch) => {
						let result = storage::save(&path, &data, epoch, &disk_gate).map_err(|e| e.to_string());
						let _ = disk_tx.send(Command::Saved(revision, result));
					}
					DiskCommand::Log(message) => {
						if let Err(error) = storage::append_log(&path.with_file_name("autocomplete.log"), &message) {
							log_error = Some(format!("Could not write autocomplete.log: {error}"));
						}
					}
					DiskCommand::FlushLog => {
						let _ = disk_tx.send(Command::LogFlushed(log_error.take().map_or(Ok(()), Err)));
					}
				}
			}
		});
		let (fetch, jobs) = mpsc::channel::<Job>();
		let fetch_tx = service.tx.clone();
		thread::spawn(move || {
			while let Ok(job) = jobs.recv() {
				let result = fetch_job(&job, || {
					let _ = fetch_tx.send(Command::RequestStarted(job.owner.clone(), job.generation));
				});
				let status = result.as_ref().err().and_then(|error| {
					// Locally rejected checkpoints use the same restart path as server-rejected cursors.
					if error.is::<crate::mastodon::InvalidAccountPagination>() {
						Some(400)
					} else {
						error.downcast_ref::<reqwest::Error>().and_then(reqwest::Error::status).map(|s| s.as_u16())
					}
				});
				let _ = fetch_tx.send(Command::Fetched(job, result.map_err(|e| e.to_string()), status));
			}
		});
		let mut accounts = loaded.accounts;
		for id in configured {
			accounts.entry(id).or_default();
		}
		let revision = accounts.values().map(|a| a.revision).max().unwrap_or(0) + 1;
		Self {
			generations: accounts.keys().map(|id| (id.clone(), 1)).collect(),
			accounts,
			credentials: HashMap::new(),
			active: None,
			service,
			waker,
			gate,
			fetch,
			disk,
			log_flushed: false,
			log_error: None,
			in_flight: false,
			saving: false,
			revision,
			saved: 0,
			next_fetch: Instant::now(),
			next_save: Instant::now() + Duration::from_secs(2),
			save_attempt: 0,
			persistence_error: loaded.error,
			writable: loaded.writable,
			closing: false,
			final_attempt: false,
		}
	}
	fn publish(&self, id: &str) {
		if let Some(account) = self.accounts.get(id) {
			let snapshot = Arc::new(account.snapshot(self.persistence_error.clone()));
			self.service.published.lock().unwrap().snapshots.insert(id.to_owned(), snapshot);
		}
		self.waker.wake();
	}
	fn changed(&mut self, id: &str) {
		self.revision += 1;
		if let Some(account) = self.accounts.get_mut(id) {
			account.revision = self.revision;
		}
		self.publish(id);
	}
	fn run(&mut self, rx: &mpsc::Receiver<Command>) {
		for id in self.accounts.keys() {
			self.publish(id);
		}
		loop {
			// All accepted messages ahead of Shutdown are processed before the save barrier.
			let now = Instant::now();
			if !self.closing {
				self.schedule(
					now,
					chrono::Utc::now().timestamp(),
					rate::FOREGROUND.load(std::sync::atomic::Ordering::SeqCst) > 0,
				);
			}
			if !self.saving
				&& self.writable
				&& self.revision != self.saved
				&& !(self.closing && self.final_attempt)
				&& (self.closing || now >= self.next_save)
			{
				self.saving = true;
				self.final_attempt = self.closing;
				let data = storage::FileData { version: 1, accounts: self.accounts.clone() };
				let _ = self.disk.send(DiskCommand::Save(data, self.revision, *self.gate.lock().unwrap()));
			}
			if self.closing
				&& self.log_flushed
				&& !self.saving
				&& (!self.writable || self.revision == self.saved || self.final_attempt)
			{
				let result = self.persistence_error.clone().or_else(|| self.log_error.clone()).map_or(Ok(()), Err);
				self.service.published.lock().unwrap().shutdown = Some(result);
				self.waker.wake();
				break;
			}
			let mut deadline = now + Duration::from_secs(86400);
			if !self.closing && !self.in_flight && self.active.is_some() {
				deadline = deadline.min(self.next_fetch.max(now + Duration::from_millis(100)));
			}
			if !self.saving && self.writable && self.saved != self.revision {
				deadline = deadline.min(self.next_save);
			}
			match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
				Ok(command) => self.command(command),
				Err(mpsc::RecvTimeoutError::Timeout) => (),
				Err(mpsc::RecvTimeoutError::Disconnected) => break,
			}
		}
	}
	fn command(&mut self, command: Command) {
		match command {
			Command::Select(id) if !self.closing => {
				if self.service.published.lock().unwrap().removed.contains(&id) {
					return;
				}
				self.active = None;
				let account = self.accounts.entry(id.clone()).or_default();
				account.sources[0].error =
					Some("User autocomplete is unavailable until account credentials can be verified.".into());
				self.publish(&id);
			}
			Command::Activate(id, credentials) if !self.closing => {
				if self.service.published.lock().unwrap().removed.contains(&id) {
					return;
				}
				let generation = self.generations.entry(id.clone()).or_insert(1);
				if self.credentials.get(&id).is_some_and(|previous| previous != &credentials) {
					*generation += 1;
				}
				let account = self.accounts.entry(id.clone()).or_default();
				for source in &mut account.sources {
					source.paused = false;
					source.retry = None;
					source.error = None;
				}
				self.credentials.insert(id.clone(), credentials);
				self.active = Some(id.clone());
				self.next_fetch = self.next_fetch.min(Instant::now() + Duration::from_secs(2));
				self.publish(&id);
			}
			Command::Pause => {
				self.active = None;
			}
			Command::Remove(id) => {
				*self.gate.lock().unwrap() += 1;
				*self.generations.entry(id.clone()).or_default() += 1;
				self.accounts.remove(&id);
				self.credentials.remove(&id);
				if self.active.as_ref() == Some(&id) {
					self.active = None;
				}
				self.service.published.lock().unwrap().snapshots.remove(&id);
				self.changed(&id);
				self.next_save = Instant::now();
			}
			Command::Interaction(id, entry, follow) if !self.closing => {
				if let Some(account) = self.accounts.get_mut(&id) {
					account.revision = self.revision + 1;
					if follow {
						account.following.insert(entry.id.clone());
						account.scanning_following.insert(entry.id.clone());
					}
					account.merge(entry, u64::MAX, true);
					self.changed(&id);
				}
			}
			Command::Relationship(id, target, change) if !self.closing => {
				if let Some(account) = self.accounts.get_mut(&id) {
					account.revision = self.revision + 1;
					account.relationship(&target, change);
					self.changed(&id);
				}
			}
			Command::ObservedRelationships(id, relationships, issued) if !self.closing => {
				if let Some(account) = self.accounts.get_mut(&id) {
					account.revision = self.revision + 1;
					for relationship in relationships {
						// Unchanged observations must not invalidate entries in pending pages.
						if account.target_revisions.get(&relationship.id).is_none_or(|r| *r <= issued)
							&& account.blocked.contains(&relationship.id) != relationship.blocking
						{
							account.relationship(
								&relationship.id,
								if relationship.blocking { Change::Block } else { Change::Unblock },
							);
						}
					}
					self.changed(&id);
				}
			}
			Command::Fetched(job, result, status) => {
				self.in_flight = false;
				if self.closing || self.generations.get(&job.owner) != Some(&job.generation) {
					return;
				}
				self.fetched(&job, result, status);
			}
			Command::RequestStarted(id, generation) if !self.closing => {
				if self.generations.get(&id) == Some(&generation)
					&& let Some(account) = self.accounts.get_mut(&id)
					&& let Some(timing) = &mut account.build_timing
					&& timing.completed_at.is_none()
				{
					timing.record_request();
					// Counting affects persistence only; searchable entries have not changed.
					self.revision += 1;
					account.revision = self.revision;
				}
			}
			Command::Saved(revision, result) => {
				self.saving = false;
				match result {
					Ok(true) => {
						self.saved = revision;
						self.persistence_error = None;
						self.save_attempt = 0;
						self.next_save = Instant::now() + Duration::from_secs(2);
					}
					Ok(false) => {
						self.next_save = Instant::now();
						self.final_attempt = false;
					}
					Err(error) => {
						self.persistence_error = Some(error);
						self.next_save = Instant::now() + rate::backoff(self.save_attempt);
						self.save_attempt += 1;
					}
				}
				for id in self.accounts.keys() {
					self.publish(id);
				}
			}
			Command::Shutdown if !self.closing => {
				self.closing = true;
				self.active = None;
				let _ = self.disk.send(DiskCommand::FlushLog);
			}
			Command::LogFlushed(result) => {
				self.log_flushed = true;
				self.log_error = result.err();
			}
			_ => (),
		}
	}
	fn schedule(&mut self, now: Instant, unix_now: i64, foreground_busy: bool) {
		// Selection and shutdown are published synchronously, before their queued commands.
		if self.service.published.lock().unwrap().active != self.active {
			return;
		}
		if self.in_flight || now < self.next_fetch {
			return;
		}
		let Some(id) = self.active.clone() else {
			return;
		};
		let Some(credentials) = self.credentials.get(&id).cloned() else {
			return;
		};
		if foreground_busy {
			self.next_fetch = now + Duration::from_millis(200);
			return;
		}
		if let Some(deadline) = rate::defer_until(&credentials.base.origin().ascii_serialization(), now) {
			self.next_fetch = deadline;
			if let Some(account) = self.accounts.get_mut(&id) {
				account.sources[0].error =
					Some("User autocomplete is unavailable while waiting for the server rate limit.".into());
			}
			self.publish(&id);
			return;
		}
		let account = self.accounts.get_mut(&id).unwrap();
		if account.sources.iter().all(|s| s.done) && account.confirm.is_empty() {
			let due = account.last_complete.unwrap_or(0) + 86400;
			let remaining = due - unix_now;
			if remaining > 0 {
				self.next_fetch = now + Duration::from_secs(remaining.unsigned_abs());
				return;
			}
			account.sources = Default::default();
			account.scanning_following.clear();
			account.scan_revision = account.revision;
		}
		let source = if !account.confirm.is_empty()
			&& !account.sources[0].paused
			&& account.sources[0].retry.is_none_or(|d| d <= now)
		{
			0
		} else {
			let Some(source) =
				account.sources.iter().position(|s| !s.done && !s.paused && s.retry.is_none_or(|d| d <= now))
			else {
				self.next_fetch = account
					.sources
					.iter()
					.filter_map(|s| s.retry)
					.filter(|d| *d > now)
					.min()
					.unwrap_or(now + Duration::from_secs(86400));
				return;
			};
			source
		};
		if account.sources[source].paused {
			self.next_fetch = now + Duration::from_secs(86400);
			return;
		}
		if let Some(deadline) = account.sources[source].retry.filter(|d| *d > now) {
			self.next_fetch = deadline;
			return;
		}
		let job = Job {
			owner: id.clone(),
			generation: self.generations[&id],
			revision: account.revision,
			source,
			next: account.sources[source].next.clone(),
			confirm: if source == 0 { account.confirm.iter().take(80).cloned().collect() } else { Vec::new() },
			credentials,
		};
		if account.build_timing.as_ref().is_none_or(|timing| timing.completed_at.is_some()) {
			let started_on_resume = account.sources.iter().any(|s| s.done || s.next.is_some() || !s.seen.is_empty())
				|| !account.confirm.is_empty();
			let timing = timing::BuildTiming::new(unix_now, started_on_resume);
			let _ = self.disk.send(DiskCommand::Log(timing.start_message(&id)));
			account.build_timing = Some(timing);
			self.changed(&id);
		}
		self.in_flight = true;
		self.next_fetch = now + Duration::from_secs(2);
		let _ = self.fetch.send(job);
	}
	fn fetched(&mut self, job: &Job, result: Result<Fetched, String>, status: Option<u16>) {
		let Some(account) = self.accounts.get_mut(&job.owner) else {
			return;
		};
		let was_complete = account.sources.iter().all(|s| s.done) && account.confirm.is_empty();
		account.revision = self.revision + 1;
		match result {
			Ok(Fetched::Page(entries, next)) => {
				for entry in entries {
					if job.source == 0 && account.target_revisions.get(&entry.id).is_none_or(|r| *r <= job.revision) {
						account.scanning_following.insert(entry.id.clone());
					}
					account.merge(entry, job.revision, false);
				}
				let source = &mut account.sources[job.source];
				if let Some(cursor) = &job.next {
					source.seen.insert(cursor.clone());
				}
				if next.as_ref().is_some_and(|cursor| source.seen.contains(cursor)) {
					source.paused = true;
					source.error =
						Some("User autocomplete is unavailable: the server repeated a pagination link.".into());
				} else {
					source.done = next.is_none();
					source.next = next;
					source.retry = None;
					source.attempt = 0;
					source.error = None;
					if source.done && job.source == 0 {
						account.confirm = account.following.difference(&account.scanning_following).cloned().collect();
						account.following.clone_from(&account.scanning_following);
					}
				}
			}
			Ok(Fetched::Relationships(relationships)) => {
				for relationship in relationships {
					if account
						.target_revisions
						.get(&relationship.id)
						.is_none_or(|r| *r <= job.revision.min(account.scan_revision))
					{
						if relationship.blocking {
							account.relationship(&relationship.id, Change::Block);
						} else {
							account.relationship(&relationship.id, Change::Unblock);
						}
						if relationship.following {
							account.following.insert(relationship.id);
						} else {
							account.relationship(&relationship.id, Change::Unfollow);
						}
					}
				}
				account.confirm.retain(|id| !job.confirm.contains(id));
				account.sources[0].retry = None;
				account.sources[0].error = None;
			}
			Err(error) => {
				let source = &mut account.sources[job.source];
				if matches!(status, Some(401 | 403)) {
					source.paused = true;
				} else {
					if matches!(status, Some(400 | 404 | 410 | 422)) && job.next.is_some() {
						source.next = None;
						source.seen.clear();
						if job.source == 0 {
							account.scanning_following.clear();
						}
					}
					let reported = if status == Some(429) {
						rate::reported_retry(&job.credentials.base.origin().ascii_serialization())
					} else {
						None
					};
					source.retry = Some(reported.unwrap_or_else(|| Instant::now() + rate::backoff(source.attempt)));
					source.attempt += 1;
				}
				source.error = Some(if source.paused {
					format!("User autocomplete is unavailable: access was denied ({error}).")
				} else {
					format!("User autocomplete is unavailable; retrying after a server or network failure ({error}).")
				});
			}
		}
		if !was_complete && account.sources.iter().all(|s| s.done) && account.confirm.is_empty() {
			let completed_at = chrono::Utc::now().timestamp();
			account.last_complete = Some(completed_at);
			if let Some(timing) = &mut account.build_timing
				&& let Some(message) = timing.finish(&job.owner, completed_at)
			{
				let _ = self.disk.send(DiskCommand::Log(message));
			}
			self.next_save = Instant::now();
		}
		self.changed(&job.owner);
	}
}

fn fetch_job(job: &Job, request_started: impl FnOnce()) -> anyhow::Result<Fetched> {
	let client = MastodonClient::for_autocomplete(job.credentials.base.clone())?;
	if !job.confirm.is_empty() {
		let relationships = client.get_relationships_observed(&job.credentials.token, &job.confirm, request_started)?;
		anyhow::ensure!(
			job.confirm.iter().all(|id| relationships.iter().any(|r| &r.id == id)),
			"Incomplete relationship confirmation response"
		);
		return Ok(Fetched::Relationships(relationships));
	}
	let page = client.autocomplete_page_observed(
		&job.credentials.token,
		&job.credentials.user_id,
		job.source == 0,
		job.next.as_deref(),
		request_started,
	)?;
	Ok(Fetched::Page(page.accounts.iter().map(|a| Entry::from_account(a, &job.credentials.base)).collect(), page.next))
}
