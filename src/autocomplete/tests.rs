use std::{
	fs,
	sync::atomic::{AtomicU64, Ordering},
};

use super::*;

static TEMP_ID: AtomicU64 = AtomicU64::new(0);
static MOCK_NETWORK: Mutex<()> = Mutex::new(());
struct TestDirectory(PathBuf);
impl TestDirectory {
	fn new() -> Self {
		let path = std::env::temp_dir().join(format!(
			"fedra-autocomplete-{}-{}",
			std::process::id(),
			TEMP_ID.fetch_add(1, Ordering::Relaxed)
		));
		fs::create_dir_all(&path).unwrap();
		Self(path)
	}
	fn cache(&self) -> PathBuf {
		self.0.join("autocomplete-cache.json")
	}
}
impl Drop for TestDirectory {
	fn drop(&mut self) {
		let _ = fs::remove_dir_all(&self.0);
	}
}

fn entry(id: &str, address: &str) -> Entry {
	Entry { id: id.into(), address: address.into(), display_name: "Alice".into(), revision: 0, unavailable: false }
}

fn model() -> (Coordinator, mpsc::Receiver<Job>) {
	let (tx, _) = mpsc::channel();
	let service = Service { tx, published: Arc::new(Mutex::new(Published::default())) };
	let (fetch, jobs) = mpsc::channel();
	let (disk, _) = mpsc::channel();
	let credentials = Credentials {
		base: url::Url::parse("https://test.invalid/").unwrap(),
		token: "secret".into(),
		user_id: "me".into(),
	};
	let coordinator = Coordinator {
		accounts: HashMap::from([("a".into(), AccountCache::default()), ("b".into(), AccountCache::default())]),
		credentials: HashMap::from([("a".into(), credentials.clone()), ("b".into(), credentials)]),
		generations: HashMap::from([("a".into(), 1), ("b".into(), 1)]),
		active: Some("a".into()),
		service,
		waker: UiWaker::silent(),
		gate: Arc::new(Mutex::new(0)),
		fetch,
		disk,
		log_flushed: false,
		log_error: None,
		in_flight: false,
		saving: false,
		revision: 0,
		saved: 0,
		next_fetch: Instant::now(),
		next_save: Instant::now(),
		save_attempt: 0,
		persistence_error: None,
		writable: true,
		closing: false,
		final_attempt: false,
	};
	coordinator.service.published.lock().unwrap().active = Some("a".into());
	for id in ["a", "b"] {
		coordinator.publish(id);
	}
	(coordinator, jobs)
}

fn job(coordinator: &Coordinator, owner: &str, source: usize) -> Job {
	Job {
		owner: owner.into(),
		generation: coordinator.generations[owner],
		revision: coordinator.accounts[owner].revision,
		source,
		next: coordinator.accounts[owner].sources[source].next.clone(),
		confirm: Vec::new(),
		credentials: coordinator.credentials[owner].clone(),
	}
}

fn page(coordinator: &mut Coordinator, job: Job, entries: Vec<Entry>, next: Option<&str>) {
	coordinator.command(Command::Fetched(job, Ok(Fetched::Page(entries, next.map(str::to_owned))), None));
}

#[test]
fn prefixes_and_one_hundred_thousand_entries() {
	let mut account = AccountCache::default();
	for i in 0..100_000 {
		let mut e = entry(&i.to_string(), &format!("@user{i:05}@example.org"));
		e.display_name = format!("Person {i:05}");
		account.entries.insert(e.id.clone(), e);
	}
	let snapshot = account.snapshot(None);
	let started = Instant::now();
	for _ in 0..1000 {
		assert_eq!(snapshot.matching("  @USER123  ").len(), 100);
		assert_eq!(snapshot.matching("user12345@exa").len(), 1);
		assert_eq!(snapshot.matching(" @PERSON 123 ").len(), 100);
		assert_eq!(snapshot.matching("example").len(), 0);
		assert_eq!(snapshot.matching("@@user").len(), 0);
		assert_eq!(snapshot.matching(" @ ").len(), 100_000);
		assert_eq!(snapshot.matching("  ").len(), 100_000);
	}
	assert_eq!(snapshot.matching("person").len(), 100_000);
	assert!(started.elapsed() < Duration::from_secs(2), "Prefix lookup unexpectedly slow: {:?}", started.elapsed());
}

#[test]
fn display_name_prefixes_merge_with_addresses_and_follow_metadata_changes() {
	let mut account = AccountCache::default();
	for (id, address, name) in [
		("zach", "@ZBennoui@dragonscave.space", "Zach Bennoui"),
		("both", "@zara@example.org", "Zara"),
		("address", "@zane@example.org", ""),
		("name", "@aaron@example.org", "  Zander  "),
		("other", "@bob@example.org", "Bob"),
		("unicode", "@emile@example.org", "Émile"),
	] {
		let mut e = entry(id, address);
		e.display_name = name.into();
		account.merge(e, 0, true);
	}
	let snapshot = account.snapshot(None);
	let ids = |snapshot: &Snapshot, query: &str| {
		let matches = snapshot.matching(query);
		(0..matches.len()).map(|row| snapshot.entries[matches.get(row).unwrap()].1.id.clone()).collect::<Vec<_>>()
	};
	for query in ["za", "@za", "  @ZA  "] {
		assert_eq!(ids(&snapshot, query), ["name", "address", "both", "zach"]);
	}
	assert_eq!(ids(&snapshot, "zach ben"), ["zach"]);
	assert_eq!(ids(&snapshot, "@zbennoui@dragons"), ["zach"]);
	assert_eq!(ids(&snapshot, "@ÉM"), ["unicode"]);
	for query in ["bennoui", "dragonscave", "@@za", "missing"] {
		assert!(ids(&snapshot, query).is_empty());
	}
	assert!(snapshot.matching("za").get(4).is_none());
	for query in ["", " ", "@"] {
		assert_eq!(snapshot.matching(query).len(), 6);
	}
	let mut updated = account.entries["zach"].clone();
	updated.display_name = "Benjamin".into();
	account.merge(updated, 0, true);
	assert!(ids(&account.snapshot(None), "zach").is_empty());
	assert_eq!(ids(&account.snapshot(None), "ben"), ["zach"]);
	account.relationship("zach", Change::Unfollow);
	assert!(ids(&account.snapshot(None), "ben").is_empty());
	assert_eq!(ids(&snapshot, "zach"), ["zach"], "Published snapshots remain immutable");
}

