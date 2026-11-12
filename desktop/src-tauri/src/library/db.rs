//! SQLite storage for the library (`<app data>/library.db`); thumbnails live in `<app data>/thumbs/`.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{params, Connection, OptionalExtension, Row};

use super::{Clip, ClipKind, ClipView, GameCount, ListFilter, NewClip, Sort, Trim, Volumes};
use crate::errors::{AppError, Result};

/// Per-track volume range the editor offers (0–200 %).
pub const MAX_VOLUME: f32 = 2.0;

/// Schema migrations; `PRAGMA user_version` records how many have run. Only ever append.
const MIGRATIONS: &[&str] = &["
    CREATE TABLE clips (
        id             TEXT PRIMARY KEY,
        path           TEXT NOT NULL UNIQUE,
        created_at     INTEGER NOT NULL,
        duration_ms    INTEGER NOT NULL,
        width          INTEGER NOT NULL,
        height         INTEGER NOT NULL,
        has_mic        INTEGER NOT NULL,
        game_name      TEXT NULL,
        steam_app_id   INTEGER NULL,
        exe            TEXT NULL,
        title          TEXT NOT NULL,
        trim_start_ms  INTEGER NULL,
        trim_end_ms    INTEGER NULL,
        game_volume    REAL NOT NULL DEFAULT 1,
        mic_volume     REAL NOT NULL DEFAULT 1,
        server_clip_id TEXT NULL,
        share_url      TEXT NULL,
        visibility     TEXT NULL,
        upload_id      TEXT NULL,
        upload_offset  INTEGER NULL
    );
    CREATE INDEX clips_created_at ON clips (created_at);
    CREATE INDEX clips_game_name ON clips (game_name);
", "
    ALTER TABLE clips ADD COLUMN favorite INTEGER NOT NULL DEFAULT 0;
", "
    ALTER TABLE clips ADD COLUMN thumbnail_ms INTEGER NULL;
", "
    ALTER TABLE clips ADD COLUMN has_voice INTEGER NOT NULL DEFAULT 0;
    ALTER TABLE clips ADD COLUMN voice_volume REAL NOT NULL DEFAULT 1;
", "
    ALTER TABLE clips ADD COLUMN master_volume REAL NOT NULL DEFAULT 1;
", "
    ALTER TABLE clips ADD COLUMN kind TEXT NOT NULL DEFAULT 'clip';
", "
    -- Imported from a folder: the original file (the clip's own path when imported in place,
    -- else the file its H.264 copy was made from). NULL for clips Klipt recorded.
    ALTER TABLE clips ADD COLUMN source_path TEXT NULL;
    CREATE INDEX clips_source_path ON clips (source_path);
"];

const COLUMNS: &str = "id, path, created_at, duration_ms, width, height, has_mic, game_name, steam_app_id, exe, \
                       title, trim_start_ms, trim_end_ms, game_volume, mic_volume, server_clip_id, share_url, \
                       visibility, upload_id, upload_offset, favorite, thumbnail_ms, has_voice, voice_volume, master_volume, kind";

fn row_to_clip(row: &Row) -> rusqlite::Result<Clip> {
    Ok(Clip {
        id: row.get(0)?,
        path: PathBuf::from(row.get::<_, String>(1)?),
        created_at: row.get(2)?,
        duration_ms: row.get(3)?,
        width: row.get(4)?,
        height: row.get(5)?,
        has_mic: row.get(6)?,
        game_name: row.get(7)?,
        steam_app_id: row.get(8)?,
        exe: row.get(9)?,
        title: row.get(10)?,
        trim_start_ms: row.get(11)?,
        trim_end_ms: row.get(12)?,
        game_volume: row.get(13)?,
        mic_volume: row.get(14)?,
        server_clip_id: row.get(15)?,
        share_url: row.get(16)?,
        visibility: row.get(17)?,
        upload_id: row.get(18)?,
        upload_offset: row.get(19)?,
        favorite: row.get(20)?,
        thumbnail_ms: row.get(21)?,
        has_voice: row.get(22)?,
        voice_volume: row.get(23)?,
        master_volume: row.get(24)?,
        kind: if row.get::<_, String>(25)? == "screenshot" { ClipKind::Screenshot } else { ClipKind::Clip },
    })
}

pub struct Library {
    conn: Connection,
    thumbs_dir: PathBuf,
}

impl Library {
    /// Opens (or creates) the database and brings its schema up to date.
    pub fn open(path: &Path) -> Result<Library> {
        let dir = path.parent().unwrap_or(Path::new("."));
        fs::create_dir_all(dir)?;
        let conn = Connection::open(path)?;
        conn.busy_timeout(Duration::from_secs(5))?;
        conn.query_row("PRAGMA journal_mode = WAL", [], |_| Ok(()))?;
        // A damaged file can still open; find out now rather than on some later query.
        let check: String = conn.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
        if check != "ok" {
            let corrupt = rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_CORRUPT);
            return Err(rusqlite::Error::SqliteFailure(corrupt, Some(check)).into());
        }
        migrate(&conn)?;
        Ok(Library { conn, thumbs_dir: dir.join("thumbs") })
    }

    /// Where the thumbnail for clip `id` is (or will be) stored.
    pub fn thumb_path(&self, id: &str) -> PathBuf {
        self.thumbs_dir.join(format!("{id}.jpg"))
    }

    pub fn insert(&self, clip: NewClip) -> Result<Clip> {
        let id = uuid::Uuid::new_v4().simple().to_string();
        self.conn.execute(
            "INSERT INTO clips (id, path, created_at, duration_ms, width, height, has_mic, game_name, steam_app_id, exe, title, has_voice, kind)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                id,
                clip.path.to_string_lossy(),
                clip.created_at,
                clip.duration_ms,
                clip.width,
                clip.height,
                clip.has_mic,
                clip.game_name,
                clip.steam_app_id,
                clip.exe,
                clip.title,
                clip.has_voice,
                clip.kind.as_str(),
            ],
        )?;
        self.get(&id)
    }

    pub fn get(&self, id: &str) -> Result<Clip> {
        self.conn
            .query_row(&format!("SELECT {COLUMNS} FROM clips WHERE id = ?1"), [id], row_to_clip)
            .optional()?
            .ok_or_else(|| AppError::ClipNotFound(id.into()))
    }

    pub fn list(&self, filter: &ListFilter) -> Result<Vec<ClipView>> {
        let order = match filter.sort {
            Sort::NewestFirst => "created_at DESC, rowid DESC",
            Sort::OldestFirst => "created_at ASC, rowid ASC",
        };
        let mut stmt = self
            .conn
            .prepare(&format!("SELECT {COLUMNS} FROM clips WHERE ?1 IS NULL OR game_name = ?1 ORDER BY {order}"))?;
        let clips = stmt.query_map([&filter.game], row_to_clip)?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(clips
            .into_iter()
            .map(|clip| {
                let thumb = self.thumb_path(&clip.id);
                ClipView { file_missing: !clip.path.is_file(), thumb_path: thumb.is_file().then_some(thumb), clip }
            })
            .collect())
    }

    /// Games that have clips, most clips first.
    pub fn games(&self) -> Result<Vec<GameCount>> {
        let mut stmt = self.conn.prepare(
            "SELECT game_name, MAX(steam_app_id), COUNT(*) FROM clips WHERE game_name IS NOT NULL
             GROUP BY game_name ORDER BY COUNT(*) DESC, game_name ASC",
        )?;
        let games = stmt
            .query_map([], |row| Ok(GameCount { name: row.get(0)?, steam_app_id: row.get(1)?, count: row.get(2)? }))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(games)
    }

    pub fn update_edit(&self, id: &str, trim: Trim, volumes: Volumes) -> Result<()> {
        validate_edit(trim, volumes)?;
        self.update(
            id,
            "UPDATE clips SET trim_start_ms = ?2, trim_end_ms = ?3, game_volume = ?4, mic_volume = ?5, voice_volume = ?6,
             master_volume = ?7 WHERE id = ?1",
            params![id, trim.start_ms, trim.end_ms, volumes.game, volumes.mic, volumes.voice, volumes.master],
        )
    }

    pub fn set_game(&self, id: &str, name: Option<&str>, steam_app_id: Option<u32>) -> Result<()> {
        self.update(
            id,
            "UPDATE clips SET game_name = ?2, steam_app_id = ?3 WHERE id = ?1",
            params![id, name, steam_app_id],
        )
    }

    pub fn rename(&self, id: &str, title: &str) -> Result<()> {
        let title = title.trim();
        if title.is_empty() {
            return Err(AppError::InvalidInput("empty title".into()));
        }
        self.update(id, "UPDATE clips SET title = ?2 WHERE id = ?1", params![id, title])
    }

    /// Clips the automatic cleanup may delete: saved before `before` (Unix ms), not starred, not
    /// uploaded, with no upload in progress or paused, and recorded by Klipt (imported clips are
    /// the user's own files, or copies of them). Oldest first.
    pub fn cleanup_candidates(&self, before: i64) -> Result<Vec<Clip>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {COLUMNS} FROM clips WHERE created_at < ?1 AND favorite = 0 AND server_clip_id IS NULL
             AND upload_id IS NULL AND source_path IS NULL ORDER BY created_at ASC"
        ))?;
        let clips = stmt.query_map([before], row_to_clip)?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(clips)
    }

    /// Whether `file` is already in the library: as a clip's file, or as the original an imported
    /// copy was made from.
    pub fn contains_file(&self, file: &Path) -> Result<bool> {
        let file = file.to_string_lossy();
        let found: Option<i64> = self
            .conn
            .query_row("SELECT 1 FROM clips WHERE path = ?1 OR source_path = ?1 LIMIT 1", [file.as_ref()], |r| r.get(0))
            .optional()?;
        Ok(found.is_some())
    }

    /// Marks clip `id` as imported from `source` (see the `source_path` column).
    pub fn set_source(&self, id: &str, source: &Path) -> Result<()> {
        self.update(id, "UPDATE clips SET source_path = ?2 WHERE id = ?1", params![id, source.to_string_lossy()])
    }

    /// Where in the video the clip's thumbnail is taken (ms from the file's start); `None` is
    /// the automatic frame.
    pub fn set_thumbnail(&self, id: &str, at_ms: Option<u64>) -> Result<()> {
        self.update(id, "UPDATE clips SET thumbnail_ms = ?2 WHERE id = ?1", params![id, at_ms])
    }

    /// Stars or unstars a clip. Favorites are never deleted by the automatic cleanup.
    pub fn set_favorite(&self, id: &str, favorite: bool) -> Result<()> {
        self.update(id, "UPDATE clips SET favorite = ?2 WHERE id = ?1", params![id, favorite])
    }

    /// Remembers (or with `None`, forgets) how far a tus upload got, so a restart can resume it.
    pub fn set_upload_progress(&self, id: &str, progress: Option<(&str, u64)>) -> Result<()> {
        let (upload_id, offset) = progress.unzip();
        self.update(
            id,
            "UPDATE clips SET upload_id = ?2, upload_offset = ?3 WHERE id = ?1",
            params![id, upload_id, offset],
        )
    }

    pub fn mark_uploaded(&self, id: &str, server_clip_id: &str, share_url: &str, visibility: &str) -> Result<()> {
        self.update(
            id,
            "UPDATE clips SET server_clip_id = ?2, share_url = ?3, visibility = ?4, upload_id = NULL, upload_offset = NULL
             WHERE id = ?1",
            params![id, server_clip_id, share_url, visibility],
        )
    }

    /// The local clip uploaded as server clip `server_clip_id`, if any.
    pub fn find_by_server_id(&self, server_clip_id: &str) -> Result<Option<Clip>> {
        Ok(self
            .conn
            .query_row(
                &format!("SELECT {COLUMNS} FROM clips WHERE server_clip_id = ?1 ORDER BY created_at LIMIT 1"),
                [server_clip_id],
                row_to_clip,
            )
            .optional()?)
    }

    /// The server copy's visibility changed: local clips uploaded as it follow.
    pub fn set_server_visibility(&self, server_clip_id: &str, visibility: &str) -> Result<()> {
        self.conn.execute("UPDATE clips SET visibility = ?2 WHERE server_clip_id = ?1", params![server_clip_id, visibility])?;
        Ok(())
    }

    /// The server clip ids local clips were uploaded as, each once.
    pub fn server_clip_ids(&self) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare("SELECT DISTINCT server_clip_id FROM clips WHERE server_clip_id IS NOT NULL")?;
        let ids = stmt.query_map([], |row| row.get(0))?.collect::<std::result::Result<Vec<String>, _>>()?;
        Ok(ids)
    }

    /// The server copy was deleted: local clips uploaded as it count as not uploaded again.
    pub fn unlink_server_clip(&self, server_clip_id: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE clips SET server_clip_id = NULL, share_url = NULL, visibility = NULL WHERE server_clip_id = ?1",
            [server_clip_id],
        )?;
        Ok(())
    }

    /// Removes the clip from the library. With `delete_file`, also deletes the video file; if that
    /// fails (for example the file is open in a player), the clip stays in the library.
    pub fn remove(&self, id: &str, delete_file: bool) -> Result<()> {
        let clip = self.get(id)?;
        if delete_file {
            remove_if_present(&clip.path)?;
        }
        self.conn.execute("DELETE FROM clips WHERE id = ?1", [id])?;
        if let Err(e) = remove_if_present(&self.thumb_path(id)) {
            eprintln!("klipt: can't delete the thumbnail of clip {id}: {e}");
        }
        Ok(())
    }

    /// Runs an UPDATE of one clip; zero changed rows means the clip doesn't exist.
    fn update(&self, id: &str, sql: &str, params: impl rusqlite::Params) -> Result<()> {
        match self.conn.execute(sql, params)? {
            0 => Err(AppError::ClipNotFound(id.into())),
            _ => Ok(()),
        }
    }
}

