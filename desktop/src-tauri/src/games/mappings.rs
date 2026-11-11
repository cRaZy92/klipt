//! Manual exe -> game name mappings, stored as `<app data>/mappings.json` (lowercase exe keys).

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::Path;

use crate::errors::Result;

pub const FILE_NAME: &str = "mappings.json";
const TEMP_NAME: &str = "mappings.json.tmp";
/// Where an unreadable `mappings.json` is moved before a new mapping is written, so the
/// mappings in it can still be recovered by hand.
pub const CORRUPT_NAME: &str = "mappings.json.corrupt";

/// Missing or unreadable files yield an empty map: mappings are a convenience, never a blocker.
pub fn load(dir: &Path) -> HashMap<String, String> {
    fs::read(dir.join(FILE_NAME)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

pub fn get(dir: &Path, exe_name: &str) -> Option<String> {
    load(dir).get(&exe_name.to_lowercase()).cloned()
}

/// Remembers that `exe_name` is `game_name`. Writes atomically (temp file, then rename).
pub fn set(dir: &Path, exe_name: &str, game_name: &str) -> Result<()> {
    let path = dir.join(FILE_NAME);
    let mut map = match fs::read(&path) {
        Ok(bytes) => match serde_json::from_slice(&bytes) {
            Ok(map) => map,
            Err(e) => {
                // Writing now would replace every mapping in it with just this one.
                eprintln!("klipt: {FILE_NAME} is unreadable ({e}); moving it to {CORRUPT_NAME}");
                fs::rename(&path, dir.join(CORRUPT_NAME))?;
                HashMap::new()
            }
        },
        Err(_) => HashMap::new(),
    };
    map.insert(exe_name.to_lowercase(), game_name.to_string());
    let temp = dir.join(TEMP_NAME);
    let mut f = fs::File::create(&temp)?;
    f.write_all(&serde_json::to_vec_pretty(&map)?)?;
    f.sync_all()?;
    fs::rename(&temp, dir.join(FILE_NAME))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_case_insensitively_and_overwrites() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(get(dir.path(), "MyGame.exe"), None);
        set(dir.path(), "MyGame.exe", "My Game").unwrap();
        assert_eq!(get(dir.path(), "mygame.EXE").as_deref(), Some("My Game"));
        set(dir.path(), "mygame.exe", "Renamed").unwrap();
        assert_eq!(get(dir.path(), "MyGame.exe").as_deref(), Some("Renamed"));
        assert!(!dir.path().join(TEMP_NAME).exists());
    }

    #[test]
    fn corrupt_file_reads_as_empty() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(FILE_NAME), "{not json").unwrap();
        assert!(load(dir.path()).is_empty());
    }

    #[test]
    fn writing_after_a_corrupt_file_keeps_the_old_file_aside() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(FILE_NAME), "{\"a.exe\": \"A\", oops").unwrap();
        set(dir.path(), "b.exe", "B").unwrap();
        assert_eq!(get(dir.path(), "b.exe").as_deref(), Some("B"));
        let kept = fs::read_to_string(dir.path().join(CORRUPT_NAME)).unwrap();
        assert!(kept.contains("a.exe"), "{kept}");
    }
}