#[test]
fn partial_empty_and_failed_availability() {
	let (mut c, _) = model();
	assert!(matches!(c.accounts["a"].snapshot(None).availability, Availability::Building));
	let request = job(&c, "a", 0);
	page(&mut c, request, vec![], None);
	assert!(matches!(c.accounts["a"].snapshot(None).availability, Availability::Building));
	let request = job(&c, "a", 1);
	c.command(Command::Fetched(request.clone(), Err("offline".into()), None));
	assert!(matches!(c.accounts["a"].snapshot(None).availability, Availability::Unavailable(_)));
	page(&mut c, request, vec![], None);
	assert!(matches!(c.accounts["a"].snapshot(None).availability, Availability::CompletedEmpty));
	let request = job(&c, "a", 0);
	c.command(Command::Fetched(request, Err("offline".into()), None));
	assert!(matches!(c.accounts["a"].snapshot(None).availability, Availability::CompletedEmpty));
	c.command(Command::Interaction("b".into(), entry("1", "@alice@example.org"), false));
	assert!(matches!(c.accounts["b"].snapshot(Some("write failed".into())).availability, Availability::UsablePartial));
}

#[test]
fn checkpoints_replay_and_newer_interactions() {
	let (mut c, _) = model();
	let old = job(&c, "a", 0);
	c.command(Command::Interaction("a".into(), entry("1", "@new@example.org"), false));
	page(
		&mut c,
		old.clone(),
		vec![entry("1", "@old@example.org")],
		Some("https://test.invalid/api/v1/accounts/me/following?cursor=opaque"),
	);
	assert_eq!(c.accounts["a"].entries["1"].address, "@new@example.org");
	assert!(c.accounts["a"].sources[0].next.is_some());
	page(
		&mut c,
		old,
		vec![entry("1", "@old@example.org")],
		Some("https://test.invalid/api/v1/accounts/me/following?cursor=opaque"),
	);
	assert_eq!(c.accounts["a"].entries.len(), 1);
	let next = job(&c, "a", 0);
	page(&mut c, next.clone(), vec![], next.next.as_deref());
	assert!(c.accounts["a"].sources[0].paused);
	assert!(!c.accounts["a"].sources[0].done);
}

#[test]
fn exclusions_dominate_pages_and_unblock_preserves_unfollow() {
	let (mut c, _) = model();
	let old = job(&c, "a", 0);
	let alice = entry("1", "@alice@example.org");
	c.command(Command::Relationship("a".into(), "1".into(), Change::Unfollow));
	page(&mut c, old.clone(), vec![alice.clone()], None);
	assert!(c.accounts["a"].entries.is_empty());
	c.command(Command::Interaction("a".into(), alice.clone(), false));
	page(&mut c, old, vec![entry("1", "@stale@example.org")], None);
	assert_eq!(c.accounts["a"].entries["1"].address, alice.address);
	c.command(Command::Relationship("a".into(), "1".into(), Change::Block));
	c.command(Command::Relationship("a".into(), "1".into(), Change::Unfollow));
	c.command(Command::Interaction("a".into(), alice.clone(), false));
	c.command(Command::Relationship("a".into(), "1".into(), Change::Unblock));
	assert!(c.accounts["a"].entries.is_empty());
	assert!(c.accounts["a"].unfollowed.contains("1"));
	c.command(Command::Interaction("a".into(), alice, false));
	assert_eq!(c.accounts["a"].entries.len(), 1);
}

#[test]
fn switching_removal_late_results_and_inactive_interactions() {
	let (mut c, _) = model();
	let old = job(&c, "a", 0);
	c.command(Command::Activate("b".into(), c.credentials["b"].clone()));
	page(&mut c, old.clone(), vec![entry("1", "@alice@example.org")], None);
	assert!(c.accounts["b"].entries.is_empty());
	c.command(Command::Interaction("a".into(), entry("2", "@bob@example.org"), false));
	assert_eq!(c.accounts["a"].entries.len(), 2);
	c.command(Command::Remove("a".into()));
	page(&mut c, old, vec![entry("1", "@alice@example.org")], None);
	c.command(Command::Interaction("a".into(), entry("2", "@bob@example.org"), false));
	assert!(!c.accounts.contains_key("a"));
	assert!(c.service.session("a".into()).snapshot().entries.is_empty());
}

