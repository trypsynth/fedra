//! Shared audio playback helpers built on rodio.
//!
//! Fedra used to hand media playback off to the Windows Media Player Legacy
//! component through wxWidgets' `MediaCtrl`. That component isn't installed on
//! every system and only speaks video-player idioms that don't matter to a
//! screen-reader-first client, so all playback (event sounds and attachment
//! playback) goes through rodio instead.

use std::{
	fs::File,
	io::{Cursor, Read, Seek},
	path::Path,
};

/// An open handle to the default audio output device.
///
/// Dropping this stops every sound still playing through it, so it must be
/// kept alive for as long as playback should continue.
pub struct AudioOutput {
	sink: rodio::MixerDeviceSink,
}

impl AudioOutput {
	/// Opens the system's default audio output device.
	pub fn open() -> Result<Self, rodio::DeviceSinkError> {
		let mut sink = rodio::DeviceSinkBuilder::open_default_sink()?;
		// Closing the media player window or quitting the app drops this
		// handle intentionally; that isn't a failure worth logging.
		sink.log_on_drop(false);
		Ok(Self { sink })
	}

	pub fn mixer(&self) -> &rodio::mixer::Mixer {
		self.sink.mixer()
	}
}

/// Decodes and plays the audio file at `path` once, at `volume` (0.0 is silent, 1.0 is the file's
/// own level), independently of any other playback already in progress on `output`. Silently does
/// nothing if the file can't be read or decoded, since this is used for best-effort event sounds.
pub fn play_once(output: &AudioOutput, path: &Path, volume: f32) {
	let Ok(file) = File::open(path) else { return };
	let Ok(source) = rodio::Decoder::try_from(file) else { return };
	play_source(output, source, volume);
}

/// Like [`play_once`], for a sound built into the executable.
pub fn play_bytes_once(output: &AudioOutput, bytes: &'static [u8], volume: f32) {
	let Ok(source) = rodio::Decoder::new(Cursor::new(bytes)) else { return };
	play_source(output, source, volume);
}

fn play_source<R: Read + Seek + Send + Sync + 'static>(output: &AudioOutput, source: rodio::Decoder<R>, volume: f32) {
	let player = rodio::Player::connect_new(output.mixer());
	player.set_volume(volume);
	player.append(source);
	player.detach();
}