fn migrate(conn: &Connection) -> Result<()> {
    let version: usize = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version > MIGRATIONS.len() {
        eprintln!("klipt: library.db is from a newer Klipt (schema {version}); opening it anyway");
    }
    for (index, sql) in MIGRATIONS.iter().enumerate().skip(version) {
        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", index + 1)?;
        tx.commit()?;
    }
    Ok(())
}

fn remove_if_present(path: &Path) -> std::io::Result<()> {
    match fs::remove_file(path) {
        Err(e) if e.kind() != ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// Checks a trim and volumes without touching the library.
pub fn validate_edit(trim: Trim, volumes: Volumes) -> Result<()> {
    if let (Some(start), Some(end)) = (trim.start_ms, trim.end_ms) {
        if start >= end {
            return Err(AppError::InvalidInput(format!("trim start {start} ms is not before end {end} ms")));
        }
    }
    for volume in [volumes.game, volumes.mic, volumes.voice, volumes.master] {
        if !(0.0..=MAX_VOLUME).contains(&volume) {
            return Err(AppError::InvalidInput(format!("volume {volume} is outside 0..={MAX_VOLUME}")));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::errors::AppError;
    use crate::library::Sort;
    use std::fs;

    fn new_clip(path: &Path, created_at: i64, game: Option<(&str, u32)>) -> NewClip {
        NewClip {
            path: path.to_path_buf(),
            created_at,
            duration_ms: 30_000,
            width: 1920,
            height: 1080,
            has_mic: false,
            has_voice: false,
            kind: crate::library::ClipKind::Clip,
            game_name: game.map(|g| g.0.to_string()),
            steam_app_id: game.map(|g| g.1),
            exe: Some("game.exe".into()),
            title: "Clip".into(),
        }
    }

    struct Fixture {
        dir: tempfile::TempDir,
        lib: Library,
    }

    impl Fixture {
        fn new() -> Fixture {
            let dir = tempfile::tempdir().unwrap();
            let lib = Library::open(&dir.path().join("data").join("library.db")).unwrap();
            Fixture { dir, lib }
        }

        /// Inserts a clip whose video file exists on disk.
        fn add(&self, name: &str, created_at: i64, game: Option<(&str, u32)>) -> Clip {
            let path = self.dir.path().join(name);
            fs::write(&path, b"video").unwrap();
            self.lib.insert(new_clip(&path, created_at, game)).unwrap()
        }
    }

    const AM: (&str, u32) = ("Active Matter", 2_887_580);
    const CS: (&str, u32) = ("Counter-Strike 2", 730);

    #[test]
    fn insert_fills_in_id_and_edit_defaults() {
        let f = Fixture::new();
        let c = f.add("a.mp4", 1_000, Some(AM));
        assert!(!c.id.is_empty());
        assert_eq!(c.game_name.as_deref(), Some("Active Matter"));
        assert_eq!(c.steam_app_id, Some(2_887_580));
        assert_eq!((c.trim_start_ms, c.trim_end_ms), (None, None));
        assert_eq!((c.game_volume, c.mic_volume), (1.0, 1.0));
        assert_eq!(c.server_clip_id, None);
        assert_eq!(f.lib.get(&c.id).unwrap(), c);
    }

    #[test]
    fn list_sorts_and_filters_by_game() {
        let f = Fixture::new();
        let a = f.add("a.mp4", 1_000, Some(AM));
        let b = f.add("b.mp4", 3_000, Some(CS));
        let c = f.add("c.mp4", 2_000, Some(AM));
        let d = f.add("d.mp4", 4_000, None);
        let ids = |filter: ListFilter| f.lib.list(&filter).unwrap().into_iter().map(|v| v.clip.id).collect::<Vec<_>>();

        assert_eq!(ids(ListFilter::default()), [d.id.as_str(), &b.id, &c.id, &a.id]);
        assert_eq!(ids(ListFilter { game: None, sort: Sort::OldestFirst }), [a.id.as_str(), &c.id, &b.id, &d.id]);
        assert_eq!(
            ids(ListFilter { game: Some("Active Matter".into()), sort: Sort::NewestFirst }),
            [c.id.as_str(), &a.id]
        );
        assert!(ids(ListFilter { game: Some("Nothing".into()), sort: Sort::NewestFirst }).is_empty());
    }

    #[test]
    fn games_are_counted_most_clips_first() {
        let f = Fixture::new();
        f.add("a.mp4", 1, Some(AM));
        f.add("b.mp4", 2, Some(CS));
        f.add("c.mp4", 3, Some(AM));
        f.add("d.mp4", 4, None);
        assert_eq!(
            f.lib.games().unwrap(),
            [
                GameCount { name: "Active Matter".into(), steam_app_id: Some(2_887_580), count: 2 },
                GameCount { name: "Counter-Strike 2".into(), steam_app_id: Some(730), count: 1 },
            ]
        );
    }

    // Review Focus 4: a clip deleted outside the app.
    #[test]
    fn a_deleted_file_lists_as_missing_and_can_be_removed() {
        let f = Fixture::new();
        let c = f.add("gone.mp4", 1, Some(AM));
        assert!(!f.lib.list(&ListFilter::default()).unwrap()[0].file_missing);

        fs::remove_file(&c.path).unwrap();
        let views = f.lib.list(&ListFilter::default()).unwrap();
        assert_eq!(views.len(), 1);
        assert!(views[0].file_missing);
        assert_eq!(views[0].thumb_path, None);

        f.lib.remove(&c.id, false).unwrap();
        assert!(f.lib.list(&ListFilter::default()).unwrap().is_empty());
        // Deleting the file of a clip whose file is already gone is fine too.
        let c2 = f.add("gone2.mp4", 2, None);
        fs::remove_file(&c2.path).unwrap();
        f.lib.remove(&c2.id, true).unwrap();
        assert!(f.lib.list(&ListFilter::default()).unwrap().is_empty());
    }

    #[test]
    fn remove_keeps_or_deletes_the_file_and_thumbnail() {
        let f = Fixture::new();
        let keep = f.add("keep.mp4", 1, None);
        f.lib.remove(&keep.id, false).unwrap();
        assert!(keep.path.exists(), "remove without delete_file leaves the video alone");

        let del = f.add("delete.mp4", 2, None);
        let thumb = f.lib.thumb_path(&del.id);
        fs::create_dir_all(thumb.parent().unwrap()).unwrap();
        fs::write(&thumb, b"jpeg").unwrap();
        assert_eq!(f.lib.list(&ListFilter::default()).unwrap()[0].thumb_path.as_deref(), Some(thumb.as_path()));

        f.lib.remove(&del.id, true).unwrap();
        assert!(!del.path.exists());
        assert!(!thumb.exists());
        assert!(matches!(f.lib.get(&del.id), Err(AppError::ClipNotFound(_))));
    }

    #[test]
    fn edits_game_title_and_upload_state_persist() {
        let f = Fixture::new();
        let c = f.add("a.mp4", 1, None);
        f.lib
            .update_edit(&c.id, Trim { start_ms: Some(1_500), end_ms: Some(20_000) }, Volumes { game: 0.8, mic: 1.5, voice: 0.4, master: 0.6 })
            .unwrap();
        f.lib.set_game(&c.id, Some("Active Matter"), Some(2_887_580)).unwrap();
        f.lib.rename(&c.id, "Nice shot").unwrap();
        f.lib.set_upload_progress(&c.id, Some(("https://s/api/uploads/u1", 8_388_608))).unwrap();
        let got = f.lib.get(&c.id).unwrap();
        assert_eq!(got.upload_id.as_deref(), Some("https://s/api/uploads/u1"));
        assert_eq!(got.upload_offset, Some(8_388_608));
        f.lib.mark_uploaded(&c.id, "srv-1", "https://clips.example.com/c/abc", "feed").unwrap();

        let got = f.lib.get(&c.id).unwrap();
        assert_eq!((got.trim_start_ms, got.trim_end_ms), (Some(1_500), Some(20_000)));
        assert_eq!((got.game_volume, got.mic_volume), (0.8, 1.5));
        assert_eq!(got.game_name.as_deref(), Some("Active Matter"));
        assert_eq!(got.title, "Nice shot");
        assert_eq!(got.server_clip_id.as_deref(), Some("srv-1"));
        assert_eq!(got.share_url.as_deref(), Some("https://clips.example.com/c/abc"));
        assert_eq!(got.visibility.as_deref(), Some("feed"));
        assert_eq!((got.upload_id, got.upload_offset), (None, None));

        f.lib.set_game(&c.id, None, None).unwrap();
        assert_eq!(f.lib.get(&c.id).unwrap().game_name, None);
    }

    #[test]
    fn server_clip_ids_lists_linked_clips_and_unlink_clears_them() {
        let f = Fixture::new();
        let a = f.add("a.mp4", 1, None);
        let b = f.add("b.mp4", 2, None);
        f.add("c.mp4", 3, None);
        f.lib.mark_uploaded(&a.id, "srv-a", "https://s/c/a", "feed").unwrap();
        f.lib.mark_uploaded(&b.id, "srv-b", "https://s/c/b", "link").unwrap();
        let mut ids = f.lib.server_clip_ids().unwrap();
        ids.sort();
        assert_eq!(ids, ["srv-a", "srv-b"]);

        f.lib.unlink_server_clip("srv-a").unwrap();
        assert_eq!(f.lib.server_clip_ids().unwrap(), ["srv-b"]);
        let got = f.lib.get(&a.id).unwrap();
        assert_eq!((got.server_clip_id, got.share_url, got.visibility), (None, None, None));
    }

    #[test]
    fn invalid_edits_and_unknown_ids_are_rejected() {
        let f = Fixture::new();
        let c = f.add("a.mp4", 1, None);
        let bad_trim = Trim { start_ms: Some(5_000), end_ms: Some(5_000) };
        assert!(matches!(f.lib.update_edit(&c.id, bad_trim, Volumes::default()), Err(AppError::InvalidInput(_))));
        let bad_volume = Volumes { game: -0.1, ..Volumes::default() };
        assert!(matches!(f.lib.update_edit(&c.id, Trim::default(), bad_volume), Err(AppError::InvalidInput(_))));
        let nan_volume = Volumes { voice: f32::NAN, ..Volumes::default() };
        assert!(matches!(f.lib.update_edit(&c.id, Trim::default(), nan_volume), Err(AppError::InvalidInput(_))));

        assert!(matches!(f.lib.rename("nope", "x"), Err(AppError::ClipNotFound(_))));
        assert!(matches!(f.lib.set_game("nope", None, None), Err(AppError::ClipNotFound(_))));
        assert!(matches!(f.lib.remove("nope", true), Err(AppError::ClipNotFound(_))));
        assert!(matches!(
            f.lib.update_edit("nope", Trim::default(), Volumes::default()),
            Err(AppError::ClipNotFound(_))
        ));
        assert!(matches!(f.lib.mark_uploaded("nope", "a", "b", "feed"), Err(AppError::ClipNotFound(_))));
    }

    #[test]
    fn reopening_keeps_the_clips() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("library.db");
        let id = {
            let lib = Library::open(&db).unwrap();
            lib.insert(new_clip(&dir.path().join("a.mp4"), 1, Some(AM))).unwrap().id
        };
        let lib = Library::open(&db).unwrap();
        assert_eq!(lib.get(&id).unwrap().game_name.as_deref(), Some("Active Matter"));
    }

    #[test]
    fn the_same_file_cannot_be_added_twice() {
        let f = Fixture::new();
        let c = f.add("a.mp4", 1, None);
        assert!(f.lib.insert(new_clip(&c.path, 2, None)).is_err());
    }

    #[test]
    fn favorites_start_off_and_can_be_toggled() {
        let f = Fixture::new();
        let c = f.add("fav.mp4", 1, None);
        assert!(!c.favorite);
        f.lib.set_favorite(&c.id, true).unwrap();
        assert!(f.lib.get(&c.id).unwrap().favorite);
        f.lib.set_favorite(&c.id, false).unwrap();
        assert!(!f.lib.get(&c.id).unwrap().favorite);
        assert!(matches!(f.lib.set_favorite("nope", true), Err(AppError::ClipNotFound(_))));
    }

    #[test]
    fn an_old_library_gains_the_favorite_column() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.db");
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(MIGRATIONS[0]).unwrap();
            conn.pragma_update(None, "user_version", 1).unwrap();
            conn.execute(
                "INSERT INTO clips (id, path, created_at, duration_ms, width, height, has_mic, title)
                 VALUES ('old', '/x.mp4', 1, 1000, 1920, 1080, 0, 'Old')",
                [],
            )
            .unwrap();
        }
        let lib = Library::open(&path).unwrap();
        assert!(!lib.get("old").unwrap().favorite);
    }

    #[test]
    fn cleanup_keeps_new_favorite_uploaded_and_uploading_clips() {
        let f = Fixture::new();
        let old = f.add("old.mp4", 100, None);
        let older = f.add("older.mp4", 50, None);
        f.add("new.mp4", 1_000, None);
        let fav = f.add("fav.mp4", 100, None);
        f.lib.set_favorite(&fav.id, true).unwrap();
        let up = f.add("up.mp4", 100, None);
        f.lib.mark_uploaded(&up.id, "srv", "https://k/c/srv", "feed").unwrap();
        let paused = f.add("paused.mp4", 100, None);
        f.lib.set_upload_progress(&paused.id, Some(("tus-1", 10))).unwrap();
        let imported = f.add("imported.mp4", 100, None);
        f.lib.set_source(&imported.id, &imported.path).unwrap();
        let ids: Vec<String> = f.lib.cleanup_candidates(500).unwrap().into_iter().map(|c| c.id).collect();
        assert_eq!(ids, vec![older.id, old.id]);
    }

    #[test]
    fn contains_file_knows_clip_files_and_import_sources() {
        let f = Fixture::new();
        let in_place = f.add("a.mp4", 1, None);
        let copy = f.add("copy.mp4", 2, None);
        let original = f.dir.path().join("original.mkv");
        f.lib.set_source(&copy.id, &original).unwrap();
        assert!(f.lib.contains_file(&in_place.path).unwrap());
        assert!(f.lib.contains_file(&original).unwrap());
        assert!(!f.lib.contains_file(&f.dir.path().join("other.mp4")).unwrap());
    }

    #[test]
    fn thumbnail_frame_is_stored() {
        let f = Fixture::new();
        let c = f.add("t.mp4", 1, None);
        assert_eq!(c.thumbnail_ms, None);
        f.lib.set_thumbnail(&c.id, Some(4_200)).unwrap();
        assert_eq!(f.lib.get(&c.id).unwrap().thumbnail_ms, Some(4_200));
        f.lib.set_thumbnail(&c.id, None).unwrap();
        assert_eq!(f.lib.get(&c.id).unwrap().thumbnail_ms, None);
    }
}

