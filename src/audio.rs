//! Shared audio playback helpers built on rodio.
//!
//! Fedra used to hand media playback off to the Windows Media Player Legacy
//! component through wxWidgets' `MediaCtrl`. That component isn't installed on
//! every system and only speaks video-player idioms that don't matter to a
//! screen-reader-first client, so all playback (the favorite/notification
//! sound and attachment playback) goes through rodio instead.

use std::{fs::File, path::Path};

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

/// Decodes and plays the audio file at `path` once, independently of any
/// other playback already in progress on `output`. Silently does nothing if
/// the file can't be read or decoded, since this is used for the best-effort
/// favorite/notification sound.
pub fn play_once(output: &AudioOutput, path: &Path) {
    let Ok(file) = File::open(path) else { return };
    let Ok(source) = rodio::Decoder::try_from(file) else { return };
    let player = rodio::Player::connect_new(output.mixer());
    player.append(source);
    player.detach();
}
