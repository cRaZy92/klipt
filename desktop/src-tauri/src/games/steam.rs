//! Steam library discovery: Valve's KeyValues text files (`libraryfolders.vdf`,
//! `appmanifest_*.acf`) and the Steam root location.

use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppManifest {
    pub appid: u32,
    pub name: String,
    pub installdir: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Node {
    Str(String),
    Obj(Vec<(String, Node)>),
}

impl Node {
    fn get(&self, key: &str) -> Option<&Node> {
        match self {
            Node::Obj(items) => items.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v),
            Node::Str(_) => None,
        }
    }

    fn str(&self, key: &str) -> Option<&str> {
        match self.get(key)? {
            Node::Str(s) => Some(s),
            Node::Obj(_) => None,
        }
    }
}

/// Tolerant KeyValues parser: quoted or bare tokens, `{}` nesting, `//` comments, `\\` `\"` `\n` `\t`
/// escapes. Malformed input yields whatever parsed up to the problem.
fn parse_kv(text: &str) -> Node {
    let chars: Vec<char> = text.chars().collect();
    let mut pos = 0;
    Node::Obj(parse_items(&chars, &mut pos, 0))
}

enum Tok {
    Str(String),
    Open,
    Close,
}

fn next_tok(c: &[char], pos: &mut usize) -> Option<Tok> {
    loop {
        match *c.get(*pos)? {
            ch if ch.is_whitespace() => *pos += 1,
            '/' if c.get(*pos + 1) == Some(&'/') => {
                while c.get(*pos).is_some_and(|&ch| ch != '\n') {
                    *pos += 1;
                }
            }
            '{' => {
                *pos += 1;
                return Some(Tok::Open);
            }
            '}' => {
                *pos += 1;
                return Some(Tok::Close);
            }
            '"' => {
                *pos += 1;
                let mut s = String::new();
                while let Some(&ch) = c.get(*pos) {
                    *pos += 1;
                    match ch {
                        '"' => break,
                        '\\' => {
                            let esc = c.get(*pos).copied();
                            *pos += 1;
                            match esc {
                                Some('n') => s.push('\n'),
                                Some('t') => s.push('\t'),
                                Some(other) => s.push(other),
                                None => break,
                            }
                        }
                        _ => s.push(ch),
                    }
                }
                return Some(Tok::Str(s));
            }
            _ => {
                let mut s = String::new();
                while let Some(&ch) = c.get(*pos) {
                    if ch.is_whitespace() || ch == '{' || ch == '}' || ch == '"' {
                        break;
                    }
                    s.push(ch);
                    *pos += 1;
                }
                return Some(Tok::Str(s));
            }
        }
    }
}

fn parse_items(c: &[char], pos: &mut usize, depth: usize) -> Vec<(String, Node)> {
    let mut items = Vec::new();
    while let Some(tok) = next_tok(c, pos) {
        let key = match tok {
            Tok::Str(k) => k,
            Tok::Close => break,
            Tok::Open => continue,
        };
        match next_tok(c, pos) {
            Some(Tok::Str(v)) => items.push((key, Node::Str(v))),
            Some(Tok::Open) if depth < 32 => items.push((key, Node::Obj(parse_items(c, pos, depth + 1)))),
            _ => break,
        }
    }
    items
}

/// The Steam install folder, if there is one.
#[cfg(target_os = "linux")]
pub fn steam_root() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    [home.join(".local/share/Steam"), home.join(".steam/steam")].into_iter().find(|p| p.exists())
}

#[cfg(windows)]
pub fn steam_root() -> Option<PathBuf> {
    super::proc_windows::steam_path_from_registry()
}

#[cfg(not(any(target_os = "linux", windows)))]
pub fn steam_root() -> Option<PathBuf> {
    None
}

/// All Steam library folders (each holds a `steamapps` dir), from `steamapps/libraryfolders.vdf`.
/// If the file is missing or lists nothing, the Steam root itself is returned.
pub fn libraries(steam_root: &Path) -> Vec<PathBuf> {
    let text = fs::read_to_string(steam_root.join("steamapps/libraryfolders.vdf")).unwrap_or_default();
    let mut libs = Vec::new();
    if let Some(Node::Obj(entries)) = parse_kv(&text).get("libraryfolders") {
        for (_, entry) in entries {
            if let Some(path) = entry.str("path") {
                libs.push(PathBuf::from(path));
            }
        }
    }
    if libs.is_empty() {
        libs.push(steam_root.to_path_buf());
    }
    libs
}

