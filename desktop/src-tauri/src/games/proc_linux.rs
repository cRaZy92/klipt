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

/// Programs that run a game rather than being it: Steam's launch helpers, the Steam Runtime
/// container, Proton/Wine, shells and wrappers from launch options. Every game of their kind runs
/// under the same ones, so a clip tagged or a mapping keyed with one would apply to all of them.
const WRAPPERS: &[&str] = &[
    "reaper", "steam-launch-wrapper", "steam-runtime-launch-client", "steam-runtime-launcher-service",
    "pressure-vessel-wrap", "pressure-vessel-adverb", "pv-bwrap", "srt-bwrap", "bwrap", "proton",
    "wine", "wine64", "wine-preloader", "wine64-preloader", "wineserver", "gamemoderun", "gamescope",
    "mangohud", "sh", "bash", "dash", "python", "python3", "steam", "steam.exe",
];

/// The game's executable name for one process, or None for a wrapper. Proton games run as
/// `wine64-preloader` with the Windows path (`Z:\...\Game.exe`) as the first argument, so that
/// path wins when it names an `.exe`; Wine's own Windows programs (`C:\windows\...`) are wrappers.
pub fn game_exe(exe_link: Option<&str>, argv0: Option<&str>) -> Option<String> {
    if let Some(arg) = argv0.filter(|a| a.to_ascii_lowercase().ends_with(".exe")) {
        if arg.to_ascii_lowercase().replace('/', "\\").contains("\\windows\\") {
            return None;
        }
        let name = arg.rsplit(['\\', '/']).next()?;
        return (!name.is_empty() && !is_wrapper(name)).then(|| name.to_string());
    }
    let name = exe_link?.rsplit('/').next()?;
    (!name.is_empty() && !is_wrapper(name)).then(|| name.to_string())
}

fn is_wrapper(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    // python3.12, pressure-vessel-launch, ... count as their family.
    WRAPPERS.iter().any(|w| name == *w || name.strip_prefix(w).is_some_and(|rest| rest.starts_with(['.', '-'])))
}

/// Resident memory in pages, for picking the game among its helpers (the game is the big one).
fn resident_pages(pid: u32) -> u64 {
    fs::read_to_string(format!("/proc/{pid}/statm"))
        .ok()
        .and_then(|s| s.split_whitespace().nth(1)?.parse().ok())
        .unwrap_or(0)
}

pub fn find_steam_game() -> Option<SteamProcess> {
    let ours = std::process::id();
    let mut parents = HashMap::new();
    // The first app id found, and the biggest non-wrapper process of it so far.
    let mut found: Option<(u32, Option<(u64, String)>)> = None;
    for entry in fs::read_dir("/proc").ok()?.flatten() {
        let Some(pid) = entry.file_name().to_str().and_then(|n| n.parse::<u32>().ok()) else { continue };
        if in_our_tree(pid, ours, &mut parents) {
            continue;
        }
        // Other users' processes are unreadable; skip them quietly.
        let Ok(environ) = fs::read(format!("/proc/{pid}/environ")) else { continue };
        // Idle/overlay apps (Bongo Cat, Wallpaper Engine…) also carry a SteamAppId: keep looking.
        let Some(app_id) = parse_environ(&environ).filter(|id| !super::known::is_idle_app(*id)) else { continue };
        let (game_id, best) = found.get_or_insert((app_id, None));
        if *game_id != app_id {
            continue;
        }
        let exe_link = fs::read_link(format!("/proc/{pid}/exe")).ok();
        let cmdline = fs::read(format!("/proc/{pid}/cmdline")).unwrap_or_default();
        let argv0 = cmdline.split(|&b| b == 0).next().map(String::from_utf8_lossy);
        let Some(exe) = game_exe(exe_link.as_deref().and_then(|p| p.to_str()), argv0.as_deref()) else { continue };
        let pages = resident_pages(pid);
        if best.as_ref().is_none_or(|(most, _)| pages > *most) {
            *best = Some((pages, exe));
        }
    }
    let (app_id, best) = found?;
    Some(SteamProcess { app_id, exe: best.map(|(_, exe)| exe) })
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
    fn steam_and_proton_wrappers_are_not_the_game() {
        let home = "/home/a/.local/share/Steam";
        for (link, argv0) in [
            (format!("{home}/steamrt64/reaper"), format!("{home}/steamrt64/reaper")),
            ("/usr/bin/steam-runtime-launch-client".into(), "steam-runtime-launch-client".into()),
            ("/usr/lib/pressure-vessel/from-host/bin/pressure-vessel-adverb".into(), "pressure-vessel-adverb".into()),
            ("/usr/bin/python3.12".into(), "python3".into()),
            ("/proton/files/bin/wineserver".into(), "wineserver".into()),
            ("/proton/files/bin/wine64-preloader".into(), r"C:\windows\system32\services.exe".into()),
            ("/proton/files/bin/wine64-preloader".into(), r"c:\windows\system32\steam.exe".into()),
        ] {
            assert_eq!(game_exe(Some(&link), Some(&argv0)), None, "{link} {argv0}");
        }
    }

    #[test]
    fn the_game_exe_is_found_behind_wrappers() {
        assert_eq!(
            game_exe(Some("/proton/files/bin/wine64-preloader"), Some(r"Z:\games\ActiveMatter\Game\AM.exe")),
            Some("AM.exe".into()),
            "Proton: the Windows exe in argv[0]"
        );
        assert_eq!(
            game_exe(Some("/lib/steamapps/common/BongoCat/BongoCat.x86_64"), Some("/lib/steamapps/common/BongoCat/BongoCat.x86_64")),
            Some("BongoCat.x86_64".into()),
            "native game"
        );
        assert_eq!(game_exe(Some("/x/cs2"), None), Some("cs2".into()));
        assert_eq!(game_exe(None, None), None);
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
