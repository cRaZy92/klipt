//! Serving an MP4 as picture-only without copying it: the handler type of each audio track
//! (`soun` in `moov/trak/mdia/hdlr`) is replaced by a type players don't know, so they skip those
//! tracks. Only those 4 bytes per track change, and only in what is served; the file on disk is
//! never touched.
//!
//! Why: the editor plays sound through Web Audio and keeps its `<video>` muted. On Linux a muted
//! `<video>` with audio still opens its own (muted) PipeWire stream, and PipeWire then remembers
//! Klipt as muted, so the next clip's sound started muted. A video without audio tracks opens none.

use std::io::{self, Read, Seek, SeekFrom};

/// What an audio track's handler type is served as: not a type players know, so they skip it.
pub const NOT_SOUND: [u8; 4] = *b"klpt";

/// Where, in the file, each audio track's 4-byte handler type is. Empty for anything that isn't
/// an MP4 with audio tracks.
pub fn audio_handler_offsets<R: Read + Seek>(r: &mut R) -> io::Result<Vec<u64>> {
    let end = r.seek(SeekFrom::End(0))?;
    let mut found = Vec::new();
    for moov in children(r, 0, end, b"moov")? {
        for trak in children(r, moov.body, moov.end, b"trak")? {
            for mdia in children(r, trak.body, trak.end, b"mdia")? {
                for hdlr in children(r, mdia.body, mdia.end, b"hdlr")? {
                    // hdlr: version and flags (4), pre_defined (4), then the handler type.
                    let at = hdlr.body + 8;
                    if at + 4 > hdlr.end {
                        continue;
                    }
                    let mut handler = [0u8; 4];
                    r.seek(SeekFrom::Start(at))?;
                    r.read_exact(&mut handler)?;
                    if &handler == b"soun" {
                        found.push(at);
                    }
                }
            }
        }
    }
    Ok(found)
}

/// Replaces the handler types at `offsets` in `buf`, which holds the file's bytes from `at` on.
pub fn apply(buf: &mut [u8], at: u64, offsets: &[u64]) {
    let buf_end = at + buf.len() as u64;
    for &offset in offsets {
        for (i, byte) in NOT_SOUND.iter().enumerate() {
            let pos = offset + i as u64;
            if pos >= at && pos < buf_end {
                buf[(pos - at) as usize] = *byte;
            }
        }
    }
}

/// A box: where its contents start and where it ends.
struct Mp4Box {
    body: u64,
    end: u64,
}

/// The boxes of type `kind` directly between `start` and `end`. Stops quietly at anything that
/// doesn't parse as a box (it isn't an MP4, or the rest is cut off).
fn children<R: Read + Seek>(r: &mut R, start: u64, end: u64, kind: &[u8; 4]) -> io::Result<Vec<Mp4Box>> {
    let mut found = Vec::new();
    let mut pos = start;
    while pos + 8 <= end {
        let mut head = [0u8; 8];
        r.seek(SeekFrom::Start(pos))?;
        r.read_exact(&mut head)?;
        let mut size = u32::from_be_bytes([head[0], head[1], head[2], head[3]]) as u64;
        let mut header = 8;
        if size == 1 {
            if pos + 16 > end {
                break;
            }
            let mut large = [0u8; 8];
            r.read_exact(&mut large)?;
            size = u64::from_be_bytes(large);
            header = 16;
        } else if size == 0 {
            size = end - pos; // to the end of the enclosing box (or file)
        }
        if size < header || pos + size > end {
            break;
        }
        if &head[4..8] == kind {
            found.push(Mp4Box { body: pos + header, end: pos + size });
        }
        pos += size;
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Cursor;

    #[test]
    fn finds_each_audio_track_handler() {
        // The fixture has a video track and two audio tracks (game, mic).
        let mut file = File::open(crate::ffmpeg::tests::fixture()).unwrap();
        let offsets = audio_handler_offsets(&mut file).unwrap();
        assert_eq!(offsets.len(), 2, "{offsets:?}");
        for at in offsets {
            let mut t = [0u8; 4];
            file.seek(SeekFrom::Start(at)).unwrap();
            file.read_exact(&mut t).unwrap();
            assert_eq!(&t, b"soun");
        }
    }

    #[test]
    fn anything_else_has_no_audio_handlers() {
        let mut junk = Cursor::new(b"not an mp4 at all, just some bytes".to_vec());
        assert_eq!(audio_handler_offsets(&mut junk).unwrap(), Vec::<u64>::new());
        let mut empty = Cursor::new(Vec::new());
        assert_eq!(audio_handler_offsets(&mut empty).unwrap(), Vec::<u64>::new());
    }

    #[test]
    fn apply_patches_only_the_bytes_inside_the_buffer() {
        // Bytes 10..20 of a file; one handler type at 8 (its last 2 bytes are inside), one at 18
        // (its first 2 bytes are inside), one far off.
        let mut buf = [b'.'; 10];
        apply(&mut buf, 10, &[8, 18, 500]);
        assert_eq!(&buf, b"pt......kl");
        let mut whole = [b'.'; 6];
        apply(&mut whole, 0, &[1]);
        assert_eq!(&whole, b".klpt.");
    }
}
