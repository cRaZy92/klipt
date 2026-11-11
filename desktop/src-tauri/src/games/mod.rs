//! Game detection: which game is running, for tagging clips and for "record only while a game
//! runs". Order: Steam (env on Linux, install path on Windows), known exes, manual mappings.

pub mod known;
pub mod mappings;
#[cfg(target_os = "linux")]
pub mod proc_linux;
#[cfg(windows)]
pub mod proc_windows;
pub mod steam;

use std::path::Path;

use serde::{Deserialize, Serialize};

use steam::AppManifest;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Detected {
    pub game_name: Option<String>,
    pub steam_app_id: Option<u32>,
    pub exe: Option<String>,
}

/// The game running right now (the foreground one on Windows), or an all-None `Detected`.
/// `data_dir` is the app data dir that holds `mappings.json`.
pub fn detect_now(data_dir: &Path) -> Detected {
    let manifests = steam::steam_root().map(|r| steam::all_manifests(&r)).unwrap_or_default();
    detect_platform(data_dir, &manifests).unwrap_or_default()
}

/// True if any game is running. Backs the "record only while a game runs" mode.
pub fn is_game_running(data_dir: &Path) -> bool {
    let manifests = steam::steam_root().map(|r| steam::all_manifests(&r)).unwrap_or_default();
    running_platform(data_dir, &manifests)
}

#[cfg(target_os = "linux")]
fn detect_platform(data_dir: &Path, manifests: &[AppManifest]) -> Option<Detected> {
    let p = proc_linux::find_steam_game()?;
    let name = manifests
        .iter()
        .find(|m| m.appid == p.app_id)
        .map(|m| m.name.clone())
        .or_else(|| p.exe.as_deref().and_then(|e| known::lookup(e).map(|k| k.name.clone())))
        .or_else(|| p.exe.as_deref().and_then(|e| mappings::get(data_dir, e)));
    Some(Detected { game_name: name, steam_app_id: Some(p.app_id), exe: p.exe })
}

#[cfg(target_os = "linux")]
fn running_platform(_data_dir: &Path, _manifests: &[AppManifest]) -> bool {
    proc_linux::find_steam_game().is_some()
}

#[cfg(windows)]
fn detect_platform(data_dir: &Path, manifests: &[AppManifest]) -> Option<Detected> {
    identify_exe_path(&proc_windows::foreground_exe_path()?.to_string_lossy(), manifests, data_dir)
}

#[cfg(windows)]
fn running_platform(data_dir: &Path, manifests: &[AppManifest]) -> bool {
    proc_windows::running_exe_paths()
        .iter()
        .any(|p| identify_exe_path(&p.to_string_lossy(), manifests, data_dir).is_some())
}

#[cfg(not(any(target_os = "linux", windows)))]
fn detect_platform(_: &Path, _: &[AppManifest]) -> Option<Detected> {
    None
}

#[cfg(not(any(target_os = "linux", windows)))]
fn running_platform(_: &Path, _: &[AppManifest]) -> bool {
    false
}