/// Installed games in one library, from its `steamapps/appmanifest_*.acf` files.
pub fn manifests(lib: &Path) -> Vec<AppManifest> {
    let Ok(dir) = fs::read_dir(lib.join("steamapps")) else { return Vec::new() };
    let mut out: Vec<AppManifest> = dir
        .flatten()
        .filter(|e| {
            let n = e.file_name();
            let n = n.to_string_lossy();
            n.starts_with("appmanifest_") && n.ends_with(".acf")
        })
        .filter_map(|e| parse_manifest(&fs::read_to_string(e.path()).ok()?))
        .collect();
    out.sort_by_key(|m| m.appid);
    out
}

fn parse_manifest(text: &str) -> Option<AppManifest> {
    let root = parse_kv(text);
    let state = root.get("AppState")?;
    Some(AppManifest {
        appid: state.str("appid")?.parse().ok()?,
        name: state.str("name")?.to_string(),
        installdir: state.str("installdir").unwrap_or_default().to_string(),
    })
}

/// Every installed game across every library.
pub fn all_manifests(steam_root: &Path) -> Vec<AppManifest> {
    libraries(steam_root).iter().flat_map(|l| manifests(l)).collect()
}

pub fn header_art_url(appid: u32) -> String {
    format!("https://cdn.cloudflare.steamstatic.com/steam/apps/{appid}/header.jpg")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixtures() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/steam")
    }

    /// A fake Steam root whose steamapps holds the fixture vdf.
    fn fake_root() -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("steamapps")).unwrap();
        fs::copy(fixtures().join("libraryfolders.vdf"), root.path().join("steamapps/libraryfolders.vdf")).unwrap();
        root
    }

    #[test]
    fn parses_the_three_real_libraries() {
        let root = fake_root();
        assert_eq!(
            libraries(root.path()),
            [
                PathBuf::from("/home/adam/.local/share/Steam"),
                PathBuf::from("/speedy/SteamLibrary"),
                PathBuf::from("/random-disk/SteamLibrary"),
            ]
        );
    }

    #[test]
    fn missing_vdf_falls_back_to_the_root() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(libraries(root.path()), [root.path().to_path_buf()]);
    }

    #[test]
    fn parses_the_active_matter_manifest() {
        let lib = tempfile::tempdir().unwrap();
        fs::create_dir_all(lib.path().join("steamapps")).unwrap();
        fs::copy(fixtures().join("appmanifest_2887580.acf"), lib.path().join("steamapps/appmanifest_2887580.acf"))
            .unwrap();
        fs::write(lib.path().join("steamapps/appmanifest_1.acf"), "garbage {{").unwrap();
        assert_eq!(
            manifests(lib.path()),
            [AppManifest { appid: 2887580, name: "Active Matter".into(), installdir: "ActiveMatter".into() }]
        );
    }

    #[test]
    fn kv_parser_handles_comments_escapes_and_nesting() {
        let n = parse_kv(
            "// top\n\"a\" { \"path\" \"C:\\\\Games\\\\Steam\" \"q\" \"say \\\"hi\\\"\" bare 5 \"sub\" { \"x\" \"1\" } }",
        );
        let a = n.get("a").unwrap();
        assert_eq!(a.str("path"), Some("C:\\Games\\Steam"));
        assert_eq!(a.str("q"), Some("say \"hi\""));
        assert_eq!(a.str("bare"), Some("5"));
        assert_eq!(a.get("sub").unwrap().str("x"), Some("1"));
    }

    #[test]
    fn kv_parser_survives_truncated_input() {
        let n = parse_kv("\"a\" { \"b\" \"c");
        assert_eq!(n.get("a").unwrap().str("b"), Some("c"));
    }

    #[test]
    fn header_art_url_shape() {
        assert_eq!(header_art_url(730), "https://cdn.cloudflare.steamstatic.com/steam/apps/730/header.jpg");
    }
}