#[test]
fn scheduler_uses_injected_time_and_independent_sources() {
	let (mut c, jobs) = model();
	let now = Instant::now();
	c.next_fetch = now;
	c.schedule(now, 100, false);
	let first = jobs.try_recv().unwrap();
	assert_eq!(first.source, 0);
	c.schedule(now + Duration::from_secs(20), 120, false);
	assert!(jobs.try_recv().is_err(), "Only one request can be in flight");
	page(&mut c, first, vec![], None);
	c.schedule(now + Duration::from_secs(1), 101, false);
	assert!(jobs.try_recv().is_err(), "Requests must be spaced");
	c.schedule(now + Duration::from_secs(2), 102, false);
	let second = jobs.try_recv().unwrap();
	assert_eq!(second.source, 1);
	c.command(Command::Fetched(second, Err("denied".into()), Some(403)));
	assert!(c.accounts["a"].sources[0].done);
	assert!(!c.accounts["a"].sources[1].done);
	c.schedule(now + Duration::from_secs(300), 400, false);
	assert!(jobs.try_recv().is_err());
	c.command(Command::Activate("a".into(), c.credentials["a"].clone()));
	c.schedule(now + Duration::from_secs(301), 401, false);
	assert_eq!(jobs.try_recv().unwrap().source, 1);
}

#[test]
fn interrupted_save_and_removal_cannot_replace_valid_file() {
	let dir = TestDirectory::new();
	let path = dir.cache();
	let data = storage::FileData { version: 1, accounts: HashMap::from([("a".into(), AccountCache::default())]) };
	let gate = Arc::new(Mutex::new(0));
	assert!(storage::save(&path, &data, 0, &gate).unwrap());
	let valid = fs::read(&path).unwrap();
	*gate.lock().unwrap() = 1;
	assert!(!storage::save(&path, &data, 0, &gate).unwrap());
	assert_eq!(fs::read(&path).unwrap(), valid);
	fs::create_dir(path.with_extension("json.tmp")).unwrap();
	assert!(storage::save(&path, &data, 1, &gate).is_err());
	assert_eq!(fs::read(&path).unwrap(), valid);
	fs::remove_dir(path.with_extension("json.tmp")).unwrap();
	let removed = storage::FileData { version: 1, accounts: HashMap::new() };
	assert!(storage::save(&path, &removed, 1, &gate).unwrap());
	assert!(storage::load(&path, &HashSet::from(["a".into()])).accounts.is_empty());
}

#[test]
fn load_salvages_accounts_preserves_corruption_and_newer_schema() {
	let dir = TestDirectory::new();
	let path = dir.cache();
	let account = AccountCache::default();
	let data = serde_json::json!({"version":1,"accounts":{"a":account,"b":{"entries":"bad"},"removed":account}});
	fs::write(&path, serde_json::to_vec(&data).unwrap()).unwrap();
	let configured = HashSet::from(["a".into(), "b".into()]);
	let loaded = storage::load(&path, &configured);
	assert!(loaded.accounts.contains_key("a"));
	assert!(!loaded.accounts.contains_key("b"));
	assert!(!loaded.accounts.contains_key("removed"));
	assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 2);
	fs::write(&path, b"{broken").unwrap();
	assert!(storage::load(&path, &configured).accounts.is_empty());
	fs::write(&path, b"{\"version\":2}").unwrap();
	let loaded = storage::load(&path, &configured);
	assert!(!loaded.writable);
	assert!(loaded.error.is_some());
	assert_eq!(fs::read(&path).unwrap(), b"{\"version\":2}");
}

#[test]
fn saved_revision_does_not_clean_newer_mutations() {
	let (mut c, _) = model();
	c.command(Command::Interaction("a".into(), entry("1", "@alice@example.org"), false));
	let saved = c.revision;
	c.command(Command::Interaction("a".into(), entry("2", "@bob@example.org"), false));
	c.command(Command::Saved(saved, Ok(true)));
	assert!(c.revision > c.saved);
	c.command(Command::Saved(c.revision, Err("disk full".into())));
	assert!(c.revision > c.saved);
	assert!(c.next_save > Instant::now());
	assert_eq!(c.service.session("a".into()).snapshot().entries.len(), 2);
}

#[test]
fn build_timing_survives_resuming_and_logs_completion_once() {
	let (mut c, jobs) = model();
	let (disk, messages) = mpsc::channel();
	c.disk = disk;
	let now = Instant::now();
	let started = chrono::Utc::now().timestamp() - 134;
	c.schedule(now, started, false);
	let first = jobs.recv().unwrap();
	c.command(Command::RequestStarted(first.owner.clone(), first.generation));
	assert_eq!(c.accounts["a"].build_timing.as_ref().unwrap().started_at, started);
	assert!(
		matches!(messages.try_recv().unwrap(), DiskCommand::Log(message) if message.contains("build started") && message.contains("account \"a\""))
	);
	page(&mut c, first, vec![], None);
	assert!(c.accounts["a"].build_timing.as_ref().unwrap().completed_at.is_none());
	let dir = TestDirectory::new();
	storage::save(&dir.cache(), &storage::FileData { version: 1, accounts: c.accounts.clone() }, 0, &c.gate).unwrap();
	c.accounts = storage::load(&dir.cache(), &HashSet::from(["a".into(), "b".into()])).accounts;
	assert_eq!(c.accounts["a"].build_timing.as_ref().unwrap().requests.as_ref().unwrap().attempts, 1);
	c.schedule(now + Duration::from_secs(2), started + 2, false);
	let second = jobs.recv().unwrap();
	c.command(Command::RequestStarted(second.owner.clone(), second.generation));
	c.command(Command::Fetched(second.clone(), Err("offline".into()), None));
	assert!(c.accounts["a"].build_timing.as_ref().unwrap().completed_at.is_none());
	assert!(messages.try_recv().is_err(), "Resume/retry must not restart timing");
	c.command(Command::RequestStarted(second.owner.clone(), second.generation));
	page(&mut c, second.clone(), vec![], None);
	let timing = c.accounts["a"].build_timing.as_ref().unwrap();
	assert_eq!(timing.started_at, started);
	assert!(timing.completed_at.unwrap() >= started + 134);
	assert!(
		matches!(messages.try_recv().unwrap(), DiskCommand::Log(message) if message.contains("build completed") && message.contains("total duration: 2 minutes") && message.contains("API requests: 3"))
	);
	let completed = c.accounts["a"].last_complete;
	page(&mut c, second, vec![], None);
	assert_eq!(c.accounts["a"].last_complete, completed);
	assert!(messages.try_recv().is_err(), "Replayed pages must not log a second completion");
	c.schedule(now + Duration::from_secs(86401), completed.unwrap() + 86401, false);
	let refreshed = c.accounts["a"].build_timing.as_ref().unwrap();
	assert_eq!(refreshed.started_at, completed.unwrap() + 86401);
	assert!(refreshed.completed_at.is_none());
	assert!(!refreshed.started_on_resume);
	assert_eq!(refreshed.requests.as_ref().unwrap().attempts, 0);
	assert!(matches!(messages.try_recv().unwrap(), DiskCommand::Log(message) if message.contains("build started")));
}

