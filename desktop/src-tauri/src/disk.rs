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
        // `fs::metadata` opens the file: on Windows a directory listing's size of a file being
        // written stays at what it was when the writer opened it.
        .filter_map(|entry| std::fs::metadata(entry.ok()?.path()).ok())
        .filter(|meta| meta.is_file())
        .filter_map(|meta| Some((meta.modified().ok()?, meta.len())))
        .filter(|(modified, _)| *modified >= since)
        .max_by_key(|(modified, _)| *modified)
        .map(|(_, len)| len)
}

/// Free space for an unprivileged user on the file system holding `dir`, asked of that folder
/// itself, so network shares and odd mounts give their own answer.
#[cfg(target_os = "linux")]
fn free_space(dir: &Path) -> Option<u64> {
    let stat = nix::sys::statvfs::statvfs(dir).ok()?;
    Some((stat.blocks_available() as u64).saturating_mul(stat.fragment_size() as u64))
}

/// Free space on the drive holding `dir`. `dunce` keeps the path a plain `C:\…` one: a
/// `canonicalize`d `\\?\C:\…` path never starts with sysinfo's `C:\` mount points.
#[cfg(not(target_os = "linux"))]
fn free_space(dir: &Path) -> Option<u64> {
    let dir = dunce::canonicalize(dir).ok()?;
    let disks = sysinfo::Disks::new_with_refreshed_list().ok()?;
    let mounts: Vec<(&Path, u64)> = disks.iter().map(|d| (d.mount_point(), d.available_space())).collect();
    free_on_mount(&dir, &mounts)
}

/// The free space of the longest mount point that `dir` is inside, if any.
#[cfg(any(not(target_os = "linux"), test))]
fn free_on_mount(dir: &Path, mounts: &[(&Path, u64)]) -> Option<u64> {
    mounts.iter().filter(|(mount, _)| dir.starts_with(mount)).max_by_key(|(mount, _)| mount.as_os_str().len()).map(|(_, free)| *free)
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
    fn the_longest_matching_mount_point_holds_the_folder() {
        let mounts = [(Path::new("/"), 10), (Path::new("/home"), 20), (Path::new("/home/a/clips-other"), 30)];
        assert_eq!(free_on_mount(Path::new("/home/a/clips"), &mounts), Some(20));
        assert_eq!(free_on_mount(Path::new("/srv"), &mounts), Some(10));
        assert_eq!(free_on_mount(Path::new("/srv"), &mounts[1..]), None);
    }

    #[test]
    fn free_space_of_a_real_folder_is_known() {
        let dir = tempfile::tempdir().unwrap();
        assert!(reading(dir.path(), 0).free_bytes.is_some_and(|b| b > 0));
    }

    #[test]
    fn no_recording_yet_is_none() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(reading(dir.path(), 0).recording_bytes, None);
    }
}
