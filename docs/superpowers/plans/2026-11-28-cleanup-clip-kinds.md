# Cleanup Clip Kinds Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let the user choose which kinds of clips (Local only / Uploaded / Favorites) the automatic cleanup deletes.

**Architecture:** Three new bools on `Settings` (Rust + TS). The library query returns every old clip the app owns and isn't uploading. One rule, `CleanupKinds::allows(&Clip)`, decides which of those get deleted, both on the candidate list and on the per-clip re-check. Old settings files with cleanup on load as "Local only" so their behavior doesn't change.

**Tech Stack:** Rust (Tauri, rusqlite, serde_json), Svelte 5 + TypeScript, vitest.

**Spec:** `docs/superpowers/specs/2026-11-28-cleanup-clip-kinds-design.md`

All paths are relative to `desktop/`. Rust tests run with `cargo test --manifest-path src-tauri/Cargo.toml <filter>`; frontend with `pnpm test` and `pnpm check`.

## Global Constraints

- Defaults: `cleanupEnabled: false`, `cleanupLocal: false`, `cleanupUploaded: true`, `cleanupFavorites: false`, `cleanupDays: 30`.
- A clip is deleted only if **every** kind it has is checked (an uploaded favorite needs Uploaded **and** Favorites).
- Never deleted: imported clips (`source_path` set), clips with an `upload_id`, clips the `busy` callback reports.
- Old settings file (no `cleanupLocal` key) with `cleanupEnabled: true` → local ✓, uploaded ✗, favorites ✗. Without cleanup on → the new defaults.
- Validation field `cleanupKinds`, message exactly: `Pick at least one kind of clip to delete.` (only when cleanup is on and none checked).
- Copy: switch hint `Deleted clips can't be recovered.`; group label `Delete these kinds of clips`; checkboxes `Local only`, `Uploaded` (hint `The server keeps its copy, which may be compressed.`), `Favorites` (hint `Starred clips.`).

## Review Focus

1. Updating with cleanup already on: the next run must delete exactly the same clips as before the update (Task 1 migration test + Task 2 rule test for local-only).
2. A clip starred or uploaded between listing and deleting must be re-checked against the *new* rule, not the old "never uploaded" check (Task 2 re-check test).
3. Hand-edited file with cleanup on and all three kinds false must not load as a cleanup that silently does nothing: reset to defaults (Task 1).
4. Uploaded favorite with only Uploaded checked is kept (Task 2).
5. Imported clips stay out of the cleanup even with every kind checked (Task 2).

---

### Task 1: Settings fields, migration, validation (Rust)

**Files:**
- Modify: `src-tauri/src/settings.rs` (struct ~line 79, `Default` ~150, `load` ~199, `validate` ~273, `with_invalid_fields_reset` ~305, tests)

**Interfaces:**
- Produces: `Settings { cleanup_local: bool, cleanup_uploaded: bool, cleanup_favorites: bool }` (camelCase in JSON); `fn parse(bytes: &[u8]) -> serde_json::Result<Settings>` (private, used by `load`); validation field `"cleanupKinds"`.

- [ ] **Step 1: Write the failing tests** in `settings.rs` `mod tests`:

```rust
#[test]
fn cleanup_kinds_default_to_uploaded_only() {
    let d = Settings::default();
    assert!(!d.cleanup_enabled);
    assert!((d.cleanup_local, d.cleanup_uploaded, d.cleanup_favorites) == (false, true, false));
}

#[test]
fn old_files_with_cleanup_on_keep_deleting_local_clips_only() {
    let on = parse(br#"{"cleanupEnabled":true,"cleanupDays":10}"#).unwrap();
    assert!((on.cleanup_local, on.cleanup_uploaded, on.cleanup_favorites) == (true, false, false));
    let off = parse(br#"{"cleanupEnabled":false}"#).unwrap();
    assert!((off.cleanup_local, off.cleanup_uploaded, off.cleanup_favorites) == (false, true, false));
    // Saved by the new version: taken as is.
    let new = parse(br#"{"cleanupEnabled":true,"cleanupLocal":false,"cleanupUploaded":true,"cleanupFavorites":true}"#).unwrap();
    assert!((new.cleanup_local, new.cleanup_uploaded, new.cleanup_favorites) == (false, true, true));
}

#[test]
fn cleanup_on_needs_at_least_one_kind() {
    let none = Settings { cleanup_enabled: true, cleanup_local: false, cleanup_uploaded: false, cleanup_favorites: false, ..Settings::default() };
    let e = none.validate();
    assert!(e.iter().any(|e| e.field == "cleanupKinds" && e.message == "Pick at least one kind of clip to delete."));
    let off = Settings { cleanup_enabled: false, ..none.clone() };
    assert!(off.validate().iter().all(|e| e.field != "cleanupKinds"));
    let reset = none.with_invalid_fields_reset();
    assert!((reset.cleanup_local, reset.cleanup_uploaded, reset.cleanup_favorites) == (false, true, false));
}
```