#[test]
fn legacy_partial_build_logs_only_time_since_resume() {
	let (mut c, jobs) = model();
	let (disk, messages) = mpsc::channel();
	c.disk = disk;
	let mut legacy = serde_json::to_value(&c.accounts["a"]).unwrap();
	legacy.as_object_mut().unwrap().remove("build_timing");
	legacy["sources"][0]["done"] = true.into();
	c.accounts.insert("a".into(), serde_json::from_value(legacy).unwrap());
	c.schedule(Instant::now(), chrono::Utc::now().timestamp(), false);
	assert!(c.accounts["a"].build_timing.as_ref().unwrap().started_on_resume);
	assert!(
		matches!(messages.try_recv().unwrap(), DiskCommand::Log(message) if message.contains("Original start time is unknown"))
	);
	page(&mut c, jobs.recv().unwrap(), vec![], None);
	assert!(
		matches!(messages.try_recv().unwrap(), DiskCommand::Log(message) if message.contains("time since resume:") && !message.contains("total duration:"))
	);
}

#[test]
fn request_counts_follow_the_originating_account_and_generation() {
	let (mut c, _) = model();
	for account in c.accounts.values_mut() {
		account.build_timing = Some(timing::BuildTiming::new(100, false));
	}
	c.command(Command::Pause);
	c.command(Command::RequestStarted("a".into(), 1));
	c.command(Command::RequestStarted("a".into(), 2));
	assert_eq!(c.accounts["a"].build_timing.as_ref().unwrap().requests.as_ref().unwrap().attempts, 1);
	assert_eq!(c.accounts["b"].build_timing.as_ref().unwrap().requests.as_ref().unwrap().attempts, 0);
	c.command(Command::Remove("a".into()));
	c.command(Command::RequestStarted("a".into(), 1));
	assert!(!c.accounts.contains_key("a"));
}

#[test]
fn mock_request_counts_include_failures_and_confirmation_batches() {
	let _network = MOCK_NETWORK.lock().unwrap();
	let mut failed = MockResponse::json(serde_json::json!([]));
	failed.status = 503;
	let (base, received) =
		server(vec![failed, MockResponse::json(serde_json::json!([])), MockResponse::json(serde_json::json!([]))]);
	let (c, _) = model();
	let mut request = job(&c, "a", 0);
	request.credentials.base = base;
	let attempts = std::cell::Cell::new(0);
	let observe = || attempts.set(attempts.get() + 1);
	assert!(fetch_job(&request, observe).is_err());
	assert!(fetch_job(&request, observe).is_ok());
	request.confirm = vec!["one".into(), "two".into()];
	// Even an incomplete confirmation response counts as one batch request.
	assert!(fetch_job(&request, observe).is_err());
	assert_eq!(attempts.get(), 3);
	for _ in 0..3 {
		received.recv_timeout(Duration::from_secs(3)).unwrap();
	}
	request.confirm.clear();
	request.next = Some("https://foreign.invalid/api/v1/accounts/me/following".into());
	assert!(fetch_job(&request, observe).is_err());
	assert_eq!(attempts.get(), 3, "A locally rejected URL must not count as an API request");
}

#[test]
fn shutdown_flushes_timing_log_and_reports_log_failure() {
	for fail in [false, true] {
		let dir = TestDirectory::new();
		let log = dir.0.join("autocomplete.log");
		if fail {
			fs::create_dir(&log).unwrap();
		} else {
			fs::write(&log, "Previous build\n").unwrap();
		}
		let (tx, rx) = mpsc::channel();
		let service = Service { tx, published: Arc::new(Mutex::new(Published::default())) };
		let mut c = Coordinator::new(dir.cache(), HashSet::from(["a".into()]), service.clone(), UiWaker::silent());
		let mut timing = timing::BuildTiming::new(100, false);
		c.disk.send(DiskCommand::Log(timing.start_message("a"))).unwrap();
		c.disk.send(DiskCommand::Log(timing.finish("a", 234).unwrap())).unwrap();
		service.shutdown();
		service.shutdown();
		c.run(&rx);
		let result = service.shutdown_result().unwrap();
		assert_eq!(result.is_err(), fail);
		assert!(dir.cache().is_file(), "Log failures must not stop cache saving");
		if fail {
			assert!(result.unwrap_err().contains("autocomplete.log"));
		} else {
			let text = fs::read_to_string(log).unwrap();
			assert_eq!(text.lines().count(), 3);
			assert!(text.starts_with("Previous build\n"));
			assert!(text.contains("total duration: 2 minutes 14 seconds"));
		}
	}
}

