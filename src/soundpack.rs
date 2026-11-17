//! Soundpacks: a folder per pack under `soundpacks`, each holding one audio file per event.
//!
//! With no pack chosen, events are silent. With one chosen, an event plays `<event>.<ext>` from the
//! pack's folder, and is silent when the pack doesn't have that file.

use std::path::{Path, PathBuf};

use crate::{AppState, audio};

/// The extensions looked for, in order, so a pack can use any format rodio decodes.
const EXTENSIONS: [&str; 6] = ["mp3", "ogg", "oga", "wav", "flac", "m4a"];

/// How much one volume shortcut changes the volume, in percent.
pub const VOLUME_STEP: u8 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SoundEvent {
	Bookmark,
	Boost,
	Boundary,
	CloseTimeline,
	DirectMessage,
	Error,
	Favorite,
	Follow,
	FollowRequest,
	HomePost,
	Mentions,
	Mention,
	Media,
	Notification,
	OpenTimeline,
	Pin,
	PollEnded,
	PollVoted,
	PostDeleted,
	PostEdited,
	PostSent,
	Start,
	VolumeChange,
}

impl SoundEvent {
	#[cfg(test)]
	pub const ALL: [Self; 23] = [
		Self::Bookmark,
		Self::Boost,
		Self::Boundary,
		Self::CloseTimeline,
		Self::DirectMessage,
		Self::Error,
		Self::Favorite,
		Self::Follow,
		Self::FollowRequest,
		Self::HomePost,
		Self::Mentions,
		Self::Mention,
		Self::Media,
		Self::Notification,
		Self::OpenTimeline,
		Self::Pin,
		Self::PollEnded,
		Self::PollVoted,
		Self::PostDeleted,
		Self::PostEdited,
		Self::PostSent,
		Self::Start,
		Self::VolumeChange,
	];

	/// The file name, without its extension, that a pack uses for this event.
	pub const fn file_stem(self) -> &'static str {
		match self {
			Self::Bookmark => "bookmark",
			Self::Boost => "boost",
			Self::Boundary => "boundary",
			Self::CloseTimeline => "close_timeline",
			Self::DirectMessage => "direct_message",
			Self::Error => "error",
			Self::Favorite => "favorite",
			Self::Follow => "follow",
			Self::FollowRequest => "follow_request",
			Self::HomePost => "home_post",
			Self::Mentions => "mentions",
			Self::Mention => "mention",
			Self::Media => "media",
			Self::Notification => "notification",
			Self::OpenTimeline => "open_timeline",
			Self::Pin => "pin",
			Self::PollEnded => "poll_ended",
			Self::PollVoted => "poll_voted",
			Self::PostDeleted => "post_deleted",
			Self::PostEdited => "post_edited",
			Self::PostSent => "post_sent",
			Self::Start => "start",
			Self::VolumeChange => "volume_change",
		}
	}
}

/// Where the packs live: `soundpacks` in the config folder, which is `%APPDATA%\Fedra` when Fedra is
/// installed, so no pack needs write access to Program Files, and Fedra's own folder when it's portable.
pub fn packs_dir() -> PathBuf {
	crate::config::home().config_dir().join("soundpacks")
}

/// The names of the packs found in `dir`, sorted.
fn packs_in(dir: &Path) -> Vec<String> {
	let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
	let mut packs: Vec<String> = entries
		.filter_map(Result::ok)
		.filter(|entry| entry.path().is_dir())
		.filter_map(|entry| entry.file_name().into_string().ok())
		.collect();
	packs.sort_by_key(|name| name.to_lowercase());
	packs
}

/// The names of the installed packs, sorted.
pub fn list_packs() -> Vec<String> {
	packs_in(&packs_dir())
}

/// The folder of the pack called `name` in `dir`, if there is one. A name that would reach outside
/// `dir`, such as one edited into the config by hand, finds nothing.
fn pack_dir_in(dir: &Path, name: &str) -> Option<PathBuf> {
	let mut components = Path::new(name).components();
	let single_name = matches!(components.next(), Some(std::path::Component::Normal(_))) && components.next().is_none();
	let path = dir.join(name);
	(single_name && path.is_dir()).then_some(path)
}

/// The file in `pack_dir` that `event` plays, if the pack has one.
fn sound_in(pack_dir: &Path, event: SoundEvent) -> Option<PathBuf> {
	EXTENSIONS
		.iter()
		.map(|extension| pack_dir.join(format!("{}.{extension}", event.file_stem())))
		.find(|path| path.is_file())
}

fn volume_fraction(percent: u8) -> f32 {
	f32::from(percent.min(100)) / 100.0
}