/// Matches an executable path against Steam installs, then known games, then manual mappings.
/// Accepts `\` or `/` separators. None means "not a game we recognize".
pub fn identify_exe_path(path: &str, manifests: &[AppManifest], data_dir: &Path) -> Option<Detected> {
    let parts: Vec<&str> = path.split(['\\', '/']).filter(|s| !s.is_empty()).collect();
    let exe = parts.last()?.to_string();

    let install_dir = parts.windows(3).find_map(|w| {
        (w[0].eq_ignore_ascii_case("steamapps") && w[1].eq_ignore_ascii_case("common")).then_some(w[2])
    });
    // A bare "steamapps\common\<exe>" has no game folder: the third part must not be the exe itself.
    if let Some(dir) = install_dir.filter(|d| *d != exe) {
        if let Some(m) = manifests.iter().find(|m| m.installdir.eq_ignore_ascii_case(dir)) {
            if known::is_idle_app(m.appid) {
                return None;
            }
            return Some(Detected { game_name: Some(m.name.clone()), steam_app_id: Some(m.appid), exe: Some(exe) });
        }
    }
    if let Some(k) = known::lookup(&exe) {
        return Some(Detected { game_name: Some(k.name.clone()), steam_app_id: k.steam_app_id, exe: Some(exe) });
    }
    mappings::get(data_dir, &exe).map(|name| Detected { game_name: Some(name), steam_app_id: None, exe: Some(exe) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_steam_apps_are_not_games() {
        let manifests = vec![AppManifest { appid: 3419430, name: "Bongo Cat".into(), installdir: "BongoCat".into() }];
        let dir = tempfile::tempdir().unwrap();
        let path = r"C:\Steam\steamapps\common\BongoCat\BongoCat.exe";
        assert_eq!(identify_exe_path(path, &manifests, dir.path()), None);
    }

    #[test]
    fn background_steam_tools_dont_count_as_a_running_game() {
        // Wallpaper Engine runs all day from steamapps\common: it mustn't keep game mode recording.
        let manifests = vec![
            AppManifest { appid: 431960, name: "Wallpaper Engine".into(), installdir: "wallpaper_engine".into() },
            AppManifest {
                appid: 228980,
                name: "Steamworks Common Redistributables".into(),
                installdir: "Steamworks Shared".into(),
            },
        ];
        let dir = tempfile::tempdir().unwrap();
        for path in [
            r"C:\Steam\steamapps\common\wallpaper_engine\wallpaper64.exe",
            r"C:\Steam\steamapps\common\wallpaper_engine\bin\webwallpaper32.exe",
            r"C:\Steam\steamapps\common\Steamworks Shared\_CommonRedist\vcredist\2022\VC_redist.x64.exe",
        ] {
            assert_eq!(identify_exe_path(path, &manifests, dir.path()), None, "{path}");
        }
    }

    fn active_matter() -> Vec<AppManifest> {
        vec![AppManifest { appid: 2887580, name: "Active Matter".into(), installdir: "ActiveMatter".into() }]
    }

    #[test]
    fn steam_install_path_resolves_through_manifests() {
        let dir = tempfile::tempdir().unwrap();
        let d = identify_exe_path(
            r"D:\SteamLibrary\steamapps\common\activematter\Game\AM.exe",
            &active_matter(),
            dir.path(),
        )
        .unwrap();
        assert_eq!(d.game_name.as_deref(), Some("Active Matter"));
        assert_eq!(d.steam_app_id, Some(2887580));
        assert_eq!(d.exe.as_deref(), Some("AM.exe"));
    }

    #[test]
    fn known_then_mapped_then_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let known = identify_exe_path(r"C:\Riot\VALORANT-Win64-Shipping.exe", &[], dir.path()).unwrap();
        assert_eq!(known.game_name.as_deref(), Some("Valorant"));
        assert!(identify_exe_path(r"C:\x\mine.exe", &[], dir.path()).is_none());
        mappings::set(dir.path(), "Mine.exe", "My Game").unwrap();
        let mapped = identify_exe_path(r"C:\x\mine.exe", &[], dir.path()).unwrap();
        assert_eq!(mapped.game_name.as_deref(), Some("My Game"));
        assert_eq!(mapped.steam_app_id, None);
    }

    #[test]
    fn unmatched_steam_dir_falls_through_to_known() {
        let dir = tempfile::tempdir().unwrap();
        let d = identify_exe_path("/lib/steamapps/common/Counter-Strike Global Offensive/cs2.exe", &[], dir.path())
            .unwrap();
        assert_eq!(d.steam_app_id, Some(730));
    }

    #[test]
    fn detected_serializes_camel_case() {
        let json = serde_json::to_value(Detected { game_name: Some("A".into()), steam_app_id: Some(1), exe: None })
            .unwrap();
        assert_eq!(json["gameName"], "A");
        assert_eq!(json["steamAppId"], 1);
    }

    #[test]
    fn detect_now_never_panics() {
        let dir = tempfile::tempdir().unwrap();
        let _ = detect_now(dir.path());
        let _ = is_game_running(dir.path());
    }
}