fn wait_for_shutdown(service: &Service) -> Result<(), String> {
	let deadline = Instant::now() + Duration::from_secs(5);
	loop {
		if let Some(result) = service.shutdown_result() {
			return result;
		}
		assert!(Instant::now() < deadline, "Shutdown did not cross the final-save barrier");
		thread::sleep(Duration::from_millis(5));
	}
}

#[test]
fn asynchronous_load_queues_updates_and_shutdown_saves_them() {
	let dir = TestDirectory::new();
	let service = Service::start(dir.cache(), HashSet::from(["a".into()]), UiWaker::silent());
	service.session("a".into()).interaction(entry("1", "@alice@example.org"));
	service.shutdown();
	wait_for_shutdown(&service).unwrap();
	let loaded = storage::load(&dir.cache(), &HashSet::from(["a".into()]));
	assert_eq!(loaded.accounts["a"].entries.len(), 1);
	assert!(!fs::read_to_string(dir.cache()).unwrap().contains("secret"));
}

#[test]
fn shutdown_new_schema_and_final_failure_preserve_previous_file() {
	let dir = TestDirectory::new();
	fs::write(dir.cache(), b"{\"version\":2}").unwrap();
	let service = Service::start(dir.cache(), HashSet::from(["a".into()]), UiWaker::silent());
	service.session("a".into()).interaction(entry("1", "@alice@example.org"));
	service.shutdown();
	assert!(wait_for_shutdown(&service).is_err());
	assert_eq!(fs::read(dir.cache()).unwrap(), b"{\"version\":2}");
	let data = storage::FileData { version: 1, accounts: HashMap::new() };
	storage::save(&dir.cache(), &data, 0, &Arc::new(Mutex::new(0))).unwrap();
	let before = fs::read(dir.cache()).unwrap();
	fs::create_dir(dir.cache().with_extension("json.tmp")).unwrap();
	let service = Service::start(dir.cache(), HashSet::from(["a".into()]), UiWaker::silent());
	service.session("a".into()).interaction(entry("1", "@alice@example.org"));
	service.shutdown();
	assert!(wait_for_shutdown(&service).is_err());
	assert_eq!(fs::read(dir.cache()).unwrap(), before);
}

fn relationship(id: &str, following: bool, blocking: bool) -> Relationship {
	serde_json::from_value(serde_json::json!({"id":id,"following":following,"blocking":blocking,"showing_reblogs":true,"notifying":false,"followed_by":false,"muting":false,"muting_notifications":false,"requested":false,"domain_blocking":false,"endorsed":false,"note":""})).unwrap()
}

#[test]
fn unchanged_relationship_observation_preserves_inflight_following_page() {
	for cached in [false, true] {
		let (mut c, _) = model();
		if cached {
			c.command(Command::Interaction("a".into(), entry("1", "@old@example.org"), true));
		}
		let pending = job(&c, "a", 0);
		c.command(Command::ObservedRelationships("a".into(), vec![relationship("1", true, false)], c.revision));
		let cursor = "https://test.invalid/api/v1/accounts/me/following?cursor=next";
		page(&mut c, pending, vec![entry("1", "@alice@example.org")], Some(cursor));
		assert_eq!(c.accounts["a"].sources[0].next.as_deref(), Some(cursor));
		let snapshot = c.service.session("a".into()).snapshot();
		assert_eq!(snapshot.entries.len(), 1, "The pending page must publish its entry");
		assert_eq!(snapshot.entries[0].1.address, "@alice@example.org");
		let next = job(&c, "a", 0);
		page(&mut c, next, vec![], None);
		let followers = job(&c, "a", 1);
		page(&mut c, followers, vec![], None);
		assert!(c.accounts["a"].following.contains("1"));
		assert!(c.accounts["a"].confirm.is_empty());
		let snapshot = c.service.session("a".into()).snapshot();
		assert_eq!(snapshot.entries[0].1.address, "@alice@example.org");
		assert!(matches!(snapshot.availability, Availability::Usable));
	}
}

#[test]
fn changed_blocking_observation_still_invalidates_inflight_pages() {
	for was_blocked in [false, true] {
		let (mut c, _) = model();
		if was_blocked {
			c.command(Command::Relationship("a".into(), "1".into(), Change::Block));
		}
		let pending = job(&c, "a", 0);
		c.command(Command::ObservedRelationships("a".into(), vec![relationship("1", true, !was_blocked)], c.revision));
		page(&mut c, pending, vec![entry("1", "@stale@example.org")], None);
		assert!(c.accounts["a"].entries.is_empty());
		assert!(!c.accounts["a"].following.contains("1"));
		assert_eq!(c.accounts["a"].blocked.contains("1"), !was_blocked);
	}
}

