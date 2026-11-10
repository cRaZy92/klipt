//! The saved login: `<app data>/auth.json` with `{serverUrl, token, username}`.

use std::fs;
use std::io::Write;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::errors::Result;

const FILE_NAME: &str = "auth.json";
const TEMP_NAME: &str = "auth.json.tmp";

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Auth {
    pub server_url: String,
    pub token: String,
    pub username: String,
}

/// Never prints the token, so a stray `{:?}` can't leak it into logs.
impl std::fmt::Debug for Auth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Auth")
            .field("server_url", &self.server_url)
            .field("token", &"<redacted>")
            .field("username", &self.username)
            .finish()
    }
}

/// `None` when there is no saved login or the file is unreadable (the user just logs in again).
pub fn load(dir: &Path) -> Option<Auth> {
    serde_json::from_slice(&fs::read(dir.join(FILE_NAME)).ok()?).ok()
}

/// Atomic write; on Unix the file is created private (0600) so the token is never readable by
/// other users, not even briefly.
pub fn save(dir: &Path, auth: &Auth) -> Result<()> {
    fs::create_dir_all(dir)?;
    let temp = dir.join(TEMP_NAME);
    let _ = fs::remove_file(&temp);
    {
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        let mut file = options.open(&temp)?;
        file.write_all(&serde_json::to_vec_pretty(auth)?)?;
        file.sync_all()?;
    }
    fs::rename(&temp, dir.join(FILE_NAME))?;
    Ok(())
}

/// Forgets the saved login. A missing file is fine.
pub fn clear(dir: &Path) -> Result<()> {
    match fs::remove_file(dir.join(FILE_NAME)) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_hides_the_token() {
        let a = Auth { server_url: "https://k".into(), token: "secret-token".into(), username: "u".into() };
        let shown = format!("{a:?}");
        assert!(!shown.contains("secret-token") && shown.contains("<redacted>"), "{shown}");
    }
}
