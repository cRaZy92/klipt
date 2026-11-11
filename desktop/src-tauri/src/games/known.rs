//! Well-known games that aren't reliably found through Steam, keyed by lowercase exe name.

use std::collections::HashMap;
use std::sync::OnceLock;

use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnownGame {
    pub name: String,
    pub steam_app_id: Option<u32>,
}

fn table() -> &'static HashMap<String, KnownGame> {
    static TABLE: OnceLock<HashMap<String, KnownGame>> = OnceLock::new();
    TABLE.get_or_init(|| {
        serde_json::from_str(include_str!("known_games.json")).expect("known_games.json is valid")
    })
}

/// Looks up an exe file name (not a path), ignoring case.
pub fn lookup(exe_name: &str) -> Option<&'static KnownGame> {
    table().get(&exe_name.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_is_case_insensitive() {
        let g = lookup("Valorant-Win64-Shipping.EXE").unwrap();
        assert_eq!(g.name, "Valorant");
        assert_eq!(g.steam_app_id, None);
        assert_eq!(lookup("CS2.exe").unwrap().steam_app_id, Some(730));
    }

    #[test]
    fn unknown_exe_is_none() {
        assert!(lookup("notepad.exe").is_none());
    }

    #[test]
    fn embedded_table_parses_with_every_listed_exe() {
        for exe in [
            "valorant-win64-shipping.exe", "overwatch.exe", "league of legends.exe", "wow.exe",
            "diablo iv.exe", "cod.exe", "hearthstone.exe", "r5apex.exe", "cs2.exe",
            "fortnitepc-win64-shipping.exe",
        ] {
            assert!(lookup(exe).is_some(), "{exe}");
        }
    }
}

/// Steam apps that run in the background next to a game (idle "games", wallpapers, overlays,
/// tools). Detection skips them so a clip isn't tagged with e.g. "Bongo Cat" instead of the game.
const IDLE_APP_IDS: &[u32] = &[
    3419430, // Bongo Cat
    2923300, // Banana
    431960,  // Wallpaper Engine
    993090,  // Lossless Scaling
    629520,  // Soundpad
    228980,  // Steamworks Common Redistributables (installers Steam runs before a game)
    1905180, // OBS Studio
    3301060, // Desktop Mate
];

pub fn is_idle_app(app_id: u32) -> bool {
    IDLE_APP_IDS.contains(&app_id)
}

#[cfg(test)]
mod idle_tests {
    #[test]
    fn idle_apps_are_recognised() {
        assert!(super::is_idle_app(3419430));
        assert!(!super::is_idle_app(2887580)); // Active Matter is a real game
    }
}