#[test]
fn authoritative_refresh_confirms_missing_following_and_rejects_stale_relationships() {
	let (mut c, _) = model();
	c.command(Command::Interaction("a".into(), entry("1", "@alice@example.org"), true));
	c.accounts.get_mut("a").unwrap().scan_revision = c.revision;
	c.accounts.get_mut("a").unwrap().scanning_following.clear();
	let request = job(&c, "a", 0);
	page(&mut c, request, vec![], None);
	assert_eq!(c.accounts["a"].confirm, ["1"]);
	assert!(c.accounts["a"].entries.contains_key("1"));
	let mut confirmation = job(&c, "a", 0);
	confirmation.confirm = vec!["1".into()];
	c.command(Command::Fetched(confirmation, Ok(Fetched::Relationships(vec![relationship("1", false, false)])), None));
	assert!(c.accounts["a"].unfollowed.contains("1"));
	assert!(!c.accounts["a"].entries.contains_key("1"));
	let issued = c.revision;
	c.command(Command::Relationship("a".into(), "1".into(), Change::Block));
	c.command(Command::ObservedRelationships("a".into(), vec![relationship("1", false, false)], issued));
	assert!(c.accounts["a"].blocked.contains("1"));
	c.command(Command::ObservedRelationships("a".into(), vec![relationship("1", false, false)], c.revision));
	assert!(!c.accounts["a"].blocked.contains("1"));
	assert!(c.accounts["a"].unfollowed.contains("1"));
}

struct MockResponse {
	status: u16,
	headers: String,
	body: String,
	delay: Duration,
}
impl MockResponse {
	fn json(body: serde_json::Value) -> Self {
		Self { status: 200, headers: String::new(), body: body.to_string(), delay: Duration::ZERO }
	}
}

fn server(responses: Vec<MockResponse>) -> (url::Url, mpsc::Receiver<String>) {
	let _ = rustls::crypto::ring::default_provider().install_default();
	use std::{
		io::{Read, Write},
		net::TcpListener,
	};
	let listener = TcpListener::bind("127.0.0.1:0").unwrap();
	let base = url::Url::parse(&format!("http://{}/", listener.local_addr().unwrap())).unwrap();
	let origin = base.origin().ascii_serialization();
	let (sent, received) = mpsc::channel();
	thread::spawn(move || {
		listener.set_nonblocking(true).unwrap();
		let deadline = Instant::now() + Duration::from_secs(10);
		for response in responses {
			let mut stream = loop {
				match listener.accept() {
					Ok((stream, _)) => break stream,
					Err(error) if error.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < deadline => {
						thread::sleep(Duration::from_millis(5))
					}
					_ => return,
				}
			};
			stream.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
			let mut request = Vec::new();
			let mut buffer = [0; 4096];
			while let Ok(n) = stream.read(&mut buffer) {
				if n == 0 {
					break;
				}
				request.extend_from_slice(&buffer[..n]);
				if request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
					break;
				}
			}
			let _ = sent.send(String::from_utf8_lossy(&request).into_owned());
			thread::sleep(response.delay);
			let header = response.headers.replace("{origin}", &origin);
			let wire = format!(
				"HTTP/1.1 {} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n{}\r\n{}",
				response.status,
				response.body.len(),
				header,
				response.body
			);
			let _ = stream.write_all(wire.as_bytes());
		}
	});
	(base, received)
}

fn account_json(id: &str, acct: &str) -> serde_json::Value {
	serde_json::json!({"id":id,"username":acct.split('@').next().unwrap(),"acct":acct,"display_name":"Alice","url":"https://example.org/@alice"})
}

fn unavailable_account_json(id: &str, moved: bool) -> serde_json::Value {
	let mut account = account_json(id, "alice@old.example");
	if moved {
		account["moved"] = account_json("destination", "alice@new.example");
	} else {
		account["suspended"] = true.into();
	}
	account
}

#[test]
fn unavailable_accounts_are_excluded_from_interactions_and_survive_restart() {
	for moved in [false, true] {
		for follow in [false, true] {
			let (mut c, _) = model();
			let active = entry("1", "@alice@old.example");
			c.command(Command::Interaction("a".into(), active.clone(), follow));
			let pending = job(&c, "a", 0);
			let account: Account = serde_json::from_value(unavailable_account_json("1", moved)).unwrap();
			let unavailable = Entry::from_account(&account, &pending.credentials.base);
			c.command(Command::Interaction("a".into(), unavailable, follow));
			assert!(c.service.session("a".into()).snapshot().entries.is_empty());
			assert!(c.accounts["a"].unavailable.contains("1"));
			assert!(!c.accounts["a"].entries.contains_key("destination"));
			let dir = TestDirectory::new();
			storage::save(&dir.cache(), &storage::FileData { version: 1, accounts: c.accounts.clone() }, 0, &c.gate)
				.unwrap();
			c.accounts = storage::load(&dir.cache(), &HashSet::from(["a".into(), "b".into()])).accounts;
			// An older page or a cached reply/quote author cannot resurrect the old account.
			page(&mut c, pending, vec![active.clone()], None);
			c.command(Command::Interaction("a".into(), active.clone(), follow));
			assert!(c.accounts["a"].entries.is_empty());
			// A later page can establish that a suspension or redirect has been reversed.
			let fresh = job(&c, "a", 1);
			page(&mut c, fresh, vec![active], None);
			assert!(!c.accounts["a"].unavailable.contains("1"));
			assert_eq!(c.accounts["a"].entries.len(), 1);
		}
	}
}

#[test]
fn stale_unavailable_pages_do_not_remove_newer_entries() {
	let (mut c, _) = model();
	let pending = job(&c, "a", 0);
	let account: Account = serde_json::from_value(unavailable_account_json("1", true)).unwrap();
	let unavailable = Entry::from_account(&account, &pending.credentials.base);
	c.command(Command::Interaction("a".into(), entry("1", "@alice@example.org"), true));
	page(&mut c, pending, vec![unavailable], None);
	assert_eq!(c.accounts["a"].entries.len(), 1);
	assert!(c.accounts["a"].unavailable.is_empty());
	// Old cache files do not have the new exclusion set.
	let mut legacy = serde_json::to_value(&c.accounts["a"]).unwrap();
	legacy.as_object_mut().unwrap().remove("unavailable");
	let restored: AccountCache = serde_json::from_value(legacy).unwrap();
	assert!(restored.valid());
	assert_eq!(restored.entries.len(), 1);
}