(Adjust `e.message` to the actual field name of the validation error struct.)

- [ ] **Step 2: Run** `cargo test --manifest-path src-tauri/Cargo.toml settings::` — Expected: compile errors (fields / `parse` missing).

- [ ] **Step 3: Implement.** Add the three fields after `cleanup_days` with doc comments; defaults per Global Constraints. Add `fn parse(bytes)`: deserialize to `serde_json::Value`, then to `Settings`; if the object has no `cleanupLocal` key and `cleanup_enabled` is true, set local=true, uploaded=false, favorites=false. `load` calls `parse` instead of `serde_json::from_slice`. In `validate`, fail `"cleanupKinds"` when enabled and all three false; in `with_invalid_fields_reset`, `"cleanupKinds"` resets the three to defaults. Update the `cleanup_enabled` doc comment (drop "uploaded and favorite clips are kept").

- [ ] **Step 4: Run** the same command — Expected: all `settings::` tests PASS (including the existing `cleanup_is_off_by_default...` and the JSON-shape test near line 389; update that test if it lists keys).

- [ ] **Step 5: Commit** `feat(desktop): settings for which kinds of clips the cleanup deletes`

### Task 2: The cleanup rule, query and run (Rust)

**Files:**
- Modify: `src-tauri/src/cleanup.rs` (module doc, `clean`, tests)
- Modify: `src-tauri/src/library/db.rs:382-389` (`cleanup_candidates`) and its test at ~1378

**Interfaces:**
- Consumes: Task 1's three `Settings` fields.
- Produces: in `cleanup.rs`, `#[derive(Clone, Copy)] pub struct CleanupKinds { pub local: bool, pub uploaded: bool, pub favorites: bool }`, `impl CleanupKinds { pub fn from_settings(s: &Settings) -> Self; pub fn allows(&self, clip: &Clip) -> bool }`. `Library::cleanup_candidates(before: i64) -> Result<Vec<Clip>>` keeps its signature, but now returns every clip with `created_at < before AND upload_id IS NULL AND source_path IS NULL` (favorites and uploaded included), oldest first.

- [ ] **Step 1: Write the failing tests.**

In `db.rs`, rename/replace `cleanup_keeps_new_favorite_uploaded_and_uploading_clips` with `cleanup_candidates_are_old_owned_clips_not_uploading`: same fixture, but assert the result is `[older, old, fav, up]` in `created_at`-then-insert order (`paused`, `imported`, `new` absent). Give `fav` and `up` distinct `created_at` (e.g. 60, 70) so the order is fixed: `[older(50), fav(60), up(70), old(100)]`.

In `cleanup.rs` tests:

```rust
#[test]
fn allows_a_clip_only_when_every_kind_it_has_is_checked() {
    // build Clips via `add` + lock(library).get(..), then set favorite / mark_uploaded
    let k = |l, u, f| CleanupKinds { local: l, uploaded: u, favorites: f };
    // local clip
    assert!(k(true, false, false).allows(&local));
    assert!(!k(false, true, true).allows(&local));
    // uploaded clip
    assert!(k(false, true, false).allows(&uploaded));
    assert!(!k(true, false, true).allows(&uploaded));
    // uploaded favorite
    assert!(!k(false, true, false).allows(&uploaded_fav));
    assert!(k(false, true, true).allows(&uploaded_fav));
    // local favorite
    assert!(!k(true, false, false).allows(&local_fav));
    assert!(k(true, false, true).allows(&local_fav));
    // upload paused
    assert!(!k(true, true, true).allows(&paused));
}

#[test]
fn deletes_only_the_checked_kinds() {
    // old local, old uploaded, old favorite; defaults (uploaded only) + enabled, days 30
    // → only the uploaded one is deleted; report.deleted == 1
}

#[test]
fn rechecks_each_clip_with_the_same_rule() {
    // old local clip; kinds = local only; busy callback stars it (set_favorite) and returns false
    // → kept (deleted == 0)
}
```

