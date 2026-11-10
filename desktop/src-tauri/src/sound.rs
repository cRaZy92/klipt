//! The "clip saved" chime.

use std::io::Cursor;
use std::time::Duration;

/// A ~220 ms two-tone chime, 44.1 kHz 16-bit mono.
pub const SAVED_WAV: &[u8] = include_bytes!("../assets/saved.wav");

/// Plays the chime on a short-lived thread. A missing or busy audio device is only logged.
pub fn play_saved() {
    let spawned = std::thread::Builder::new().name("klipt-sound".into()).spawn(|| {
        if let Err(e) = play_blocking(SAVED_WAV) {
            eprintln!("klipt: can't play the save sound: {e}");
        }
    });
    if let Err(e) = spawned {
        eprintln!("klipt: can't start the sound thread: {e}");
    }
}

fn play_blocking(wav: &'static [u8]) -> Result<(), String> {
    let mut device = rodio::DeviceSinkBuilder::open_default_sink().map_err(|e| e.to_string())?;
    // Closing the device after a sound is expected; don't log it.
    device.log_on_drop(false);
    let player = rodio::play(device.mixer(), Cursor::new(wav)).map_err(|e| e.to_string())?;
    player.sleep_until_end();
    // Let the device drain its buffer so the tail of the chime isn't cut off.
    std::thread::sleep(Duration::from_millis(100));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_embedded_chime_is_a_short_wav_that_decodes() {
        assert_eq!(&SAVED_WAV[..4], b"RIFF");
        assert_eq!(&SAVED_WAV[8..12], b"WAVE");
        let decoder = rodio::Decoder::new(Cursor::new(SAVED_WAV)).unwrap();
        let duration = rodio::Source::total_duration(&decoder).expect("a wav knows its length");
        assert!((Duration::from_millis(150)..=Duration::from_millis(300)).contains(&duration), "{duration:?}");
    }
}