#[test]
fn mock_pages_filter_unavailable_accounts_without_extra_requests() {
	let _network = MOCK_NETWORK.lock().unwrap();
	for source in [0, 1] {
		let mut first = MockResponse::json(serde_json::json!([
			unavailable_account_json("moved", true),
			unavailable_account_json("suspended", false)
		]));
		let endpoint = if source == 0 { "following" } else { "followers" };
		first.headers = format!("Link: <{{origin}}/api/v1/accounts/me/{endpoint}?cursor=next>; rel=\"next\"\r\n");
		let mut active = account_json("active", "alice@one.example");
		active["moved"] = serde_json::Value::Null;
		active["suspended"] = false.into();
		let (base, requests) = server(vec![
			first,
			MockResponse::json(serde_json::json!([active, account_json("other", "alice@two.example")])),
		]);
		let (mut c, _) = model();
		c.credentials.get_mut("a").unwrap().base = base;
		c.command(Command::Interaction("a".into(), entry("moved", "@alice@old.example"), false));
		for _ in 0..2 {
			let request = job(&c, "a", source);
			let result = fetch_job(&request, || {}).unwrap();
			c.command(Command::Fetched(request, Ok(result), None));
			assert!(requests.recv_timeout(Duration::from_secs(2)).unwrap().contains(endpoint));
			assert!(!c.accounts["a"].entries.contains_key("moved"));
			assert!(!c.accounts["a"].entries.contains_key("suspended"));
		}
		assert!(requests.try_recv().is_err());
		assert!(c.accounts["a"].sources[source].done);
		assert_eq!(c.accounts["a"].entries.len(), 2, "Active accounts with the same username stay distinct");
		assert!(c.accounts["a"].entries.contains_key("active"));
		assert!(c.accounts["a"].entries.contains_key("other"));
	}
}

#[test]
fn mock_pagination_preserves_opaque_links_and_rejects_foreign_urls_and_redirects() {
	let _network = MOCK_NETWORK.lock().unwrap();
	let mut first = MockResponse::json(serde_json::json!([account_json("server-local-id", "alice@example.org")]));
	first.headers = "Link: <{origin}/api/v1/accounts/me/following?cursor=opaque%2Bvalue>; rel=\"next\"\r\n".into();
	let (base, requests) = server(vec![first, MockResponse::json(serde_json::json!([]))]);
	let client = MastodonClient::for_autocomplete(base.clone()).unwrap();
	let page = client.autocomplete_page("test-token", "me", true, None).unwrap();
	assert_eq!(page.accounts[0].id, "server-local-id");
	assert!(page.next.as_ref().unwrap().ends_with("cursor=opaque%2Bvalue"));
	assert!(client.autocomplete_page("test-token", "me", true, page.next.as_deref()).unwrap().next.is_none());
	let first = requests.recv_timeout(Duration::from_secs(2)).unwrap();
	assert!(first.to_lowercase().contains("authorization: bearer test-token"));
	assert!(requests.recv_timeout(Duration::from_secs(2)).unwrap().contains("cursor=opaque%2Bvalue"));
	assert!(
		client
			.autocomplete_page("test-token", "me", true, Some("https://evil.invalid/api/v1/accounts/me/following"))
			.is_err()
	);
	assert!(
		client
			.autocomplete_page(
				"test-token",
				"me",
				true,
				Some(base.join("api/v1/accounts/other/following").unwrap().as_str())
			)
			.is_err()
	);
	let mut redirect = MockResponse::json(serde_json::json!([]));
	redirect.status = 302;
	redirect.headers = "Location: https://evil.invalid/\r\n".into();
	let (base, _) = server(vec![redirect]);
	assert!(MastodonClient::for_autocomplete(base).unwrap().autocomplete_page("test-token", "me", true, None).is_err());
}

#[test]
fn mock_rate_observations_are_shared_and_respect_both_deadlines() {
	let _network = MOCK_NETWORK.lock().unwrap();
	let mut response = MockResponse::json(serde_json::json!([]));
	response.status = 429;
	response.headers = format!(
		"Retry-After: 60\r\nX-RateLimit-Limit: 100\r\nX-RateLimit-Remaining: 0\r\nX-RateLimit-Reset: {}\r\n",
		(chrono::Utc::now() + chrono::Duration::seconds(90)).to_rfc3339()
	);
	let (base, _) = server(vec![response]);
	let client = MastodonClient::new(base.clone()).unwrap();
	let error = client.get_following_page("token", "me", None).err().unwrap();
	assert_eq!(
		error.downcast_ref::<reqwest::Error>().and_then(reqwest::Error::status).map(|s| s.as_u16()),
		Some(429),
		"Expected mock server response: {error:#}"
	);
	let now = Instant::now();
	let deadline = rate::defer_until(&base.origin().ascii_serialization(), now).unwrap();
	assert!(deadline > now + Duration::from_secs(85));
	assert!(rate::defer_until(&base.origin().ascii_serialization(), now + Duration::from_secs(95)).is_none());
	for (limit, remaining, defer) in [(5, 1, true), (5, 5, false), (1, 1, false), (1000, 100, true), (1000, 101, false)]
	{
		let mut response = MockResponse::json(serde_json::json!([]));
		response.headers = format!(
			"X-RateLimit-Limit: {limit}\r\nX-RateLimit-Remaining: {remaining}\r\nX-RateLimit-Reset: {}\r\n",
			(chrono::Utc::now() + chrono::Duration::seconds(60)).to_rfc3339()
		);
		let (base, _) = server(vec![response]);
		MastodonClient::new(base.clone()).unwrap().get_following_page("token", "me", None).unwrap();
		assert_eq!(rate::defer_until(&base.origin().ascii_serialization(), Instant::now()).is_some(), defer);
	}
	assert!((30..=33).contains(&rate::backoff(0).as_secs()));
	assert_eq!(rate::backoff(100).as_secs(), 300);
}

