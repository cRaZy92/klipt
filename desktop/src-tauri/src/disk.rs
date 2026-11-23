// Disk space and the size of the running full recording, checked while it records so a full
// disk doesn't cost the whole file.

use std::path::Path;
use std::time::{Duration, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiskReading {
    /// The running recording's file so far (None: not found yet).
    pub recording_bytes: Option<u64>,
    /// Free space on the clips folder's drive (None: unknown, e.g. a network share).
    pub free_bytes: Option<u64>,
}

/// The newest file in `<clips_dir>/Recordings` changed at or after `since_ms` (Unix ms), and the
/// free space of the drive that holds `clips_dir`.
pub fn reading(clips_dir: &Path, since_ms: i64) -> DiskReading {
    DiskReading { recording_bytes: recording_size(&clips_dir.join("Recordings"), since_ms), free_bytes: free_space(clips_dir) }
}

fn recording_size(dir: &Path, since_ms: i64) -> Option<u64> {
    // A second of slack: some file systems keep coarse times.
    let since = UNIX_EPOCH + Duration::from_millis(since_ms.max(0) as u64).saturating_sub(Duration::from_secs(1));
    std::fs::read_dir(dir)
        .ok()?
        .filter_map(|entry| entry.ok()?.metadata().ok())
        .filter(|meta| meta.is_file())
        .filter_map(|meta| Some((meta.modified().ok()?, meta.len())))
        .filter(|(modified, _)| *modified >= since)
        .max_by_key(|(modified, _)| *modified)
        .map(|(_, len)| len)
}

fn free_space(dir: &Path) -> Option<u64> {
    let dir = dir.canonicalize().ok()?;
    let disks = sysinfo::Disks::new_with_refreshed_list().ok()?;
    disks
        .iter()
        .filter(|d| dir.starts_with(d.mount_point()))
        .max_by_key(|d| d.mount_point().as_os_str().len())
        .map(|d| d.available_space())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newest_recording_since_the_start_is_measured() {
        let dir = tempfile::tempdir().unwrap();
        let recordings = dir.path().join("Recordings");
        std::fs::create_dir(&recordings).unwrap();
        let old = recordings.join("old.mp4");
        std::fs::write(&old, vec![0u8; 10]).unwrap();
        let file = std::fs::File::options().write(true).open(&old).unwrap();
        file.set_modified(UNIX_EPOCH + Duration::from_secs(1_000)).unwrap();
        std::fs::write(recordings.join("new.mp4"), vec![0u8; 42]).unwrap();
        let since = std::time::SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as i64 - 5_000;
        assert_eq!(reading(dir.path(), since).recording_bytes, Some(42));
    }

    #[test]
    fn no_recording_yet_is_none() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(reading(dir.path(), 0).recording_bytes, None);
    }
}