/// Plays `event` from the chosen soundpack at the chosen volume. Does nothing when there's no pack,
/// when the pack's folder is gone, or when there's no audio device.
pub fn play(state: &AppState, event: SoundEvent) {
	play_at_volume(state, event, state.config.sound_volume);
}

/// Like [`play`], at `percent` volume rather than the chosen one.
pub fn play_at_volume(state: &AppState, event: SoundEvent, percent: u8) {
	let Some(pack) = state.config.soundpack.as_deref() else { return };
	let Some(output) = state.notification_sound.as_ref() else { return };
	let Some(pack_dir) = pack_dir_in(&packs_dir(), pack) else { return };
	let volume = volume_fraction(percent);
	if let Some(path) = sound_in(&pack_dir, event) {
		audio::play_once(output, &path, volume);
	}
}

#[cfg(test)]
mod tests {
	use std::fs;

	use super::*;

	/// An empty folder that is removed when it goes out of scope.
	struct TempDir(PathBuf);

	impl TempDir {
		fn new(name: &str) -> Self {
			let path = std::env::temp_dir().join(format!("fedra-soundpack-test-{}-{name}", std::process::id()));
			let _ = fs::remove_dir_all(&path);
			fs::create_dir_all(&path).unwrap();
			Self(path)
		}
	}

	impl Drop for TempDir {
		fn drop(&mut self) {
			let _ = fs::remove_dir_all(&self.0);
		}
	}

	#[test]
	fn every_event_has_a_distinct_file_name() {
		for (i, event) in SoundEvent::ALL.iter().enumerate() {
			assert!(!SoundEvent::ALL[i + 1..].iter().any(|other| other.file_stem() == event.file_stem()));
		}
	}

	#[test]
	fn file_names_match_the_documented_events() {
		let stems: Vec<&str> = SoundEvent::ALL.iter().map(|event| event.file_stem()).collect();
		assert_eq!(
			stems,
			[
				"bookmark",
				"boost",
				"boundary",
				"close_timeline",
				"direct_message",
				"error",
				"favorite",
				"follow",
				"follow_request",
				"home_post",
				"mentions",
				"mention",
				"media",
				"notification",
				"open_timeline",
				"pin",
				"poll_ended",
				"poll_voted",
				"post_deleted",
				"post_edited",
				"post_sent",
				"start",
				"volume_change",
			]
		);
	}

	#[test]
	fn packs_are_the_folders_sorted_ignoring_case() {
		let root = TempDir::new("list");
		for name in ["beta", "Alpha", "gamma"] {
			fs::create_dir(root.0.join(name)).unwrap();
		}
		fs::write(root.0.join("readme.txt"), "not a pack").unwrap();
		assert_eq!(packs_in(&root.0), ["Alpha", "beta", "gamma"]);
		assert_eq!(packs_in(&root.0.join("missing")), Vec::<String>::new());
	}

	#[test]
	fn a_pack_name_cannot_reach_outside_the_packs_folder() {
		let root = TempDir::new("names");
		fs::create_dir(root.0.join("good")).unwrap();
		assert!(pack_dir_in(&root.0, "good").is_some());
		assert!(pack_dir_in(&root.0, "missing").is_none());
		assert!(pack_dir_in(&root.0, "").is_none());
		assert!(pack_dir_in(&root.0, "..").is_none());
		assert!(pack_dir_in(&root.0, "good/..").is_none());
		assert!(pack_dir_in(&root.0, "good/sub").is_none());
	}

	#[test]
	fn an_event_finds_its_file_in_any_supported_format() {
		let pack = TempDir::new("files");
		fs::write(pack.0.join("boost.wav"), "").unwrap();
		fs::write(pack.0.join("open_timeline.ogg"), "").unwrap();
		fs::write(pack.0.join("open_timeline.mp3"), "").unwrap();
		assert_eq!(sound_in(&pack.0, SoundEvent::Boost), Some(pack.0.join("boost.wav")));
		// mp3 comes first when a pack has the same sound in two formats.
		assert_eq!(sound_in(&pack.0, SoundEvent::OpenTimeline), Some(pack.0.join("open_timeline.mp3")));
		assert_eq!(sound_in(&pack.0, SoundEvent::Error), None);
	}

	#[test]
	fn volume_is_a_fraction_that_stops_at_full() {
		assert!((volume_fraction(0) - 0.0).abs() < f32::EPSILON);
		assert!((volume_fraction(50) - 0.5).abs() < f32::EPSILON);
		assert!((volume_fraction(100) - 1.0).abs() < f32::EPSILON);
		assert!((volume_fraction(200) - 1.0).abs() < f32::EPSILON);
	}
}