#[test]
fn shutdown_during_inflight_fetch_does_not_wait_for_network_or_backoff() {
	let _network = MOCK_NETWORK.lock().unwrap();
	let dir = TestDirectory::new();
	let mut response = MockResponse::json(serde_json::json!([]));
	response.delay = Duration::from_secs(2);
	let (base, requests) = server(vec![response]);
	let service = Service::start(dir.cache(), HashSet::from(["a".into()]), UiWaker::silent());
	service.activate("a".into(), base, "secret".into(), "me".into());
	requests.recv_timeout(Duration::from_secs(5)).unwrap();
	let started = Instant::now();
	service.shutdown();
	wait_for_shutdown(&service).unwrap();
	assert!(started.elapsed() < Duration::from_secs(1));
}

#[test]
fn successful_interaction_survives_dropped_ui_receiver_and_account_switch() {
	let _network = MOCK_NETWORK.lock().unwrap();
	let dir = TestDirectory::new();
	let status = serde_json::json!({"id":"post","content":"hello","created_at":"2026-01-01T00:00:00Z","account":account_json("local-author", "alice@example.org"),"spoiler_text":"","visibility":"public","reblogs_count":0,"favourites_count":1,"replies_count":0});
	let (base, requests) = server(vec![MockResponse::json(status)]);
	let service = Service::start(dir.cache(), HashSet::from(["a".into(), "b".into()]), UiWaker::silent());
	let handle =
		crate::network::start_network(base, "token".into(), UiWaker::silent(), service.session("a".into())).unwrap();
	handle.send(crate::network::NetworkCommand::Favorite { status_id: "post".into() });
	drop(handle);
	service.published.lock().unwrap().active = Some("b".into());
	requests.recv_timeout(Duration::from_secs(3)).unwrap();
	let deadline = Instant::now() + Duration::from_secs(3);
	while service.session("a".into()).snapshot().entries.is_empty() {
		assert!(Instant::now() < deadline);
		thread::sleep(Duration::from_millis(5));
	}
	assert_eq!(service.session("a".into()).snapshot().entries[0].1.id, "local-author");
	assert!(service.session("b".into()).snapshot().entries.is_empty());
	service.shutdown();
	wait_for_shutdown(&service).unwrap();
}

#[test]
fn background_total_timeout_is_bounded() {
	let _network = MOCK_NETWORK.lock().unwrap();
	let mut response = MockResponse::json(serde_json::json!([]));
	response.delay = Duration::from_millis(300);
	let (base, _) = server(vec![response]);
	let client =
		MastodonClient::autocomplete_with_timeouts(base, Duration::from_millis(50), Duration::from_millis(100))
			.unwrap();
	let started = Instant::now();
	let error = client.autocomplete_page("token", "me", true, None).err().unwrap();
	assert!(error.downcast_ref::<reqwest::Error>().is_some_and(reqwest::Error::is_timeout));
	assert!(started.elapsed() < Duration::from_secs(1));
}

#[test]
fn foreground_priority_refresh_deadline_and_credentials_generation() {
	let (mut c, jobs) = model();
	let now = Instant::now();
	c.next_fetch = now;
	c.schedule(now, 100, true);
	assert!(jobs.try_recv().is_err());
	c.schedule(now + Duration::from_secs(1), 101, false);
	let old = jobs.try_recv().unwrap();
	let mut credentials = c.credentials["a"].clone();
	credentials.token = "replacement-token".into();
	c.command(Command::Activate("a".into(), credentials));
	page(&mut c, old, vec![entry("1", "@stale@example.org")], None);
	assert!(c.accounts["a"].entries.is_empty());
	let account = c.accounts.get_mut("a").unwrap();
	account.sources.iter_mut().for_each(|source| source.done = true);
	account.last_complete = Some(100);
	account.entries.insert("1".into(), entry("1", "@alice@example.org"));
	c.schedule(now + Duration::from_secs(2), 200, false);
	assert!(jobs.try_recv().is_err());
	c.schedule(now + Duration::from_secs(86400), 86500, false);
	assert_eq!(jobs.try_recv().unwrap().source, 0);
	assert_eq!(c.accounts["a"].entries.len(), 1, "Refresh must preserve saved data");
}

#[test]
fn rejected_checkpoint_restarts_only_failed_source_and_retains_data() {
	let (mut c, _) = model();
	let request = job(&c, "a", 0);
	page(
		&mut c,
		request,
		vec![entry("1", "@alice@example.org")],
		Some("https://test.invalid/api/v1/accounts/me/following?cursor=expired"),
	);
	let request = job(&c, "a", 0);
	c.command(Command::Fetched(request, Err("expired cursor".into()), Some(400)));
	assert!(c.accounts["a"].sources[0].next.is_none());
	assert!(c.accounts["a"].sources[0].retry.is_some());
	assert!(!c.accounts["a"].sources[1].done);
	assert_eq!(c.accounts["a"].entries.len(), 1);
}
