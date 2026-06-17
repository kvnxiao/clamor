//! Custom audio file playback via `rodio`.
//!
//! Used only for the `{ file = "..." }` sound option. Opening the default
//! output device costs ~100-200ms, which is fine for a short chime.

use crate::Error;
use crate::Result;
use camino::Utf8Path;
use rodio::DeviceSinkBuilder;
use rodio::play;
use std::io::BufReader;

/// Plays an audio file to completion on the default output device, blocking
/// until playback finishes.
///
/// Supports WAV, OGG/Vorbis, MP3, and FLAC.
///
/// # Errors
///
/// Returns [`Error::Io`] if the file cannot be opened, [`Error::AudioDevice`]
/// if no output device is available, or [`Error::AudioPlay`] if the file
/// cannot be decoded or played.
pub(crate) fn play_file(path: &Utf8Path) -> Result<()> {
    let file = fs_err::File::open(path)?;
    let reader = BufReader::new(file);
    let handle = DeviceSinkBuilder::open_default_sink().map_err(Error::AudioDevice)?;
    let player = play(handle.mixer(), reader).map_err(|source| Error::AudioPlay {
        path: path.to_owned(),
        source,
    })?;
    player.sleep_until_end();
    Ok(())
}
