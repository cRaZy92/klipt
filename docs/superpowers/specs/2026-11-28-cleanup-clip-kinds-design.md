# Cleanup: choose which clips it deletes

## Why

The automatic cleanup (Settings → Cleanup) only deletes clips that have never been uploaded, so the
clips with no copy anywhere else are the ones it removes. Uploaded clips are already on the server,
so they're the most natural ones to free up space with. The owner wants to choose which kinds of
clips the cleanup may delete.

## Goals

- Under the existing "Delete clips older than N days" setting, add a checkbox for each kind of clip
  the cleanup may delete: **Local only** (never uploaded), **Uploaded** and **Favorites**.
- Defaults: the cleanup stays **off**. Only **Uploaded** is checked.
- All checked kinds use the same age (one `cleanupDays`).

## Non-goals

- A separate age for each kind (option B; not chosen).
- Deleting anything from the server. Cleanup only touches files on this PC.

## Anti-goals

- Never deletes a file the app doesn't own (imported clips point at the user's original file), or a
  clip with an upload in progress or paused.
- An existing user who already has cleanup on must not suddenly start losing a different set of clips
  after updating.

## Approaches considered

- **A. One age plus a checkbox per kind** (chosen): simple, and covers "free space, but keep what
  matters".
- **B. A separate age per kind**: more flexible, more settings to understand. Not chosen.

## Design

- **Settings** (Rust `Settings` and TS `Settings`): add `cleanup_local: bool`, `cleanup_uploaded: bool`,
  `cleanup_favorites: bool`. Defaults: `false`, `true`, `false`.
- **Which kind a clip counts as**: a clip is deleted only if **every** kind it has is checked
  (overlaps are cautious). So an uploaded favorite needs both Uploaded and Favorites checked.
  Local = `server_clip_id IS NULL`; Uploaded = `server_clip_id IS NOT NULL`; Favorite = `favorite = 1`.
- **Always kept**: imported clips (`source_path` set), clips with an `upload_id` (in progress or
  paused), and clips busy uploading or exporting right now. Same as today.
- **The library query** `cleanup_candidates` takes the three flags and filters to match. The
  per-clip re-check in `cleanup::clean` (it reloads the clip before deleting) applies the same rule,
  through one shared function so the two can't drift apart.
- **Migrating existing settings** (my call): a settings file saved before this change has none of the
  new fields. If it has `cleanup_enabled: true`, load it as Local ✓, Uploaded ✗, Favorites ✗. That's
  exactly today's behavior, so nothing changes for them until they edit it. Otherwise use the new
  defaults.
- **Validation** (my call): cleanup on with no kind checked is a field error ("Pick at least one kind
  of clip to delete."), so it can't look on while doing nothing.
- **UI** (Settings → Cleanup): three checkboxes under the days field, under a small label like
  "Delete these kinds of clips". Hints: Uploaded says "The server keeps its copy, which may be
  compressed."; Favorites says "Starred clips." The switch's hint drops "Uploaded and favorite clips
  are always kept." and keeps "Deleted clips can't be recovered." The favorite star's tooltip on
  clip cards ("kept by the automatic cleanup") only says that when Favorites isn't checked.
- Update doc comments that say "uploaded and favorite clips are kept" (`cleanup.rs`, `settings.rs`,
  `api.ts`).

## Left to the builder

- Exact wording and layout of the checkboxes, matching the Settings page style.
- How the migration detects the old format (for example, `Option` fields during deserialization).
- Tests: the query/rule for each combination of kinds, migration of an old settings file with cleanup
  on and off, the "no kind checked" validation.