Update `deletes_old_clips_only_when_on_and_keeps_busy_ones` to set `cleanup_local: true` (its clips are local).

- [ ] **Step 2: Run** `cargo test --manifest-path src-tauri/Cargo.toml cleanup` — Expected: FAIL (`CleanupKinds` missing, candidate list differs).

- [ ] **Step 3: Implement.** SQL drops `favorite = 0 AND server_clip_id IS NULL`. `allows`: `upload_id.is_none() && (!clip.favorite || self.favorites) && if clip.server_clip_id.is_some() { self.uploaded } else { self.local }`. In `clean`, filter candidates by `kinds.allows` and `!busy`, and replace the re-check `match` guard with `Ok(now) if kinds.allows(&now)`. Update the module doc and the `import.rs` module comment only if wording now lies (imported clips are still never removed: leave it).

- [ ] **Step 4: Run** `cargo test --manifest-path src-tauri/Cargo.toml` — Expected: all PASS (including `import::...keeps_it_from_the_cleanup`).

- [ ] **Step 5: Commit** `feat(desktop): the cleanup deletes the kinds of clips chosen in Settings`

### Task 3: Settings UI, TS validation, favorite tooltip

**Files:**
- Modify: `src/lib/api.ts:184-186` (Settings type + doc comment)
- Modify: `src/lib/settings.ts:59` (validation), `src/lib/settings.test.ts` (defaults + test)
- Modify: `src/views/Settings.svelte:896-922` (Cleanup section; also map `cleanupKinds` in the field→section map near line 460, like `cleanupDays: "clips"`)
- Modify: `src/App.svelte:498` → `src/views/Library.svelte` props → `src/components/ClipCard.svelte:268`

**Interfaces:**
- Consumes: JSON keys `cleanupLocal`, `cleanupUploaded`, `cleanupFavorites`, field `cleanupKinds` from Task 1.
- Produces: `Library` prop `cleanupDeletesFavorites: boolean` (App passes `!!settings?.cleanupEnabled && !!settings?.cleanupFavorites`); `ClipCard` prop `cleanupDeletesFavorites?: boolean` (default `false`).

- [ ] **Step 1: Write the failing test** in `settings.test.ts` (add the three keys to `defaults` with the default values first):

```ts
describe("cleanup kinds", () => {
  it("needs at least one kind while cleanup is on", () => {
    const none = { cleanupLocal: false, cleanupUploaded: false, cleanupFavorites: false };
    expect(fields({ cleanupEnabled: true, ...none })).toEqual(["cleanupKinds"]);
    expect(fields({ cleanupEnabled: false, ...none })).toEqual([]);
    expect(fields({ cleanupEnabled: true })).toEqual([]);
  });
});
```

- [ ] **Step 2: Run** `pnpm test settings` — Expected: FAIL.

- [ ] **Step 3: Implement.** Types in `api.ts`; the check in `validateSettings` with the exact message. In `Settings.svelte`, change the switch hint to `Deleted clips can't be recovered.` and, after the days row, add a group labelled `Delete these kinds of clips` with three checkboxes (`bind:checked={draft.cleanupLocal}` etc.) and the hints from Global Constraints, then `{#if fieldErrors.cleanupKinds}<small class="error">…</small>{/if}`. Use the page's existing checkbox/hint styling; plain checkboxes, not switches. Favorite star `title`: `clip.favorite ? (cleanupDeletesFavorites ? "Favorite" : "Favorite: kept by the automatic cleanup") : "Add to favorites"`.

- [ ] **Step 4: Run** `pnpm test && pnpm check` — Expected: all PASS, 0 errors.

- [ ] **Step 5: Commit** `feat(desktop): Settings lets you pick which kinds of clips the cleanup deletes`
