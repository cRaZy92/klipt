//! The editor's filmstrip: one small JPEG per 100 ms of a clip, cached in `<cache>/edit/<id>-strip`
//! so the timeline can show frames and scrub without decoding the video.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub const FRAMES_PER_SECOND: u32 = 10;
pub const WIDTH: u32 = 640;

const COMPLETE_MARKER: &str = "complete";
const OPENED_MARKER: &str = "opened";
const STRIP_SUFFIX: &str = "-strip";

/// Where the filmstrip of clip `id` lives: `<cache>/edit/<id>-strip`.
pub fn strip_dir(cache_dir: &Path, id: &str) -> PathBuf {
    cache_dir.join("edit").join(format!("{id}{STRIP_SUFFIX}"))
}

/// The frame at `index` (0 = 0 ms, 1 = 100 ms, ...): `<dir>/<index padded to 6>.jpg`.
pub fn frame_path(dir: &Path, index: u32) -> PathBuf {
    dir.join(format!("{index:06}.jpg"))
}

/// How many frames a clip of `duration_ms` has.
pub fn expected_frames(duration_ms: u64) -> u32 {
    let step = 1000 / FRAMES_PER_SECOND as u64;
    duration_ms.div_ceil(step) as u32
}

/// Whether extraction finished, so every frame on disk is whole.
pub fn is_complete(dir: &Path) -> bool {
    dir.join(COMPLETE_MARKER).exists()
}

/// How many frames from 0 on are readable. Until extraction is complete the highest one is left
/// out, since it may be half-written.
pub fn ready_frames(dir: &Path) -> u32 {
    let mut count = 0;
    while frame_path(dir, count).exists() {
        count += 1;
    }
    if is_complete(dir) { count } else { count.saturating_sub(1) }
}

/// Notes that the strip was just opened (its `opened` file's mtime is the last open).
pub fn mark_opened(dir: &Path) {
    let marker = dir.join(OPENED_MARKER);
    let result = fs::create_dir_all(dir).and_then(|()| {
        fs::OpenOptions::new().write(true).create(true).truncate(false).open(&marker)?.set_modified(SystemTime::now())
    });
    if let Err(e) = result {
        eprintln!("klipt: can't mark {} opened: {e}", marker.display());
    }
}

/// One cached filmstrip, as far as eviction cares.
pub struct StripInfo {
    pub id: String,
    pub bytes: u64,
    pub opened: SystemTime,
}

/// The strips to delete, least recently opened first, so the rest fit in `cap_bytes`.
/// The strip `keep_id` is never chosen, even if it alone is over the cap.
pub fn choose_evictions(strips: &[StripInfo], cap_bytes: u64, keep_id: &str) -> Vec<String> {
    let mut total: u64 = strips.iter().map(|s| s.bytes).sum();
    let mut candidates: Vec<&StripInfo> = strips.iter().filter(|s| s.id != keep_id).collect();
    candidates.sort_by_key(|s| s.opened);
    let mut chosen = Vec::new();
    for strip in candidates {
        if total <= cap_bytes {
            break;
        }
        total -= strip.bytes;
        chosen.push(strip.id.clone());
    }
    chosen
}

fn list_strips(cache_dir: &Path) -> Vec<StripInfo> {
    let Ok(entries) = fs::read_dir(cache_dir.join("edit")) else { return Vec::new() };
    entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let id = name.strip_suffix(STRIP_SUFFIX)?.to_string();
            let dir = entry.path();
            if !dir.is_dir() {
                return None;
            }
            let bytes = fs::read_dir(&dir)
                .map(|files| files.flatten().filter_map(|f| f.metadata().ok()).filter(|m| m.is_file()).map(|m| m.len()).sum())
                .unwrap_or(0);
            let opened = fs::metadata(dir.join(OPENED_MARKER))
                .and_then(|m| m.modified())
                .or_else(|_| fs::metadata(&dir).and_then(|m| m.modified()))
                .unwrap_or(SystemTime::UNIX_EPOCH);
            Some(StripInfo { id, bytes, opened })
        })
        .collect()
}

/// Deletes the least recently opened strips until the cache's strips fit in `cap_bytes`
/// (never `keep_id`'s). Best effort.
pub fn evict(cache_dir: &Path, cap_bytes: u64, keep_id: &str) {
    for id in choose_evictions(&list_strips(cache_dir), cap_bytes, keep_id) {
        let dir = strip_dir(cache_dir, &id);
        if let Err(e) = fs::remove_dir_all(&dir) {
            eprintln!("klipt: can't delete {}: {e}", dir.display());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn info(id: &str, bytes: u64, secs: u64) -> StripInfo {
        StripInfo { id: id.into(), bytes, opened: SystemTime::UNIX_EPOCH + Duration::from_secs(secs) }
    }

    fn strip_with_frames(frames: &[u32]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for &i in frames {
            fs::write(frame_path(dir.path(), i), b"x").unwrap();
        }
        dir
    }

    #[test]
    fn expected_frames_rounds_up() {
        assert_eq!(expected_frames(0), 0);
        assert_eq!(expected_frames(1), 1);
        assert_eq!(expected_frames(100), 1);
        assert_eq!(expected_frames(101), 2);
        assert_eq!(expected_frames(67_000), 670);
    }

    #[test]
    fn ready_frames_skips_the_last_until_complete() {
        let dir = strip_with_frames(&[0, 1, 2, 3, 4]);
        assert_eq!(ready_frames(dir.path()), 4);
        fs::write(dir.path().join("complete"), b"").unwrap();
        assert!(is_complete(dir.path()));
        assert_eq!(ready_frames(dir.path()), 5);

        let gappy = strip_with_frames(&[0, 1, 3]);
        assert_eq!(ready_frames(gappy.path()), 1);
    }

    #[test]
    fn choose_evictions_drops_least_recently_opened_until_under_cap() {
        let strips = [info("a", 100, 1), info("b", 100, 3), info("c", 100, 2)];
        assert_eq!(choose_evictions(&strips, 150, ""), ["a", "c"]);
    }

    #[test]
    fn choose_evictions_never_drops_the_kept_strip() {
        let strips = [info("a", 300, 1), info("b", 100, 2)];
        assert_eq!(choose_evictions(&strips, 150, "a"), ["b"]);
    }

    #[test]
    fn choose_evictions_none_under_cap() {
        let strips = [info("a", 100, 1), info("b", 100, 2)];
        assert!(choose_evictions(&strips, 200, "").is_empty());
    }

    #[test]
    fn evict_deletes_the_oldest_opened_strip_dir() {
        let cache = tempfile::tempdir().unwrap();
        for (id, secs) in [("old", 1), ("new", 2)] {
            let dir = strip_dir(cache.path(), id);
            fs::create_dir_all(&dir).unwrap();
            fs::write(frame_path(&dir, 0), vec![0u8; 100]).unwrap();
            mark_opened(&dir);
            let opened = SystemTime::UNIX_EPOCH + Duration::from_secs(secs);
            fs::File::options().write(true).open(dir.join("opened")).unwrap().set_modified(opened).unwrap();
        }
        evict(cache.path(), 150, "");
        assert!(!strip_dir(cache.path(), "old").exists());
        assert!(strip_dir(cache.path(), "new").exists());
    }
}
