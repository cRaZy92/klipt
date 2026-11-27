//! Files dropped from a network share. On Linux, a file manager like Dolphin hands over a share's
//! files as addresses (`smb://server/share/clip.mp4`), not paths, and Tauri passes them on as they
//! are. KDE's kio-fuse can make any such address a local path (it is how KDE apps open network
//! files), so Klipt asks it over D-Bus. Without kio-fuse (GNOME, or not installed) the user is told
//! to mount the share first.

use std::path::PathBuf;

use crate::errors::{AppError, Result};

/// Whether `s` is a network address (`smb://…`, `sftp://…`) rather than a path. `file://` isn't:
/// Tauri already turns those into paths.
pub fn is_address(s: &str) -> bool {
    let Some((scheme, _)) = s.split_once("://") else {
        return false;
    };
    !scheme.is_empty()
        && scheme.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
        && scheme.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
        && !scheme.eq_ignore_ascii_case("file")
}

/// Tauri percent-decodes dropped addresses, so a file named `a#1.mp4` arrives as `…/a#1.mp4`.
/// Encodes the path again (everything after the server) so it reads back as the same file.
pub fn reencode(address: &str) -> String {
    let Some((scheme, rest)) = address.split_once("://") else {
        return address.to_string();
    };
    let (server, path) = match rest.find('/') {
        Some(i) => rest.split_at(i),
        None => (rest, ""),
    };
    let mut out = format!("{scheme}://{server}");
    for b in path.bytes() {
        if b.is_ascii_alphanumeric() || b"/-._~!$&'()*+,;=:@".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

const MOUNT_FIRST: &str =
    "Klipt can't open files from a network share directly. Mount the share as a folder first, then drop the files from there.";

/// Turns a network address into a local path through kio-fuse.
#[cfg(target_os = "linux")]
pub async fn local_path(address: &str) -> Result<PathBuf> {
    let failed = |e: ashpd::zbus::Error| {
        crate::elog!("klipt: kio-fuse couldn't mount {address}: {e}");
        AppError::Refused(MOUNT_FIRST.into())
    };
    let connection = ashpd::zbus::Connection::session().await.map_err(failed)?;
    let reply = connection
        .call_method(
            Some("org.kde.KIOFuse"),
            "/org/kde/KIOFuse",
            Some("org.kde.KIOFuse.VFS"),
            "mountUrl",
            &(reencode(address),),
        )
        .await
        .map_err(failed)?;
    let path: String = reply.body().deserialize().map_err(failed)?;
    Ok(PathBuf::from(path))
}

#[cfg(not(target_os = "linux"))]
pub async fn local_path(_address: &str) -> Result<PathBuf> {
    Err(AppError::Refused(MOUNT_FIRST.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tells_addresses_from_paths() {
        assert!(is_address("smb://nas/share/clip.mp4"));
        assert!(is_address("sftp://user@host/home/clip.mp4"));
        assert!(!is_address("/home/me/clip.mp4"));
        assert!(!is_address("file:///home/me/clip.mp4"));
        assert!(!is_address(r"C:\Videos\clip.mp4"));
        assert!(!is_address(r"\\nas\share\clip.mp4"));
        assert!(!is_address("/home/me/odd://name.mp4"));
    }

    #[test]
    fn encodes_the_path_again() {
        assert_eq!(reencode("smb://nas/share/my clip #1.mp4"), "smb://nas/share/my%20clip%20%231.mp4");
        assert_eq!(reencode("smb://user@nas:445/a/b.mp4"), "smb://user@nas:445/a/b.mp4");
        assert_eq!(reencode("smb://nas/Hra č.1.mp4"), "smb://nas/Hra%20%C4%8D.1.mp4");
        assert_eq!(reencode("smb://nas"), "smb://nas");
    }
}
