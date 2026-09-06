//! Per-event notification sounds.
//!
//! Fedra historically played a single `sounds/boop.mp3` for every notification. This module
//! keeps that as the fallback and lets each event take a sound of its own instead, independently
//! switchable by the user. No per-event audio is bundled: the sounds are whatever the user puts
//! in their sounds folder, so an untouched install behaves exactly as it always did.
//!
//! Playback uses one [`MediaCtrl`] per event. `MediaCtrl::load` is asynchronous and the backend
//! does not reliably deliver a `Loaded` event, so nothing here waits for one: controls are loaded
//! up front by [`SoundPlayer::preload`] at startup, exactly as the single-sound implementation
//! did, and playing is then just stop-then-play. A control created later, because the user picked
//! a different file, is played immediately and retried from the `Loaded` handler if that event
//! does happen to arrive.
//!
//! Volume can only be set once a file is loaded, so it is applied after `load` and again on every
//! play rather than once at construction.

use std::{
	cell::{Cell, RefCell},
	collections::HashMap,
	path::{Path, PathBuf},
	rc::Rc,
};

use wxdragon::prelude::*;

/// A user-facing event that can have its own sound.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SoundEvent {
	Mention,
	DirectMessage,
	HomePost,
	Follow,
	FollowRequest,
	Favorite,
	Boost,
	Bookmark,
	Pin,
	PollEnded,
	PollVoted,
	PostEdited,
	PostSent,
	PostDeleted,
	Error,
}

impl SoundEvent {
	/// Every event, in the order they are listed in the options dialog.
	pub const ALL: [Self; 15] = [
		Self::Mention,
		Self::DirectMessage,
		Self::HomePost,
		Self::Follow,
		Self::FollowRequest,
		Self::Favorite,
		Self::Boost,
		Self::Bookmark,
		Self::Pin,
		Self::PollEnded,
		Self::PollVoted,
		Self::PostEdited,
		Self::PostSent,
		Self::PostDeleted,
		Self::Error,
	];

	/// Stable identifier used as the config key and as the default file stem.
	#[must_use]
	pub const fn key(self) -> &'static str {
		match self {
			Self::Mention => "mention",
			Self::DirectMessage => "direct_message",
			Self::HomePost => "home_post",
			Self::Follow => "follow",
			Self::FollowRequest => "follow_request",
			Self::Favorite => "favorite",
			Self::Boost => "boost",
			Self::Bookmark => "bookmark",
			Self::Pin => "pin",
			Self::PollEnded => "poll_ended",
			Self::PollVoted => "poll_voted",
			Self::PostEdited => "post_edited",
			Self::PostSent => "post_sent",
			Self::PostDeleted => "post_deleted",
			Self::Error => "error",
		}
	}

	/// Human-readable name shown in the options dialog.
	#[must_use]
	pub const fn label(self) -> &'static str {
		match self {
			Self::Mention => "Mention",
			Self::DirectMessage => "Direct message",
			Self::HomePost => "New post in home timeline",
			Self::Follow => "Followed someone, or gained a follower",
			Self::FollowRequest => "Follow request",
			Self::Favorite => "Favorited a post, or yours was favorited",
			Self::Boost => "Boosted a post, or yours was boosted",
			Self::Bookmark => "Bookmarked or unbookmarked a post",
			Self::Pin => "Pinned or unpinned a post",
			Self::PollEnded => "Poll ended",
			Self::PollVoted => "Voted in a poll",
			Self::PostEdited => "Edited a post, or one you follow was edited",
			Self::PostSent => "Sent a post or reply",
			Self::PostDeleted => "Deleted a post",
			Self::Error => "Action failed",
		}
	}

	/// Conventional filename for this event inside a sound pack.
	#[must_use]
	pub fn default_file(self) -> String {
		format!("{}.mp3", self.key())
	}

	/// Whether this event makes a sound out of the box.
	///
	/// Every event starts enabled. Home-timeline posts are the one that can fire continuously on a
	/// busy feed, so its sound is deliberately the quietest and shortest of the set; unchecking it
	/// in the Sounds tab silences it without touching the rest.
	#[must_use]
	pub const fn default_enabled(self) -> bool {
		let _ = self;
		true
	}

	/// Map a Mastodon notification `type` onto a sound.
	///
	/// Returns `None` for kinds Fedra has no sound for, so new server-side notification types
	/// stay silent rather than borrowing another event's sound.
	#[must_use]
	pub fn from_notification_kind(kind: &str) -> Option<Self> {
		match kind {
			"mention" => Some(Self::Mention),
			"status" => Some(Self::HomePost),
			"follow" => Some(Self::Follow),
			"follow_request" => Some(Self::FollowRequest),
			"favourite" | "favorite" => Some(Self::Favorite),
			"reblog" => Some(Self::Boost),
			"poll" => Some(Self::PollEnded),
			"update" => Some(Self::PostEdited),
			_ => None,
		}
	}
}

