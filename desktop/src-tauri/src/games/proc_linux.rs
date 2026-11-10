//! Linux detection: Steam sets `SteamAppId`/`SteamGameId` in the environment of every game it
//! launches, so a `/proc/*/environ` scan finds the running game.

use std::collections::HashMap;
use std::fs;

/// The Steam app id in a raw `environ` blob (NUL-separated `KEY=VALUE`). `SteamAppId` wins over
/// `SteamGameId`; zero and unparsable values count as absent.
pub fn parse_environ(bytes: &[u8]) -> Option<u32> {
    let find = |key: &str| {
        bytes.split(|&b| b == 0).find_map(|var| {
            let var = std::str::from_utf8(var).ok()?;
            let id: u32 = var.strip_prefix(key)?.strip_prefix('=')?.parse().ok()?;
            (id != 0).then_some(id)
        })
    };
    find("SteamAppId").or_else(|| find("SteamGameId"))
}

fn ppid_of(pid: u32) -> Option<u32> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // The comm field may contain spaces and parens; the fields after the last ')' are stable.
    let rest = &stat[stat.rfind(')')? + 1..];
    rest.split_whitespace().nth(1)?.parse().ok()
}

/// True for our own pid and anything we spawned (for example the recorder).
fn in_our_tree(pid: u32, ours: u32, parents: &mut HashMap<u32, Option<u32>>) -> bool {
    let mut cur = pid;
    for _ in 0..64 {
        if cur == ours {
            return true;
        }
        if cur <= 1 {
            return false;
        }
        let parent = *parents.entry(cur).or_insert_with(|| ppid_of(cur));
        match parent {
            Some(p) => cur = p,
            None => return false,
        }
    }
    false
}

/// A running Steam game: its app id and the executable's file name, when readable.
pub struct SteamProcess {
    pub app_id: u32,
    pub exe: Option<String>,
}

pub fn find_steam_game() -> Option<SteamProcess> {
    let ours = std::process::id();
    let mut parents = HashMap::new();
    for entry in fs::read_dir("/proc").ok()?.flatten() {
        let Some(pid) = entry.file_name().to_str().and_then(|n| n.parse::<u32>().ok()) else { continue };
        if in_our_tree(pid, ours, &mut parents) {
            continue;
        }
        // Other users' processes are unreadable; skip them quietly.
        let Ok(environ) = fs::read(format!("/proc/{pid}/environ")) else { continue };
        if let Some(app_id) = parse_environ(&environ) {
            let exe = fs::read_link(format!("/proc/{pid}/exe"))
                .ok()
                .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()));
            return Some(SteamProcess { app_id, exe });
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_steam_app_id() {
        assert_eq!(parse_environ(b"HOME=/home/a\0SteamAppId=2887580\0PATH=/bin\0"), Some(2887580));
    }

    #[test]
    fn zero_is_none() {
        assert_eq!(parse_environ(b"SteamAppId=0\0"), None);
    }

    #[test]
    fn falls_back_to_steam_game_id_and_prefers_app_id() {
        assert_eq!(parse_environ(b"SteamGameId=730\0"), Some(730));
        assert_eq!(parse_environ(b"SteamGameId=730\0SteamAppId=5\0"), Some(5));
        assert_eq!(parse_environ(b"SteamAppId=0\0SteamGameId=730\0"), Some(730));
    }

    #[test]
    fn ignores_lookalikes_and_garbage() {
        assert_eq!(parse_environ(b"XSteamAppId=5\0SteamAppIdent=6\0SteamAppId=abc\0"), None);
        assert_eq!(parse_environ(b""), None);
    }

    #[test]
    fn our_own_process_and_children_are_in_our_tree() {
        let ours = std::process::id();
        let mut cache = HashMap::new();
        assert!(in_our_tree(ours, ours, &mut cache));
        let mut child = std::process::Command::new("sleep").arg("5").spawn().unwrap();
        let in_tree = in_our_tree(child.id(), ours, &mut cache);
        let _ = child.kill();
        let _ = child.wait();
        assert!(in_tree);
        assert!(!in_our_tree(1, ours, &mut cache));
    }
}
