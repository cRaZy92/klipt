//! SQLite storage for the library (`<app data>/library.db`); thumbnails live in `<app data>/thumbs/`.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{params, Connection, OptionalExtension, Row};

use super::{AudioRange, BlurRange, LocalActivity, PendingUpload, Clip, ClipKind, ClipView, GameCount, ListFilter, NewClip, Sort, Trim, Volumes, CutRange, SubtitleRange, ZoomRange};
use super::{MAX_SUBTITLE_CHARS, MIN_AUDIO_RANGE_MS, MAX_ZOOM, MIN_BLUR_MS, MIN_SUBTITLE_MS, MIN_ZOOM, MIN_ZOOM_MS};
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
", "
    -- Volumes of audio tracks besides the game, mic and voice chat ones (imported clips), as a
    -- JSON object of track index to volume, e.g. {\"2\": 0.5}. Tracks not listed play at 1.
    ALTER TABLE clips ADD COLUMN extra_volumes TEXT NOT NULL DEFAULT '{}';
", "
    -- The editor's Blur video switch: uploads and exports blur the picture (only the sound matters).
    ALTER TABLE clips ADD COLUMN blur_video INTEGER NOT NULL DEFAULT 0;
", "
    -- Fields of an uploaded clip changed in the app that still have to reach the server: a comma
    -- list of title, game, favorite. Until then the server's value doesn't overwrite them.
    ALTER TABLE clips ADD COLUMN sync_pending TEXT NOT NULL DEFAULT '';
    -- Clips uploaded before syncing existed: the app's title and game win once, and a local star.
    UPDATE clips SET sync_pending = CASE WHEN favorite = 1 THEN 'title,game,favorite' ELSE 'title,game' END
        WHERE server_clip_id IS NOT NULL;
", "
    -- Parts removed in the editor: JSON list of {\"startMs\", \"endMs\"} (ms into the file).
    ALTER TABLE clips ADD COLUMN removed_ranges TEXT NOT NULL DEFAULT '[]';
", "
    -- Zooms set in the editor: JSON list of {\"startMs\", \"endMs\", \"zoom\", \"x\", \"y\"} (ms into the file).
    ALTER TABLE clips ADD COLUMN zoom_ranges TEXT NOT NULL DEFAULT '[]';
", "
    -- Subtitles typed in the editor: JSON list of {\"startMs\", \"endMs\", \"text\"} (ms into the file).
    ALTER TABLE clips ADD COLUMN subtitles TEXT NOT NULL DEFAULT '[]';
", "
    -- A full recording (started and stopped by the user), not a replay-buffer clip. Local only.
    ALTER TABLE clips ADD COLUMN is_recording INTEGER NOT NULL DEFAULT 0;
", "
    -- Parts of audio tracks muted or turned down in the editor: JSON list of
    -- {\"track\", \"startMs\", \"endMs\", \"level\"} (ms into the file).
    ALTER TABLE clips ADD COLUMN audio_ranges TEXT NOT NULL DEFAULT '[]';
", "
    -- Uploads started but not finished, so they carry on after a restart. `tagged` is a JSON
    -- list of usernames; `queued_at` is Unix ms.
    CREATE TABLE pending_uploads (
        clip_id    TEXT PRIMARY KEY,
        title      TEXT NOT NULL,
        visibility TEXT NOT NULL,
        tagged     TEXT NOT NULL DEFAULT '[]',
        replace    INTEGER NOT NULL DEFAULT 0,
        queued_at  INTEGER NOT NULL
    );
", "
    -- An admin's upload in high quality: encoded with less loss, not compressed by the server.
    ALTER TABLE pending_uploads ADD COLUMN high_quality INTEGER NOT NULL DEFAULT 0;
", "
    -- Parts of the picture blurred in the editor: JSON list of {\"startMs\", \"endMs\"} (ms into the file).
    ALTER TABLE clips ADD COLUMN blur_ranges TEXT NOT NULL DEFAULT '[]';
", "
    -- Server albums to put an upload in once it's online: JSON list of album ids.
    ALTER TABLE pending_uploads ADD COLUMN albums TEXT NOT NULL DEFAULT '[]';
", "
    -- 'combine', 'part' or 'download': made from other clips or fetched from the server, not
    -- recorded here. NULL for recorded and imported clips. Counts for achievements.
    ALTER TABLE clips ADD COLUMN origin TEXT NULL;
", "
    -- The server's 'feed' is called 'friends' now (beside 'link' and 'public').
    UPDATE clips SET visibility = 'friends' WHERE visibility = 'feed';
    UPDATE pending_uploads SET visibility = 'friends' WHERE visibility = 'feed';
"];

/// Queries that select more after these read them from index 34 on.
const COLUMNS: &str = "id, path, created_at, duration_ms, width, height, has_mic, game_name, steam_app_id, exe, \
                       title, trim_start_ms, trim_end_ms, game_volume, mic_volume, server_clip_id, share_url, \
                       visibility, upload_id, upload_offset, favorite, thumbnail_ms, has_voice, voice_volume, master_volume, kind, \
                       extra_volumes, blur_video, removed_ranges, zoom_ranges, subtitles, is_recording, audio_ranges, blur_ranges";

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
        // Unreadable JSON (hand-edited database) reads as "all at 1" rather than failing the clip.
        extra_volumes: serde_json::from_str(&row.get::<_, String>(26)?).unwrap_or_default(),
        blur_video: row.get(27)?,
        removed_ranges: serde_json::from_str(&row.get::<_, String>(28)?).unwrap_or_default(),
        zoom_ranges: serde_json::from_str(&row.get::<_, String>(29)?).unwrap_or_default(),
        subtitles: serde_json::from_str(&row.get::<_, String>(30)?).unwrap_or_default(),
        is_recording: row.get(31)?,
        audio_ranges: serde_json::from_str(&row.get::<_, String>(32)?).unwrap_or_default(),
        blur_ranges: serde_json::from_str(&row.get::<_, String>(33)?).unwrap_or_default(),
    })
}

fn split_fields(text: &str) -> Vec<String> {
    text.split(',').filter(|f| !f.is_empty()).map(String::from).collect()
}

/// A server copy's synced fields, as `GET /api/clips` returns them.
#[derive(Debug, Default, Clone)]
pub struct ServerFields {
    pub title: Option<String>,
    /// Game name (None = Unknown) and Steam app id.
    pub game: Option<(Option<String>, Option<u32>)>,
    pub favorite: Option<bool>,
    pub visibility: Option<String>,
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

    /// The picture the owner chose as clip `id`'s thumbnail (cut to the clip's shape). While it
    /// exists, the thumbnail is made from it instead of a frame, and uploads send it.
    pub fn thumb_image_path(&self, id: &str) -> PathBuf {
        self.thumbs_dir.join(format!("{id}.image.jpg"))
    }

    pub fn insert(&self, clip: NewClip) -> Result<Clip> {
        let id = uuid::Uuid::new_v4().simple().to_string();
        self.conn.execute(
            "INSERT INTO clips (id, path, created_at, duration_ms, width, height, has_mic, game_name, steam_app_id, exe, title, has_voice, kind, is_recording)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
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
                clip.is_recording,
            ],
        )?;
        self.get(&id)
    }

    /// Marks clip `id` as made from other clips (`combine`, `part`) or fetched from the server (`download`).
    pub fn set_origin(&self, id: &str, origin: &str) -> Result<()> {
        self.conn.execute("UPDATE clips SET origin = ?2 WHERE id = ?1", params![id, origin])?;
        Ok(())
    }

    /// What the library says about the user's activity, for achievements: counts and "firsts".
    /// Recorded clips are the ones Klipt captured here (not imported, combined, cut out or downloaded).
    pub fn activity(&self) -> Result<LocalActivity> {
        const RECORDED: &str = "kind = 'clip' AND source_path IS NULL AND origin IS NULL";
        let count = |sql: &str| -> Result<u64> { Ok(self.conn.query_row(sql, [], |r| r.get::<_, i64>(0))?.max(0) as u64) };
        let exists = |cond: &str| -> Result<bool> { Ok(count(&format!("SELECT EXISTS (SELECT 1 FROM clips WHERE {cond})"))? == 1) };
        let mut firsts = Vec::new();
        let checks: [(&'static str, String); 9] = [
            ("cut", "removed_ranges <> '[]'".into()),
            ("zoom", "zoom_ranges <> '[]'".into()),
            ("subtitle", "subtitles <> '[]'".into()),
            ("blur", "blur_ranges <> '[]'".into()),
            ("mutePart", "audio_ranges <> '[]'".into()),
            ("combine", "origin = 'combine'".into()),
            ("part", "origin = 'part'".into()),
            // Saved between 3:00 and 5:00 on this PC's clock.
            ("nightOwl", format!("{RECORDED} AND strftime('%H', created_at / 1000, 'unixepoch', 'localtime') IN ('03', '04')")),
            ("marathon", format!("{RECORDED} AND is_recording = 1 AND duration_ms > {}", 3 * 3_600_000)),
        ];
        for (first, cond) in checks {
            if exists(&cond)? {
                firsts.push(first);
            }
        }
        Ok(LocalActivity {
            clips_saved: count(&format!("SELECT COUNT(*) FROM clips WHERE {RECORDED} AND is_recording = 0"))?,
            screenshots: count("SELECT COUNT(*) FROM clips WHERE kind = 'screenshot' AND origin IS NULL")?,
            recorded_ms: count(&format!("SELECT COALESCE(SUM(duration_ms), 0) FROM clips WHERE {RECORDED}"))?,
            recordings: count(&format!("SELECT COUNT(*) FROM clips WHERE {RECORDED} AND is_recording = 1"))?,
            imports: count("SELECT COUNT(*) FROM clips WHERE source_path IS NOT NULL")?,
            games: count("SELECT COUNT(DISTINCT game_name) FROM clips WHERE game_name IS NOT NULL")?,
            firsts,
        })
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

    pub fn update_edit(&self, id: &str, trim: Trim, volumes: &Volumes) -> Result<()> {
        validate_edit(trim, volumes)?;
        let extra = serde_json::to_string(&volumes.extra).map_err(|e| AppError::Internal(e.to_string()))?;
        self.update(
            id,
            "UPDATE clips SET trim_start_ms = ?2, trim_end_ms = ?3, game_volume = ?4, mic_volume = ?5, voice_volume = ?6,
             master_volume = ?7, extra_volumes = ?8 WHERE id = ?1",
            params![id, trim.start_ms, trim.end_ms, volumes.game, volumes.mic, volumes.voice, volumes.master, extra],
        )
    }

    /// Stores the parts removed in the editor (checked with `validate_cuts` by the caller).
    pub fn set_removed_ranges(&self, id: &str, removed: &[CutRange]) -> Result<()> {
        let json = serde_json::to_string(removed).map_err(|e| AppError::Internal(e.to_string()))?;
        self.update(id, "UPDATE clips SET removed_ranges = ?2 WHERE id = ?1", params![id, json])
    }

    /// Stores the zooms set in the editor (checked with `validate_zooms` by the caller).
    pub fn set_zoom_ranges(&self, id: &str, zooms: &[ZoomRange]) -> Result<()> {
        let json = serde_json::to_string(zooms).map_err(|e| AppError::Internal(e.to_string()))?;
        self.update(id, "UPDATE clips SET zoom_ranges = ?2 WHERE id = ?1", params![id, json])
    }

    /// Stores the blurred parts set in the editor (checked with `validate_blurs` by the caller).
    pub fn set_blur_ranges(&self, id: &str, blurs: &[BlurRange]) -> Result<()> {
        let json = serde_json::to_string(blurs).map_err(|e| AppError::Internal(e.to_string()))?;
        self.update(id, "UPDATE clips SET blur_ranges = ?2 WHERE id = ?1", params![id, json])
    }

    /// Stores the muted and turned-down parts of the audio tracks (checked with
    /// `validate_audio_ranges` by the caller).
    pub fn set_audio_ranges(&self, id: &str, ranges: &[AudioRange]) -> Result<()> {
        let json = serde_json::to_string(ranges).map_err(|e| AppError::Internal(e.to_string()))?;
        self.update(id, "UPDATE clips SET audio_ranges = ?2 WHERE id = ?1", params![id, json])
    }

    /// Stores the subtitles typed in the editor (checked with `validate_subtitles` by the caller).
    pub fn set_subtitles(&self, id: &str, subs: &[SubtitleRange]) -> Result<()> {
        let json = serde_json::to_string(subs).map_err(|e| AppError::Internal(e.to_string()))?;
        self.update(id, "UPDATE clips SET subtitles = ?2 WHERE id = ?1", params![id, json])
    }

    /// The editor's "Blur video" for clip `id`.
    pub fn set_blur_video(&self, id: &str, blur: bool) -> Result<()> {
        self.update(id, "UPDATE clips SET blur_video = ?2 WHERE id = ?1", params![id, blur])
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

    /// Remembers that `field` (title, game or favorite) of clip `id` changed and still has to
    /// reach its server copy. Does nothing for a clip that isn't uploaded.
    pub fn mark_sync_pending(&self, id: &str, field: &str) -> Result<()> {
        let pending = self.sync_pending_of(id)?;
        if pending.iter().any(|f| f == field) {
            return Ok(());
        }
        let mut fields = pending;
        fields.push(field.to_string());
        self.conn.execute(
            "UPDATE clips SET sync_pending = ?2 WHERE id = ?1 AND server_clip_id IS NOT NULL",
            params![id, fields.join(",")],
        )?;
        Ok(())
    }

    fn sync_pending_of(&self, id: &str) -> Result<Vec<String>> {
        let text: Option<String> =
            self.conn.query_row("SELECT sync_pending FROM clips WHERE id = ?1", [id], |r| r.get(0)).optional()?;
        Ok(split_fields(&text.unwrap_or_default()))
    }

    /// Uploaded clips with edits still to send, with the fields to send.
    pub fn pending_syncs(&self) -> Result<Vec<(Clip, Vec<String>)>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {COLUMNS}, sync_pending FROM clips WHERE sync_pending != '' AND server_clip_id IS NOT NULL"
        ))?;
        let rows = stmt
            .query_map([], |row| Ok((row_to_clip(row)?, split_fields(&row.get::<_, String>(34)?))))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// The server took `sent`'s values of `fields`: those stop being pending, unless the clip was
    /// edited again meanwhile (then the newer value is still to send).
    pub fn clear_sync_pending(&self, sent: &Clip, fields: &[String]) -> Result<()> {
        let Ok(now) = self.get(&sent.id) else { return Ok(()) };
        let unchanged = |f: &str| match f {
            "title" => now.title == sent.title,
            "game" => now.game_name == sent.game_name && now.steam_app_id == sent.steam_app_id,
            "favorite" => now.favorite == sent.favorite,
            _ => true,
        };
        let left: Vec<String> = self
            .sync_pending_of(&sent.id)?
            .into_iter()
            .filter(|f| !(fields.contains(f) && unchanged(f)))
            .collect();
        self.conn.execute("UPDATE clips SET sync_pending = ?2 WHERE id = ?1", params![sent.id, left.join(",")])?;
        Ok(())
    }

    /// Copies the server copy's values into the local clips uploaded as it, except fields with
    /// edits still to send. `None` leaves a field as it is (the server didn't say).
    pub fn apply_server(&self, server_clip_id: &str, server: &ServerFields) -> Result<bool> {
        let mut stmt = self.conn.prepare("SELECT id, sync_pending FROM clips WHERE server_clip_id = ?1")?;
        let linked = stmt
            .query_map([server_clip_id], |r| Ok((r.get::<_, String>(0)?, split_fields(&r.get::<_, String>(1)?))))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut changed = 0;
        for (id, pending) in linked {
            let free = |f: &str| !pending.iter().any(|p| p == f);
            if let (Some(title), true) = (&server.title, free("title")) {
                changed += self.conn.execute(
                    "UPDATE clips SET title = ?2 WHERE id = ?1 AND title != ?2",
                    params![id, title],
                )?;
            }
            if let (Some((name, steam)), true) = (&server.game, free("game")) {
                changed += self.conn.execute(
                    "UPDATE clips SET game_name = ?2, steam_app_id = ?3 WHERE id = ?1
                     AND (game_name IS NOT ?2 OR steam_app_id IS NOT ?3)",
                    params![id, name, steam],
                )?;
            }
            if let (Some(favorite), true) = (server.favorite, free("favorite")) {
                changed += self.conn.execute(
                    "UPDATE clips SET favorite = ?2 WHERE id = ?1 AND favorite != ?2",
                    params![id, favorite],
                )?;
            }
            if let Some(visibility) = &server.visibility {
                changed += self.conn.execute(
                    "UPDATE clips SET visibility = ?2 WHERE id = ?1 AND visibility IS NOT ?2",
                    params![id, visibility],
                )?;
            }
        }
        Ok(changed > 0)
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
        self.remove_pending_upload(id)?;
        for thumb in [self.thumb_path(id), self.thumb_image_path(id)] {
            if let Err(e) = remove_if_present(&thumb) {
                crate::elog!("klipt: can't delete the thumbnail of clip {id}: {e}");
            }
        }
        Ok(())
    }

    /// Remembers an upload until it finishes. A retry updates its settings but keeps its place.
    pub fn add_pending_upload(&self, p: &PendingUpload) -> Result<()> {
        let visibility = p.visibility.as_str();
        self.conn.execute(
            "INSERT INTO pending_uploads (clip_id, title, visibility, tagged, replace, queued_at, high_quality, albums)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(clip_id) DO UPDATE SET title = excluded.title, visibility = excluded.visibility,
                 tagged = excluded.tagged, replace = excluded.replace, high_quality = excluded.high_quality,
                 albums = excluded.albums",
            params![
                p.clip_id,
                p.title,
                visibility,
                serde_json::to_string(&p.tagged)?,
                p.replace,
                p.queued_at_ms,
                p.high_quality,
                serde_json::to_string(&p.albums)?
            ],
        )?;
        Ok(())
    }

    pub fn remove_pending_upload(&self, clip_id: &str) -> Result<()> {
        self.conn.execute("DELETE FROM pending_uploads WHERE clip_id = ?1", [clip_id])?;
        Ok(())
    }

    pub fn clear_pending_uploads(&self) -> Result<()> {
        self.conn.execute("DELETE FROM pending_uploads", [])?;
        Ok(())
    }

    /// Unfinished uploads, oldest first.
    pub fn pending_uploads(&self) -> Result<Vec<PendingUpload>> {
        let mut stmt = self.conn.prepare(
            "SELECT clip_id, title, visibility, tagged, replace, queued_at, high_quality, albums FROM pending_uploads
             ORDER BY queued_at, clip_id",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                (row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?),
                (row.get::<_, bool>(4)?, row.get::<_, i64>(5)?, row.get::<_, bool>(6)?, row.get::<_, String>(7)?),
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let ((clip_id, title, visibility, tagged), (replace, queued_at_ms, high_quality, albums)) = row?;
            let visibility = crate::upload::Visibility::parse(&visibility);
            let tagged = serde_json::from_str(&tagged).unwrap_or_default();
            let albums = serde_json::from_str(&albums).unwrap_or_default();
            out.push(PendingUpload { clip_id, title, visibility, tagged, replace, queued_at_ms, high_quality, albums });
        }
        Ok(out)
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
        crate::elog!("klipt: library.db is from a newer Klipt (schema {version}); opening it anyway");
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
pub fn validate_edit(trim: Trim, volumes: &Volumes) -> Result<()> {
    if let (Some(start), Some(end)) = (trim.start_ms, trim.end_ms) {
        if start >= end {
            return Err(AppError::InvalidInput(format!("trim start {start} ms is not before end {end} ms")));
        }
    }
    let named = [volumes.game, volumes.mic, volumes.voice, volumes.master];
    for volume in named.into_iter().chain(volumes.extra.values().copied()) {
        if !(0.0..=MAX_VOLUME).contains(&volume) {
            return Err(AppError::InvalidInput(format!("volume {volume} is outside 0..={MAX_VOLUME}")));
        }
    }
    Ok(())
}

/// Checks the removed parts of a clip `duration_ms` long against its trim: each non-empty, sorted,
/// not overlapping, inside the trim, and something left to keep.
pub fn validate_cuts(trim: Trim, removed: &[CutRange], duration_ms: u64) -> Result<()> {
    let start = trim.start_ms.unwrap_or(0);
    let end = trim.end_ms.unwrap_or(duration_ms);
    let bad = |why: &str| Err(AppError::InvalidInput(format!("removed parts: {why}")));
    let mut at = start;
    let mut removed_ms = 0;
    for r in removed {
        if r.start_ms >= r.end_ms {
            return bad("a part is empty");
        }
        if r.start_ms < at {
            return bad("parts overlap, are out of order or start before the trim");
        }
        if r.end_ms > end {
            return bad("a part ends after the trim");
        }
        removed_ms += r.end_ms - r.start_ms;
        at = r.end_ms;
    }
    if !removed.is_empty() && removed_ms >= end.saturating_sub(start) {
        return bad("nothing of the clip is kept");
    }
    Ok(())
}

/// Checks the zooms of a clip `duration_ms` long: each at least `MIN_ZOOM_MS` long, sorted, not
/// overlapping (touching is fine), inside the file, and with its zoom and area in range. Zooms
/// outside the trim or in removed parts are allowed: they just don't show.
pub fn validate_zooms(zooms: &[ZoomRange], duration_ms: u64) -> Result<()> {
    let bad = |why: &str| Err(AppError::InvalidInput(format!("zooms: {why}")));
    let mut at = 0;
    for z in zooms {
        if z.end_ms < z.start_ms.saturating_add(MIN_ZOOM_MS) {
            return bad("a zoom is too short");
        }
        if z.start_ms < at {
            return bad("zooms overlap or are out of order");
        }
        if z.end_ms > duration_ms {
            return bad("a zoom ends after the clip");
        }
        // `contains` is false for NaN, so NaN is refused too.
        if !(MIN_ZOOM..=MAX_ZOOM).contains(&z.zoom) {
            return bad("a zoom is out of range");
        }
        if !(0.0..=1.0).contains(&z.x) || !(0.0..=1.0).contains(&z.y) {
            return bad("a zoom's area is outside the picture");
        }
        at = z.end_ms;
    }
    Ok(())
}

/// Checks the blurred parts of a clip `duration_ms` long: each at least `MIN_BLUR_MS` long,
/// sorted, not overlapping (touching is fine) and inside the file. Parts outside the trim or in
/// removed parts are allowed: they just don't show.
pub fn validate_blurs(blurs: &[BlurRange], duration_ms: u64) -> Result<()> {
    let bad = |why: &str| Err(AppError::InvalidInput(format!("blurred parts: {why}")));
    let mut at = 0;
    for b in blurs {
        if b.end_ms < b.start_ms.saturating_add(MIN_BLUR_MS) {
            return bad("a part is too short");
        }
        if b.start_ms < at {
            return bad("parts overlap or are out of order");
        }
        if b.end_ms > duration_ms {
            return bad("a part ends after the clip");
        }
        at = b.end_ms;
    }
    Ok(())
}

/// Checks the subtitles of a clip `duration_ms` long: each at least `MIN_SUBTITLE_MS` long,
/// sorted, not overlapping (touching is fine), inside the file, with one line of trimmed,
/// non-empty text of at most `MAX_SUBTITLE_CHARS` characters.
pub fn validate_subtitles(subs: &[SubtitleRange], duration_ms: u64) -> Result<()> {
    let bad = |why: &str| Err(AppError::InvalidInput(format!("subtitles: {why}")));
    let mut at = 0;
    for s in subs {
        if s.end_ms < s.start_ms.saturating_add(MIN_SUBTITLE_MS) {
            return bad("a subtitle is too short");
        }
        if s.start_ms < at {
            return bad("subtitles overlap or are out of order");
        }
        if s.end_ms > duration_ms {
            return bad("a subtitle ends after the clip");
        }
        if s.text.trim().is_empty() || s.text.trim() != s.text {
            return bad("a subtitle has no text or untrimmed text");
        }
        if s.text.contains(['\n', '\r']) {
            return bad("a subtitle has more than one line");
        }
        if s.text.chars().count() > MAX_SUBTITLE_CHARS {
            return bad("a subtitle is too long");
        }
        at = s.end_ms;
    }
    Ok(())
}

/// Checks the muted and turned-down parts of a clip `duration_ms` long: sorted by track, then
/// time; each at least `MIN_AUDIO_RANGE_MS` long, inside the file, at a level from 0 up to (not
/// including) 1; never overlapping on one track (touching is fine).
pub fn validate_audio_ranges(ranges: &[AudioRange], duration_ms: u64) -> Result<()> {
    let bad = |why: &str| Err(AppError::InvalidInput(format!("audio parts: {why}")));
    let mut prev: Option<&AudioRange> = None;
    for r in ranges {
        if r.end_ms < r.start_ms.saturating_add(MIN_AUDIO_RANGE_MS) {
            return bad("a part is too short");
        }
        if r.end_ms > duration_ms {
            return bad("a part ends after the clip");
        }
        if !(r.level.is_finite() && (0.0..1.0).contains(&r.level)) {
            return bad("a part's level must be from 0 to below 1");
        }
        if let Some(p) = prev {
            if r.track < p.track || (r.track == p.track && r.start_ms < p.end_ms) {
                return bad("parts overlap or are out of order");
            }
        }
        prev = Some(r);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
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
            is_recording: false,
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
    fn inserts_and_reads_back_is_recording() {
        let f = Fixture::new();
        let plain = f.add("a.mp4", 1_000, None);
        assert!(!plain.is_recording);
        let path = f.dir.path().join("Video_1.mp4");
        fs::write(&path, b"video").unwrap();
        let rec = f.lib.insert(NewClip { is_recording: true, ..new_clip(&path, 2_000, Some(AM)) }).unwrap();
        assert!(rec.is_recording);
        assert!(f.lib.get(&rec.id).unwrap().is_recording);
        assert!(!f.lib.get(&plain.id).unwrap().is_recording);
    }

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
    fn server_edits_skip_fields_waiting_to_sync() {
        let f = Fixture::new();
        let c = f.add("a.mp4", 1, None);
        f.lib.mark_sync_pending(&c.id, "title").unwrap(); // not uploaded: ignored
        assert!(f.lib.pending_syncs().unwrap().is_empty());
        f.lib.mark_uploaded(&c.id, "srv-1", "https://s/c/1", "link").unwrap();
        f.lib.rename(&c.id, "Mine").unwrap();
        f.lib.mark_sync_pending(&c.id, "title").unwrap();
        f.lib.mark_sync_pending(&c.id, "title").unwrap();

        let server = ServerFields {
            title: Some("Website".into()),
            game: Some((Some("Deadlock".into()), Some(1_422_450))),
            favorite: Some(true),
            visibility: Some("friends".into()),
        };
        assert!(f.lib.apply_server("srv-1", &server).unwrap());
        let got = f.lib.get(&c.id).unwrap();
        assert_eq!(got.title, "Mine");
        assert_eq!(got.game_name.as_deref(), Some("Deadlock"));
        assert!(got.favorite);
        assert_eq!(got.visibility.as_deref(), Some("friends"));
        assert!(!f.lib.apply_server("srv-1", &server).unwrap());

        let pending = f.lib.pending_syncs().unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].1, vec!["title".to_string()]);
        // Edited again while sending: still pending.
        f.lib.rename(&c.id, "Mine 2").unwrap();
        f.lib.clear_sync_pending(&pending[0].0, &pending[0].1).unwrap();
        assert_eq!(f.lib.pending_syncs().unwrap().len(), 1);
        let pending = f.lib.pending_syncs().unwrap();
        f.lib.clear_sync_pending(&pending[0].0, &pending[0].1).unwrap();
        assert!(f.lib.pending_syncs().unwrap().is_empty());
        f.lib.apply_server("srv-1", &server).unwrap();
        assert_eq!(f.lib.get(&c.id).unwrap().title, "Website");
    }

    fn pending(id: &str, title: &str, at: i64) -> PendingUpload {
        PendingUpload {
            clip_id: id.into(),
            title: title.into(),
            visibility: crate::upload::Visibility::Link,
            tagged: vec!["sam".into()],
            replace: true,
            queued_at_ms: at,
            high_quality: true,
            albums: vec!["album0000001".into()],
        }
    }

    #[test]
    fn pending_uploads_round_trip_oldest_first() {
        let f = Fixture::new();
        let a = f.add("a.mp4", 1, None);
        let b = f.add("b.mp4", 2, None);
        f.lib.add_pending_upload(&pending(&b.id, "B", 200)).unwrap();
        f.lib.add_pending_upload(&pending(&a.id, "A", 100)).unwrap();
        let got = f.lib.pending_uploads().unwrap();
        assert_eq!(got, vec![pending(&a.id, "A", 100), pending(&b.id, "B", 200)]);
    }

    #[test]
    fn pending_upload_retry_keeps_queue_place() {
        let f = Fixture::new();
        let a = f.add("a.mp4", 1, None);
        f.lib.add_pending_upload(&pending(&a.id, "Old", 100)).unwrap();
        f.lib.add_pending_upload(&pending(&a.id, "New", 300)).unwrap();
        let got = f.lib.pending_uploads().unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!((got[0].title.as_str(), got[0].queued_at_ms), ("New", 100));
    }

    #[test]
    fn removing_a_pending_upload() {
        let f = Fixture::new();
        let a = f.add("a.mp4", 1, None);
        f.lib.add_pending_upload(&pending(&a.id, "A", 100)).unwrap();
        f.lib.remove_pending_upload(&a.id).unwrap();
        assert!(f.lib.pending_uploads().unwrap().is_empty());
    }

    #[test]
    fn deleting_a_clip_drops_its_pending_upload() {
        let f = Fixture::new();
        let a = f.add("a.mp4", 1, None);
        f.lib.add_pending_upload(&pending(&a.id, "A", 100)).unwrap();
        f.lib.remove(&a.id, false).unwrap();
        assert!(f.lib.pending_uploads().unwrap().is_empty());
    }

    #[test]
    fn clear_pending_uploads_empties_the_table() {
        let f = Fixture::new();
        let a = f.add("a.mp4", 1, None);
        let b = f.add("b.mp4", 2, None);
        f.lib.add_pending_upload(&pending(&a.id, "A", 100)).unwrap();
        f.lib.add_pending_upload(&pending(&b.id, "B", 200)).unwrap();
        f.lib.clear_pending_uploads().unwrap();
        assert!(f.lib.pending_uploads().unwrap().is_empty());
    }

    #[test]
    fn edits_game_title_and_upload_state_persist() {
        let f = Fixture::new();
        let c = f.add("a.mp4", 1, None);
        f.lib
            .update_edit(&c.id, Trim { start_ms: Some(1_500), end_ms: Some(20_000) }, &Volumes { game: 0.8, mic: 1.5, voice: 0.4, master: 0.6, extra: Default::default() })
            .unwrap();
        f.lib.set_game(&c.id, Some("Active Matter"), Some(2_887_580)).unwrap();
        f.lib.rename(&c.id, "Nice shot").unwrap();
        f.lib.set_upload_progress(&c.id, Some(("https://s/api/uploads/u1", 8_388_608))).unwrap();
        let got = f.lib.get(&c.id).unwrap();
        assert_eq!(got.upload_id.as_deref(), Some("https://s/api/uploads/u1"));
        assert_eq!(got.upload_offset, Some(8_388_608));
        f.lib.mark_uploaded(&c.id, "srv-1", "https://clips.example.com/c/abc", "friends").unwrap();

        let got = f.lib.get(&c.id).unwrap();
        assert_eq!((got.trim_start_ms, got.trim_end_ms), (Some(1_500), Some(20_000)));
        assert_eq!((got.game_volume, got.mic_volume), (0.8, 1.5));
        assert_eq!(got.game_name.as_deref(), Some("Active Matter"));
        assert_eq!(got.title, "Nice shot");
        assert_eq!(got.server_clip_id.as_deref(), Some("srv-1"));
        assert_eq!(got.share_url.as_deref(), Some("https://clips.example.com/c/abc"));
        assert_eq!(got.visibility.as_deref(), Some("friends"));
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
        f.lib.mark_uploaded(&a.id, "srv-a", "https://s/c/a", "friends").unwrap();
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
    fn blur_video_persists() {
        let f = Fixture::new();
        let c = f.add("a.mp4", 1, None);
        assert!(!f.lib.get(&c.id).unwrap().blur_video, "clips aren't blurred unless asked");
        f.lib.set_blur_video(&c.id, true).unwrap();
        assert!(f.lib.get(&c.id).unwrap().blur_video);
        assert!(matches!(f.lib.set_blur_video("nope", true), Err(AppError::ClipNotFound(_))));
    }

    #[test]
    fn extra_track_volumes_persist_and_are_checked() {
        let f = Fixture::new();
        let c = f.add("a.mp4", 1, None);
        assert!(f.lib.get(&c.id).unwrap().extra_volumes.is_empty(), "new clips play every track at 1");
        let volumes = Volumes { extra: BTreeMap::from([(1, 0.25), (3, 2.0)]), ..Volumes::default() };
        f.lib.update_edit(&c.id, Trim::default(), &volumes).unwrap();
        assert_eq!(f.lib.get(&c.id).unwrap().extra_volumes, BTreeMap::from([(1, 0.25), (3, 2.0)]));
        let loud = Volumes { extra: BTreeMap::from([(1, 2.5)]), ..Volumes::default() };
        assert!(matches!(f.lib.update_edit(&c.id, Trim::default(), &loud), Err(AppError::InvalidInput(_))));
    }

    fn r(start_ms: u64, end_ms: u64) -> CutRange {
        CutRange { start_ms, end_ms }
    }

    #[test]
    fn removed_ranges_round_trip() {
        let f = Fixture::new();
        let c = f.add("a.mp4", 1, None);
        assert!(f.lib.get(&c.id).unwrap().removed_ranges.is_empty(), "new clips have no cuts");
        f.lib.set_removed_ranges(&c.id, &[r(2000, 3000), r(5000, 6000)]).unwrap();
        assert_eq!(f.lib.get(&c.id).unwrap().removed_ranges, vec![r(2000, 3000), r(5000, 6000)]);
        assert!(matches!(f.lib.set_removed_ranges("nope", &[]), Err(AppError::ClipNotFound(_))));
    }

    fn z(start_ms: u64, end_ms: u64, zoom: f32, x: f32, y: f32) -> ZoomRange {
        ZoomRange { start_ms, end_ms, zoom, x, y }
    }

    #[test]
    fn zoom_ranges_round_trip() {
        let f = Fixture::new();
        let c = f.add("a.mp4", 1000, None);
        assert!(f.lib.get(&c.id).unwrap().zoom_ranges.is_empty(), "new clips have no zooms");
        f.lib.set_zoom_ranges(&c.id, &[z(1000, 3000, 2.0, 0.5, 0.5)]).unwrap();
        assert_eq!(f.lib.get(&c.id).unwrap().zoom_ranges, vec![z(1000, 3000, 2.0, 0.5, 0.5)]);
        assert!(matches!(f.lib.set_zoom_ranges("nope", &[]), Err(AppError::ClipNotFound(_))));
    }

    #[test]
    fn blur_ranges_round_trip_and_bad_ones_are_refused() {
        let f = Fixture::new();
        let c = f.add("a.mp4", 1, None);
        let b = |start_ms, end_ms| BlurRange { start_ms, end_ms };
        assert!(f.lib.get(&c.id).unwrap().blur_ranges.is_empty(), "new clips have no blurred parts");
        f.lib.set_blur_ranges(&c.id, &[b(1000, 3000)]).unwrap();
        assert_eq!(f.lib.get(&c.id).unwrap().blur_ranges, vec![b(1000, 3000)]);
        assert!(matches!(f.lib.set_blur_ranges("nope", &[]), Err(AppError::ClipNotFound(_))));

        let bad = |blurs: &[BlurRange]| matches!(validate_blurs(blurs, 10_000), Err(AppError::InvalidInput(_)));
        assert!(bad(&[b(1000, 1200)]), "too short");
        assert!(bad(&[b(3000, 4000), b(1000, 2000)]), "out of order");
        assert!(bad(&[b(1000, 3000), b(2000, 4000)]), "overlapping");
        assert!(bad(&[b(9000, 10_001)]), "past the end");
        assert!(validate_blurs(&[], 10_000).is_ok());
        assert!(validate_blurs(&[b(0, 500), b(500, 10_000)], 10_000).is_ok(), "touching");
    }

    #[test]
    fn invalid_zooms_are_refused() {
        let bad = |zooms: &[ZoomRange]| matches!(validate_zooms(zooms, 10_000), Err(AppError::InvalidInput(_)));
        assert!(bad(&[z(3000, 3000, 2.0, 0.5, 0.5)]), "empty");
        assert!(bad(&[z(3000, 2000, 2.0, 0.5, 0.5)]), "backwards");
        assert!(bad(&[z(1000, 1499, 2.0, 0.5, 0.5)]), "499 ms");
        assert!(bad(&[z(1000, 3000, 2.0, 0.5, 0.5), z(2500, 4000, 2.0, 0.5, 0.5)]), "overlap");
        assert!(bad(&[z(5000, 6000, 2.0, 0.5, 0.5), z(1000, 2000, 2.0, 0.5, 0.5)]), "out of order");
        assert!(bad(&[z(9000, 10_001, 2.0, 0.5, 0.5)]), "past the end");
        assert!(bad(&[z(1000, 3000, 1.2, 0.5, 0.5)]));
        assert!(bad(&[z(1000, 3000, 4.1, 0.5, 0.5)]));
        assert!(bad(&[z(1000, 3000, 2.0, -0.1, 0.5)]));
        assert!(bad(&[z(1000, 3000, 2.0, 0.5, 1.1)]));
        assert!(bad(&[z(1000, 3000, f32::NAN, 0.5, 0.5)]));
        assert!(validate_zooms(&[], 10_000).is_ok());
        assert!(validate_zooms(&[z(0, 500, 1.25, 0.5, 0.5), z(500, 10_000, 4.0, 1.0, 0.0)], 10_000).is_ok(), "touching");
    }

    fn s(start_ms: u64, end_ms: u64, text: &str) -> SubtitleRange {
        SubtitleRange { start_ms, end_ms, text: text.into() }
    }

    #[test]
    fn subtitles_round_trip() {
        let f = Fixture::new();
        let c = f.add("a.mp4", 1000, None);
        assert!(f.lib.get(&c.id).unwrap().subtitles.is_empty(), "new clips have no subtitles");
        f.lib.set_subtitles(&c.id, &[s(1000, 3000, "čo to bolo")]).unwrap();
        assert_eq!(f.lib.get(&c.id).unwrap().subtitles, vec![s(1000, 3000, "čo to bolo")]);
        assert!(matches!(f.lib.set_subtitles("nope", &[]), Err(AppError::ClipNotFound(_))));
    }

    fn a(track: u32, start_ms: u64, end_ms: u64, level: f32) -> AudioRange {
        AudioRange { track, start_ms, end_ms, level }
    }

    #[test]
    fn audio_ranges_round_trip() {
        let f = Fixture::new();
        let c = f.add("a.mp4", 1000, None);
        assert!(f.lib.get(&c.id).unwrap().audio_ranges.is_empty(), "new clips have none");
        let ranges = [a(0, 1000, 3000, 0.0), a(1, 500, 900, 0.25)];
        f.lib.set_audio_ranges(&c.id, &ranges).unwrap();
        assert_eq!(f.lib.get(&c.id).unwrap().audio_ranges, ranges.to_vec());
        assert!(matches!(f.lib.set_audio_ranges("nope", &[]), Err(AppError::ClipNotFound(_))));
    }

    #[test]
    fn invalid_audio_ranges_are_refused() {
        let bad = |r: &[AudioRange]| matches!(validate_audio_ranges(r, 10_000), Err(AppError::InvalidInput(_)));
        assert!(bad(&[a(0, 1000, 1199, 0.0)]), "199 ms");
        assert!(bad(&[a(0, 3000, 2000, 0.0)]), "backwards");
        assert!(bad(&[a(0, 9000, 10_001, 0.0)]), "past the end");
        assert!(bad(&[a(0, 1000, 2000, 1.0)]), "level 1 changes nothing");
        assert!(bad(&[a(0, 1000, 2000, -0.1)]), "negative level");
        assert!(bad(&[a(0, 1000, 2000, f32::NAN)]), "NaN level");
        assert!(bad(&[a(0, 1000, 3000, 0.0), a(0, 2500, 4000, 0.5)]), "overlap on one track");
        assert!(bad(&[a(1, 1000, 2000, 0.0), a(0, 3000, 4000, 0.0)]), "tracks out of order");
        assert!(bad(&[a(0, 5000, 6000, 0.0), a(0, 1000, 2000, 0.0)]), "times out of order");
        assert!(validate_audio_ranges(&[], 10_000).is_ok());
        assert!(validate_audio_ranges(&[a(0, 0, 200, 0.0), a(0, 200, 10_000, 0.5)], 10_000).is_ok(), "touching");
        assert!(validate_audio_ranges(&[a(0, 1000, 3000, 0.0), a(1, 1000, 3000, 0.25)], 10_000).is_ok(), "same time, other track");
    }

    #[test]
    fn invalid_subtitles_are_refused() {
        let bad = |subs: &[SubtitleRange]| matches!(validate_subtitles(subs, 10_000), Err(AppError::InvalidInput(_)));
        assert!(bad(&[s(3000, 3000, "a")]), "empty");
        assert!(bad(&[s(3000, 2000, "a")]), "backwards");
        assert!(bad(&[s(1000, 1299, "a")]), "299 ms");
        assert!(bad(&[s(1000, 3000, "a"), s(2500, 4000, "b")]), "overlap");
        assert!(bad(&[s(5000, 6000, "a"), s(1000, 2000, "b")]), "out of order");
        assert!(bad(&[s(9000, 10_001, "a")]), "past the end");
        assert!(bad(&[s(1000, 3000, "")]), "no text");
        assert!(bad(&[s(1000, 3000, "   ")]), "blank text");
        assert!(bad(&[s(1000, 3000, "a\nb")]), "two lines");
        assert!(bad(&[s(1000, 3000, &"a".repeat(201))]), "201 chars");
        assert!(validate_subtitles(&[], 10_000).is_ok());
        assert!(validate_subtitles(&[s(0, 300, "a"), s(300, 10_000, "b")], 10_000).is_ok(), "touching");
        assert!(validate_subtitles(&[s(0, 1000, &"ľ".repeat(200))], 10_000).is_ok(), "200 chars, not bytes");
    }

    #[test]
    fn validate_cuts_rejects_bad_cuts() {
        let trim = Trim { start_ms: Some(1000), end_ms: Some(9000) };
        let bad = |cuts: &[CutRange]| matches!(validate_cuts(trim, cuts, 10_000), Err(AppError::InvalidInput(_)));
        assert!(bad(&[r(5000, 6000), r(2000, 3000)]), "unsorted");
        assert!(bad(&[r(2000, 4000), r(3000, 5000)]), "overlapping");
        assert!(bad(&[r(3000, 3000)]), "empty");
        assert!(bad(&[r(500, 2000)]), "before the trim");
        assert!(bad(&[r(8000, 9500)]), "after the trim");
        assert!(bad(&[r(1000, 9000)]), "nothing kept");
        assert!(validate_cuts(trim, &[r(1000, 2000), r(8000, 9000)], 10_000).is_ok(), "touching the edges");
        assert!(validate_cuts(Trim::default(), &[r(0, 500)], 10_000).is_ok());
        assert!(validate_cuts(Trim::default(), &[], 10_000).is_ok());
    }

    #[test]
    fn invalid_edits_and_unknown_ids_are_rejected() {
        let f = Fixture::new();
        let c = f.add("a.mp4", 1, None);
        let bad_trim = Trim { start_ms: Some(5_000), end_ms: Some(5_000) };
        assert!(matches!(f.lib.update_edit(&c.id, bad_trim, &Volumes::default()), Err(AppError::InvalidInput(_))));
        let bad_volume = Volumes { game: -0.1, ..Volumes::default() };
        assert!(matches!(f.lib.update_edit(&c.id, Trim::default(), &bad_volume), Err(AppError::InvalidInput(_))));
        let nan_volume = Volumes { voice: f32::NAN, ..Volumes::default() };
        assert!(matches!(f.lib.update_edit(&c.id, Trim::default(), &nan_volume), Err(AppError::InvalidInput(_))));

        assert!(matches!(f.lib.rename("nope", "x"), Err(AppError::ClipNotFound(_))));
        assert!(matches!(f.lib.set_game("nope", None, None), Err(AppError::ClipNotFound(_))));
        assert!(matches!(f.lib.remove("nope", true), Err(AppError::ClipNotFound(_))));
        assert!(matches!(
            f.lib.update_edit("nope", Trim::default(), &Volumes::default()),
            Err(AppError::ClipNotFound(_))
        ));
        assert!(matches!(f.lib.mark_uploaded("nope", "a", "b", "friends"), Err(AppError::ClipNotFound(_))));
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
    fn old_feed_clips_and_queued_uploads_become_friends() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.db");
        let before = MIGRATIONS.len() - 1;
        {
            let conn = Connection::open(&path).unwrap();
            for m in &MIGRATIONS[..before] {
                conn.execute_batch(m).unwrap();
            }
            conn.pragma_update(None, "user_version", before).unwrap();
            conn.execute(
                "INSERT INTO clips (id, path, created_at, duration_ms, width, height, has_mic, title, visibility)
                 VALUES ('old', '/x.mp4', 1, 1000, 1920, 1080, 0, 'Old', 'feed')",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO pending_uploads (clip_id, title, visibility, tagged, replace, queued_at) VALUES ('old', 'Old', 'feed', '[]', 0, 1)",
                [],
            )
            .unwrap();
        }
        let lib = Library::open(&path).unwrap();
        assert_eq!(lib.get("old").unwrap().visibility.as_deref(), Some("friends"));
        assert_eq!(lib.pending_uploads().unwrap()[0].visibility, crate::upload::Visibility::Friends);
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
        f.lib.mark_uploaded(&up.id, "srv", "https://k/c/srv", "friends").unwrap();
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

    /// Unix ms of 03:30 local time today, whatever the test machine's time zone.
    fn local_half_past_three(lib: &Library) -> i64 {
        lib.conn
            .query_row("SELECT CAST(strftime('%s', date('now', 'localtime') || ' 03:30', 'utc') AS INTEGER) * 1000", [], |r| r.get(0))
            .unwrap()
    }

    #[test]
    fn activity_counts_recorded_clips_only() {
        let f = Fixture::new();
        let night = local_half_past_three(&f.lib);
        let buffer = f.add("buffer.mp4", night, Some(AM));
        let path = f.dir.path().join("rec.mp4");
        fs::write(&path, b"video").unwrap();
        let rec = f
            .lib
            .insert(NewClip { is_recording: true, duration_ms: 3 * 3_600_000 + 60_000, ..new_clip(&path, night + 3_600_000 * 8, Some(CS)) })
            .unwrap();
        let imported = f.add("imported.mp4", 3_000, Some(("Doom", 1)));
        f.lib.set_source(&imported.id, &f.dir.path().join("old.mp4")).unwrap();
        let combined = f.add("combined.mp4", 4_000, None);
        f.lib.set_origin(&combined.id, "combine").unwrap();
        let shot_path = f.dir.path().join("shot.png");
        fs::write(&shot_path, b"png").unwrap();
        f.lib.insert(NewClip { kind: crate::library::ClipKind::Screenshot, ..new_clip(&shot_path, 5_000, None) }).unwrap();
        f.lib.set_removed_ranges(&buffer.id, &[CutRange { start_ms: 0, end_ms: 1_000 }]).unwrap();

        let a = f.lib.activity().unwrap();
        assert_eq!(a.clips_saved, 1);
        assert_eq!(a.recordings, 1);
        assert_eq!(a.imports, 1);
        assert_eq!(a.screenshots, 1);
        assert_eq!(a.recorded_ms, buffer.duration_ms + rec.duration_ms);
        assert_eq!(a.games, 3);
        for first in ["nightOwl", "marathon", "combine", "cut"] {
            assert!(a.firsts.contains(&first), "{first} in {:?}", a.firsts);
        }
        for first in ["zoom", "part", "subtitle"] {
            assert!(!a.firsts.contains(&first), "{first} in {:?}", a.firsts);
        }
    }

    #[test]
    fn activity_of_an_empty_library_is_zero() {
        let f = Fixture::new();
        let a = f.lib.activity().unwrap();
        assert_eq!((a.clips_saved, a.recorded_ms, a.games), (0, 0, 0));
        assert!(a.firsts.is_empty());
    }
}