/// One [`MediaCtrl`] and the file it holds.
struct Slot {
	ctrl: MediaCtrl,
	path: PathBuf,
}

/// Owns the media controls used for notification sounds.
pub struct SoundPlayer {
	parent: Frame,
	slots: RefCell<HashMap<SoundEvent, Slot>>,
	/// Shared so the deferred `Loaded` handlers pick up later volume changes.
	volume: Rc<Cell<f64>>,
}

impl SoundPlayer {
	/// Create a player whose controls will be parented to `parent`.
	#[must_use]
	pub fn new(parent: Frame) -> Self {
		Self { parent, slots: RefCell::new(HashMap::new()), volume: Rc::new(Cell::new(0.8)) }
	}

	/// Directory installed alongside the executable, holding [`Self::FALLBACK_FILE`].
	#[must_use]
	pub fn bundled_dir() -> PathBuf {
		std::env::current_exe()
			.ok()
			.and_then(|path| path.parent().map(|p| p.join("sounds")))
			.unwrap_or_else(|| PathBuf::from("sounds"))
	}

	/// Per-user sound directory, which takes precedence over the bundled one.
	///
	/// Custom sounds live here so that reinstalling or updating Fedra, which overwrites the
	/// bundled directory, cannot delete them.
	#[must_use]
	pub fn user_dir() -> PathBuf {
		crate::config::config_dir().join("sounds")
	}

	/// Resolve a configured sound value to a file on disk.
	///
	/// Absolute paths are used verbatim; bare names are looked up in the user directory first and
	/// then in the bundled one.
	#[must_use]
	pub fn resolve(file: &str) -> Option<PathBuf> {
		if file.trim().is_empty() {
			return None;
		}
		let as_path = Path::new(file);
		if as_path.is_absolute() {
			return as_path.exists().then(|| as_path.to_path_buf());
		}
		let user = Self::user_dir().join(file);
		if user.exists() {
			return Some(user);
		}
		let bundled = Self::bundled_dir().join(file);
		bundled.exists().then_some(bundled)
	}

	/// Extensions a sound may use, in the order they are tried.
	const EXTENSIONS: [&'static str; 5] = ["mp3", "wav", "ogg", "flac", "m4a"];

	/// The two folders packs are read from, personal first so it can shadow a shipped pack.
	fn pack_roots() -> [PathBuf; 2] {
		[Self::user_dir().join("packs"), Self::bundled_dir().join("packs")]
	}

	/// The one sound Fedra ships, played by any event nothing else provides.
	///
	/// Fedra has always played this file for every notification, and it stays the fallback so a
	/// fresh install sounds exactly as it did before per-event sounds existed. Nothing is bundled
	/// per event on purpose: the point of the feature is that the sounds are the user's own, and a
	/// fixed set would both impose one person's taste and put redistribution terms on the
	/// repository that it should not have to carry.
	pub const FALLBACK_FILE: &'static str = "boop.mp3";

	/// Path to `event`'s sound, from the pack if it has one.
	///
	/// Tries `pack`, then the default pack, so a pack missing a sound borrows it rather than
	/// leaving that event silent, and finally [`Self::FALLBACK_FILE`]. An event is therefore
	/// silent only when the user has switched it off.
	#[must_use]
	pub fn pack_file(pack: &str, event: SoundEvent) -> Option<String> {
		Self::pack_file_exact(pack, event)
			.or_else(|| {
				(pack != crate::config::DEFAULT_SOUND_PACK)
					.then(|| Self::pack_file_exact(crate::config::DEFAULT_SOUND_PACK, event))
					.flatten()
			})
			.or_else(|| Some(Self::FALLBACK_FILE.to_string()))
	}

	/// Path to `event`'s sound in `pack` only, without falling back to another pack.
	#[must_use]
	pub fn pack_file_exact(pack: &str, event: SoundEvent) -> Option<String> {
		if pack.trim().is_empty() {
			return None;
		}
		for root in Self::pack_roots() {
			for extension in Self::EXTENSIONS {
				let candidate = root.join(pack).join(format!("{}.{extension}", event.key()));
				if candidate.exists() {
					return Some(candidate.to_string_lossy().into_owned());
				}
			}
		}
		// Older layouts kept the sounds loose in the sounds folder rather than in a pack.
		if pack == crate::config::DEFAULT_SOUND_PACK {
			for root in [Self::user_dir(), Self::bundled_dir()] {
				let candidate = root.join(event.default_file());
				if candidate.exists() {
					return Some(candidate.to_string_lossy().into_owned());
				}
			}
		}
		None
	}

	/// Every pack found on disk, sorted, with the default first if present.
	#[must_use]
	pub fn list_packs() -> Vec<String> {
		let mut packs: Vec<String> = Vec::new();
		for root in Self::pack_roots() {
			let Ok(entries) = std::fs::read_dir(&root) else { continue };
			for entry in entries.flatten() {
				if !entry.path().is_dir() {
					continue;
				}
				let Some(name) = entry.file_name().to_str().map(str::to_owned) else { continue };
				if !packs.contains(&name) {
					packs.push(name);
				}
			}
		}
		packs.sort_by_key(|name| (name != crate::config::DEFAULT_SOUND_PACK, name.to_lowercase()));
		if packs.is_empty() {
			packs.push(crate::config::DEFAULT_SOUND_PACK.to_string());
		}
		packs
	}

	/// How many of the events `pack` actually provides, used to warn about an incomplete pack.
	#[must_use]
	pub fn pack_coverage(pack: &str) -> usize {
		SoundEvent::ALL.iter().filter(|event| Self::pack_file_exact(pack, **event).is_some()).count()
	}

	/// Turn a pack folder name into something worth reading aloud.
	#[must_use]
	pub fn pack_display_name(pack: &str) -> String {
		let mut out = String::with_capacity(pack.len());
		for (index, word) in pack.split(['_', '-']).filter(|w| !w.is_empty()).enumerate() {
			if index > 0 {
				out.push(' ');
			}
			let mut chars = word.chars();
			if let Some(first) = chars.next() {
				out.extend(first.to_uppercase());
				out.push_str(chars.as_str());
			}
		}
		if out.is_empty() { pack.to_string() } else { out }
	}

	/// Create the user sound folder, and the `packs` folder inside it, with a readme.
	///
	/// Fedra bundles no per-event audio, so the folder starts empty and the readme has to do the
	/// explaining: which names the events answer to, and what to drop in. Opening a bare folder
	/// with no hint of either is the failure this avoids. Existing files are never overwritten.
	///
	/// Returns the folder either way, so a failure to seed still opens something.
	pub fn seed_user_dir() -> PathBuf {
		let dir = Self::user_dir();
		if std::fs::create_dir_all(&dir).is_err() {
			return dir;
		}
		let _ = std::fs::create_dir_all(dir.join("packs"));
		let readme = dir.join("readme.txt");
		if !readme.exists() {
			let names = SoundEvent::ALL.iter().map(|event| event.default_file()).collect::<Vec<_>>().join("\r\n  ");
			let _ = std::fs::write(
				&readme,
				format!(
					"Fedra notification sounds\r\n\
					 \r\n\
					 Fedra ships one sound, {fallback}, and plays it for every event until you say \
					 otherwise. Anything you put here takes its place.\r\n\
					 \r\n\
					 To change one event, use Change in the Sounds tab and point it at any file on \
					 disk.\r\n\
					 \r\n\
					 To change them all at once, make a pack: create a folder under packs, name it \
					 whatever you like, and put files in it named after the events they play for. It \
					 appears in the Sound pack list the next time you open the options. A pack may \
					 use mp3, wav, ogg, flac, or m4a, and any event a pack does not provide falls \
					 back to {fallback} rather than going silent, so a pack of one file is perfectly \
					 valid.\r\n\
					 \r\n\
					 The names, one per event:\r\n\
					 \r\n  {names}\r\n\
					 \r\n\
					 This folder is yours. Updating Fedra never touches it.\r\n",
					fallback = Self::FALLBACK_FILE,
				),
			);
		}
		dir
	}

	/// Set the output volume for every current and future sound. `volume` is 0-100.
	pub fn set_volume(&self, volume: u8) {
		let scaled = f64::from(volume.min(100)) / 100.0;
		self.volume.set(scaled);
		for slot in self.slots.borrow().values() {
			slot.ctrl.set_volume(scaled);
		}
	}

	/// Load a control for every event that has a usable sound file.
	///
	/// Called at startup so a control is fully loaded well before the first notification, which is
	/// what makes playback reliable without depending on the `Loaded` event.
	pub fn preload(&self, settings: &crate::config::SoundSettings) {
		for event in SoundEvent::ALL {
			// Resolve through the settings so the active pack and any per-event override agree
			// with what will actually be played later.
			if let Some(file) = settings.file_for(event)
				&& let Some(path) = Self::resolve(&file)
			{
				self.ensure_slot(event, path, false);
			}
		}
	}

	/// Load just one event's sound, for auditioning without touching the other fourteen.
	pub fn preload_one(&self, settings: &crate::config::SoundSettings, event: SoundEvent) {
		let file = settings.file_for(event).or_else(|| Self::pack_file(&settings.pack, event)).unwrap_or_default();
		if let Some(path) = Self::resolve(&file) {
			self.ensure_slot(event, path, false);
		}
	}

	/// Drop every cached control, tearing down the windows behind them.
	///
	/// `MediaCtrl` is a `Copy` handle with no `Drop`, so the child windows have to be destroyed
	/// explicitly or each rebuild would orphan one.
	pub fn invalidate_all(&self) {
		for (_, slot) in self.slots.borrow_mut().drain() {
			slot.ctrl.destroy();
		}
	}

	/// Create and load the control for `event`, replacing any control holding a different file.
	///
	/// `play_when_ready` starts playback as soon as the file is loaded, for the case where a sound
	/// is played before it was ever preloaded.
	fn ensure_slot(&self, event: SoundEvent, path: PathBuf, play_when_ready: bool) {
		let mut slots = self.slots.borrow_mut();
		if let Some(slot) = slots.get_mut(&event) {
			// Already holding this exact file; nothing to do.
			if slot.path == path {
				if play_when_ready {
					slot.ctrl.stop();
					slot.ctrl.play();
				}
				return;
			}
			// Point the existing control at the new file rather than building another one.
			// Creating a MediaCtrl is far more expensive than loading into one, and switching
			// packs changes every event at once, so rebuilding here made the dialog crawl.
			if slot.ctrl.load(&path.to_string_lossy()) {
				slot.path = path;
				slot.ctrl.set_volume(self.volume.get());
				if play_when_ready {
					slot.ctrl.play();
				}
				return;
			}
			// The control refused the new file, so fall through and build a fresh one.
			if let Some(stale) = slots.remove(&event) {
				stale.ctrl.destroy();
			}
		}
		let ctrl = MediaCtrl::builder(&self.parent).with_size(Size::new(0, 0)).build();
		let volume = self.volume.clone();
		let pending_play = Rc::new(Cell::new(play_when_ready));
		let ctrl_for_event = ctrl;
		{
			let pending_play = pending_play.clone();
			let volume = volume.clone();
			// Only a safety net. The backend may never send this, which is why playback does not
			// depend on it.
			ctrl.on_loaded(move |_| {
				ctrl_for_event.set_volume(volume.get());
				if pending_play.replace(false) {
					ctrl_for_event.stop();
					ctrl_for_event.play();
				}
			});
		}
		if !ctrl.load(&path.to_string_lossy()) {
			ctrl.destroy();
			return;
		}
		// Volume only takes effect once a file is loaded, so apply it now rather than at creation.
		ctrl.set_volume(volume.get());
		if play_when_ready && pending_play.replace(false) {
			ctrl.play();
		}
		slots.insert(event, Slot { ctrl, path });
	}

	/// Play `event` using `file`, which may be a bare name or an absolute path.
	///
	/// Does nothing if the file cannot be found, so a missing or deleted custom sound degrades to
	/// silence rather than an error dialog on every notification.
	pub fn play(&self, event: SoundEvent, file: &str) {
		let Some(path) = Self::resolve(file) else {
			return;
		};
		let needs_build = {
			let slots = self.slots.borrow();
			slots.get(&event).is_none_or(|slot| slot.path != path)
		};
		if needs_build {
			// Builds and starts it; nothing further to do here.
			self.ensure_slot(event, path, true);
			return;
		}
		let slots = self.slots.borrow();
		if let Some(slot) = slots.get(&event) {
			slot.ctrl.set_volume(self.volume.get());
			slot.ctrl.stop();
			slot.ctrl.play();
		}
	}
}

#[cfg(test)]
mod tests {
	use super::SoundEvent;

	fn sounds_dir() -> std::path::PathBuf {
		std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("sounds")
	}

	fn missing_events(pack_dir: &std::path::Path) -> Vec<String> {
		SoundEvent::ALL.iter().map(|event| event.default_file()).filter(|file| !pack_dir.join(file).exists()).collect()
	}

	/// Every event falls back to this one file, so losing it would silence the feature outright for
	/// anyone who has not chosen sounds of their own.
	#[test]
	fn fallback_sound_is_shipped() {
		let path = sounds_dir().join(super::SoundPlayer::FALLBACK_FILE);
		assert!(path.exists(), "the fallback sound is missing from {}", path.display());
	}

	/// A pack that is present must be complete, so switching to it never silently borrows a sound
	/// from somewhere else.
	///
	/// Fedra itself ships none, and `sounds/packs` is ignored by git precisely so a contributor's
	/// own pack stays out of the repository. This therefore checks whatever is on the machine
	/// running it, which is the useful thing to check either way.
	#[test]
	fn any_pack_present_is_complete() {
		let Ok(entries) = std::fs::read_dir(sounds_dir().join("packs")) else { return };
		for entry in entries.flatten().filter(|entry| entry.path().is_dir()) {
			let missing = missing_events(&entry.path());
			assert!(missing.is_empty(), "pack {:?} is missing {missing:?}", entry.file_name());
		}
	}

	/// Config keys are persisted, so duplicates would silently merge two events into one setting.
	#[test]
	fn event_keys_are_unique() {
		let mut keys: Vec<_> = SoundEvent::ALL.iter().map(|event| event.key()).collect();
		let before = keys.len();
		keys.sort_unstable();
		keys.dedup();
		assert_eq!(keys.len(), before, "duplicate SoundEvent keys");
	}
}
