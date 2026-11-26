# Clip Player Rework Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the in-app clip player start fast and by itself, give it Klipt's own overlay controls with fullscreen and a favorite star, and show "Link only" on My clips cards.

**Architecture:** Server clips play through the desktop's local media server, which downloads each opened clip once in a single stream into a size-capped cache and answers WebKit's ~1 MB range requests from that file as it fills. `ClipPlayer.svelte` drops the native `controls` for a new `PlayerBar.svelte` overlay, starts playback from code, and toggles Tauri window fullscreen. The parents (Feed, My clips) own the favorite state and pass it in.

**Tech Stack:** Rust (Tauri 2, reqwest 0.13 async + `stream`, std threads in `media_server.rs`), Svelte 5 runes, TypeScript, Vitest, `cargo test`.

**Spec:** `docs/superpowers/specs/2026-11-26-clip-player-design.md`

## Global Constraints

- Clips that aren't uploaded keep opening the editor when clicked; the player is never opened for local-only clips.
- No local-file playback in the player; every clip streams the server copy (through the cache).
- Nothing is fetched before a clip is opened: no preloading on hover or on list load.
- No autoplaying the next clip; "Up next" stays as it is.
- Cache: in the app's cache dir (`app_cache_dir()/player`), at most 1 GB, oldest dropped first, keyed by the full media URL (it is versioned, `?v=` / `.v1.mp4`).
- If the cache path fails, the player falls back to `clip.videoUrl`.
- Bar hides 2 s after the mouse last moved, only while playing; shown while paused, seeking, or while a bar control has focus.
- Speeds: 0.5×, 1×, 1.5×, 2×.
- Keys: Space play/pause · M mute · J / L −5 s / +5 s · F fullscreen · ← / → previous / next clip (unchanged) · Esc leaves fullscreen, else closes. Keys wait while "Add to album" is open.
- Screenshots (`kind === "screenshot"`) keep the image with no bar.
- Never run prettier (no config in the repo; it reformats whole files).
- Linux is the tested platform; Windows (WebView2) must still compile and fall back gracefully.

## Review Focus

1. Closing the player (or switching clips with ← / →) mid-download: the download keeps going for the cache, but the media connection thread must not hang forever waiting for bytes; a dropped client ends its thread. Test in Task 2.
2. The server answers 404/500 or the network drops mid-file: waiting range requests must end (error), the partial file must not stay in the cache as if complete, and the next open retries. Test in Task 1.
3. A request for a range beyond what's downloaded so far (WebKit seeks to the end for the `moov`, or the user seeks ahead): it is served as soon as those bytes arrive, not refused. Test in Task 1 (`wait_for`), Task 2.
4. Closing the player while in fullscreen leaves fullscreen, so the app isn't stuck fullscreen on the clip list. Covered in Task 5 Step 3.
5. `play()` refused or aborted (clip switched quickly): the bar shows paused with a play button, no unhandled promise rejection. Covered in Task 5 Step 3.

---

### Task 1: Read-ahead download cache (`player_cache.rs`)

**Files:**
- Create: `desktop/src-tauri/src/player_cache.rs`
- Modify: `desktop/src-tauri/src/lib.rs` (add `pub mod player_cache;`)

**Interfaces:**
- Produces:
  - `pub struct PlayerCache` (Clone, `Arc` inside) with `pub fn new(dir: PathBuf, max_bytes: u64) -> PlayerCache`
  - `pub const PLAYER_CACHE_MAX_BYTES: u64 = 1024 * 1024 * 1024;`
  - `pub fn open(&self, url: &str) -> Arc<Download>`: returns the running or finished download for `url`, starting one if there is none (or the previous one failed). A finished one is a file on disk named `<sha-or-hash of url>.mp4`; an in-progress one is `<hash>.part`, renamed on completion.
  - `pub struct Download` with `pub fn wait_total(&self, timeout: Duration) -> io::Result<u64>` (the `Content-Length`; error if the request failed or timed out), `pub fn wait_for(&self, end_exclusive: u64, timeout: Duration) -> io::Result<()>` (blocks until that many bytes are on disk, or error on failure/timeout), and `pub fn path(&self) -> PathBuf` (the file to read from, `.part` or final).
  - `pub fn evict(&self)`: deletes finished cache files, oldest mtime first, until the total is ≤ `max_bytes`; never deletes a file whose download is in the map and not finished.

- [ ] **Step 1: Write the failing tests** (in `player_cache.rs` `#[cfg(test)]`). Use a tiny `std::net::TcpListener` test server that serves a 3 MB body (`(0..len).map(|i| (i % 251) as u8)`), with `Content-Length`, in 64 KB writes with a 5 ms sleep between them, counting the requests it gets.
  - `downloads_once_and_serves_ranges_as_they_arrive`: `open(url)` twice returns the same `Arc` (`Arc::ptr_eq`); `wait_total` = 3 MB; `wait_for(3 MB)` then reading `path()` gives exactly the body; the server saw **1** request.
  - `wait_for_ahead_of_the_download_blocks_until_bytes_arrive`: right after `open`, `wait_for(2_000_000, 10 s)` returns Ok and the file is ≥ 2,000,000 bytes.
  - `a_failed_download_errors_waiters_and_retries_next_open`: the server answers `500` once, then the body; `wait_total` errs; no `.mp4` is left in the dir; a second `open` starts a new download that succeeds (server saw 2 requests).
  - `a_cut_off_download_is_not_kept_as_finished`: the server sends half the body and closes; `wait_for(total)` errs; no final `.mp4` exists.
  - `evict_drops_oldest_finished_files_first`: three finished 400 KB files with mtimes t1 < t2 < t3, `max_bytes` = 900 KB → only the t1 file is deleted.
  - `a_finished_file_survives_a_restart`: after one completed download, a **new** `PlayerCache` on the same dir serves `open(url)` from disk with the server seeing no new request.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd desktop/src-tauri && cargo test player_cache`
Expected: FAIL (module/functions missing).

- [ ] **Step 3: Implement `player_cache.rs`.** State per download behind a `Mutex` + `Condvar` (`total: Option<u64>`, `written: u64`, `state: Running | Done | Failed`). The download runs on `tauri::async_runtime::spawn` with a `reqwest::Client` (connect timeout 15 s, no overall timeout) streaming `bytes_stream()` into the `.part` file, updating `written` and `notify_all` after each chunk; on success fsync + rename to `.mp4`, then `evict()`; on any error or a short body set `Failed`, delete the `.part`, and remove it from the map so the next `open` retries. Reuse the rustls `ring` provider install line from `upload/api.rs::with_timeout`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd desktop/src-tauri && cargo test player_cache`
Expected: PASS (6 tests).

- [ ] **Step 5: Commit**

```bash
git add desktop/src-tauri/src/player_cache.rs desktop/src-tauri/src/lib.rs
git commit -m "feat(desktop): read-ahead cache for server clips (one download per clip, ranges served as it fills)"
```

### Task 2: Media server serves cached server clips

**Files:**
- Modify: `desktop/src-tauri/src/media_server.rs` (`Served`, `register`, `handle`, tests)
- Modify: `desktop/src-tauri/src/lib.rs:86` (start the media server with a `PlayerCache`; run `evict()` once at startup)

**Interfaces:**
- Consumes: `PlayerCache::{new, open, evict}`, `Download::{wait_total, wait_for, path}` (Task 1).
- Produces: `MediaServer::start(cache: PlayerCache) -> io::Result<MediaServer>` and `pub fn url_for_remote(&self, url: &str) -> String` (same URL → same token, like `url_for`).

- [ ] **Step 1: Write the failing tests** (in `media_server.rs` tests, reusing Task 1's style of slow test server):
  - `serves_a_remote_clip_through_the_cache_with_ranges`: `url_for_remote(url)` twice gives the same URL; a `Range: bytes=1000000-1999999` request returns `206`, `Content-Range: bytes 1000000-1999999/3000000` and exactly those bytes; a second ranged request for `0-99` returns the first 100 bytes; the upstream saw 1 request.
  - `a_remote_clip_that_fails_upstream_answers_502`: the upstream answers 500 → the media server responds `502 Bad Gateway`.
  - `a_client_that_goes_away_mid_wait_ends_its_thread`: request a range near the end of a slow download and close the socket right away; within 15 s the server's open-connection count is back to 0 (expose the counter to tests via a `#[cfg(test)]` accessor).

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd desktop/src-tauri && cargo test media_server`
Expected: FAIL.

- [ ] **Step 3: Implement.** `Served` becomes an enum: `File { path, video_only }` (today's behaviour) and `Remote { url }`. For `Remote`: `wait_total(30 s)` (error → `502`), parse the range against it, write the head, then copy in 256 KB pieces: `wait_for(piece_end, 30 s)` → read that piece from `path()` → write. A write error ends the loop (client gone). Keep `Connection: close` and the CORS rule unchanged.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd desktop/src-tauri && cargo test media_server`
Expected: PASS (old and new tests).

- [ ] **Step 5: Commit**

```bash
git add desktop/src-tauri/src/media_server.rs desktop/src-tauri/src/lib.rs
git commit -m "feat(desktop): media server plays server clips from the read-ahead cache"
```

### Task 3: `player_video_url` command

**Files:**
- Modify: `desktop/src-tauri/src/commands.rs` (new command), `desktop/src-tauri/src/lib.rs` (register it), `desktop/src/lib/api.ts` (binding)

**Interfaces:**
- Consumes: `MediaServer::url_for_remote` (Task 2); the saved `Auth.server_url` (`upload/auth.rs`).
- Produces: Tauri command `player_video_url(url: String) -> Result<String>`; TS `api.playerVideoUrl(url: string): Promise<string>`.

- [ ] **Step 1: Write the failing test** for a pure helper `fn is_server_media(server_url: &str, url: &str) -> bool` in `commands.rs` tests: `true` for `https://klipt.example/media/abc.v1.mp4` with server `https://klipt.example`; `false` for another host, another scheme, another port, and for `https://klipt.example/api/me`.

- [ ] **Step 2: Run it to verify it fails**

Run: `cd desktop/src-tauri && cargo test is_server_media`
Expected: FAIL.

- [ ] **Step 3: Implement** `is_server_media` (same origin via `reqwest::Url`, path starts with `/media/`) and the command: no saved login or not server media → `AppError::InvalidInput`; else `Ok(media_server.url_for_remote(&url))`. Add `playerVideoUrl: (url: string) => invoke<string>("player_video_url", { url })` to `api.ts`.

- [ ] **Step 4: Run the tests and the type check**

Run: `cd desktop/src-tauri && cargo test is_server_media && cd .. && pnpm exec svelte-check --threshold error`
Expected: PASS; `0 ERRORS`.

- [ ] **Step 5: Commit**

```bash
git add desktop/src-tauri/src/commands.rs desktop/src-tauri/src/lib.rs desktop/src/lib/api.ts
git commit -m "feat(desktop): player_video_url gives a cached local URL for a server clip"
```

### Task 4: Player bar helpers and `PlayerBar.svelte`

**Files:**
- Create: `desktop/src/lib/player.ts`, `desktop/src/lib/player.test.ts`, `desktop/src/components/PlayerBar.svelte`

**Interfaces:**
- Produces (`lib/player.ts`):
  - `export const SPEEDS = [0.5, 1, 1.5, 2] as const;` and `export function nextSpeed(current: number): number` (cycles; an unknown value → 1)
  - `export function clockLabel(seconds: number): string` (`"0:11"`, `"1:05"`, `"1:02:03"`; NaN/negative → `"0:00"`)
  - `export type PlayerAction = "toggle" | "mute" | "back" | "forward" | "fullscreen" | "escape";` and `export function playerKey(e: { key: string; ctrlKey?: boolean; altKey?: boolean; metaKey?: boolean }): PlayerAction | null` (Space/k → toggle, m → mute, j → back, l → forward, f → fullscreen, Escape → escape; case-insensitive; any modifier → null; arrows → null, left to `navKey`)
  - `export interface PlayerPrefs { volume: number; muted: boolean }`, `export function loadPlayerPrefs(): PlayerPrefs` (default `{ volume: 1, muted: false }`, clamps volume to 0–1, survives a throwing or empty `localStorage`) and `export function savePlayerPrefs(p: PlayerPrefs): void` (key `klipt.player`, errors swallowed)
- Produces (`PlayerBar.svelte`): props `{ video: HTMLVideoElement; fullscreen: boolean; visible: boolean; ontogglefullscreen: () => void; onfocuschange: (focused: boolean) => void }`. It reads/sets `video` state itself (play/pause, `currentTime`, `buffered`, `volume`, `muted`, `playbackRate`) and saves prefs on volume/mute change.

- [ ] **Step 1: Write the failing tests** in `player.test.ts`: `nextSpeed(0.5) === 1`, `nextSpeed(2) === 0.5`, `nextSpeed(1.25) === 1`; `clockLabel(11) === "0:11"`, `clockLabel(65) === "1:05"`, `clockLabel(3723) === "1:02:03"`, `clockLabel(NaN) === "0:00"`; `playerKey({ key: " " }) === "toggle"`, `playerKey({ key: "F" }) === "fullscreen"`, `playerKey({ key: "j" }) === "back"`, `playerKey({ key: "f", ctrlKey: true }) === null`, `playerKey({ key: "ArrowLeft" }) === null`; `loadPlayerPrefs()` returns the default when `localStorage.getItem` throws, and `{ volume: 1, muted: true }` after saving `{ volume: 7, muted: true }`.

- [ ] **Step 2: Run them to verify they fail**

Run: `cd desktop && pnpm exec vitest run src/lib/player.test.ts`
Expected: FAIL.

- [ ] **Step 3: Implement `lib/player.ts`, then `PlayerBar.svelte`** to mockup A (`.superpowers/brainstorm/60904-1795712869/content/player-bar.html`): a dark gradient fade at the bottom of the video; seek bar (played = `--accent`, buffered lighter, knob on hover/drag, pointer capture for dragging, click to seek); row: play/pause, volume icon (click = mute) + slider, `clockLabel(current) / clockLabel(duration)`, spacer, speed button (`nextSpeed`, label `1×`), fullscreen button (icon switches when `fullscreen`). Hidden state = `opacity: 0` + `pointer-events: none`, 200 ms fade. Use existing `Icon` names where they exist; add missing icons to `Icon.svelte`.

- [ ] **Step 4: Run the tests and the type check**

Run: `cd desktop && pnpm exec vitest run src/lib/player.test.ts && pnpm exec svelte-check --threshold error`
Expected: PASS; `0 ERRORS`.

- [ ] **Step 5: Commit**

```bash
git add desktop/src/lib/player.ts desktop/src/lib/player.test.ts desktop/src/components/PlayerBar.svelte desktop/src/components/Icon.svelte
git commit -m "feat(desktop): Klipt's own player bar (seek, volume, speed, fullscreen button)"
```

### Task 5: Rework `ClipPlayer.svelte` (bar, fullscreen, start on open, cached source, header)

**Files:**
- Modify: `desktop/src/components/ClipPlayer.svelte`
- Modify: `desktop/src-tauri/capabilities/default.json` (add `"core:window:allow-set-fullscreen"`, `"core:window:allow-is-fullscreen"`)

**Interfaces:**
- Consumes: `PlayerBar` props, `playerKey`, `loadPlayerPrefs` (Task 4); `api.playerVideoUrl` (Task 3); `navKey` (existing).
- Produces: new `ClipPlayer` props `favorite?: boolean` and `onfavorite?: () => void` (star hidden when `onfavorite` is missing).

- [ ] **Step 1: Source.** On each `clip.id`, set `src` to `await api.playerVideoUrl(clip.videoUrl)`, falling back to `clip.videoUrl` when it rejects; on the video's first `error` with a cached `src`, switch once to `clip.videoUrl`. Screenshots keep `clip.videoUrl` on the `<img>`.

- [ ] **Step 2: Start on open.** Remove `controls` and `autoplay`; apply `loadPlayerPrefs()` to the video; on `loadedmetadata` call `video.play()` and swallow `AbortError`/`NotAllowedError` (the bar then shows paused). Clicking the video toggles play/pause; double-click toggles fullscreen.

- [ ] **Step 3: Fullscreen and keys.** `fullscreen` state; toggling calls `getCurrentWindow().setFullscreen(on)` (`@tauri-apps/api/window`) and adds a `.fullscreen` class that makes the player fill the window with only the stage and bar (no header, no backdrop padding). Remember whether the window was fullscreen before (`isFullscreen()`) and restore that on leave. `onclose` wrapper: leave fullscreen first, then close. Key handling: keep the `albumOpen` guard and `navKey` arrows; `playerKey` → toggle / mute / ±5 s (clamped to 0..duration) / fullscreen; `escape` leaves fullscreen if on, else closes. Ignore keys while focus is in a text field. Bar visibility: a 2 s timer reset on `pointermove` over the stage; visible when paused, seeking or `onfocuschange(true)`; hide the cursor (`cursor: none`) on the stage while the bar is hidden.

- [ ] **Step 4: Header.** Tools become: prev/next (unchanged), ★ (`favorite` filled/empty, `aria-pressed`, calls `onfavorite`), Copy link, Album, a ⋯ menu button with one item "Open on web" (closes on outside click and Esc, like `ClipCard`'s menu), close.

- [ ] **Step 5: Type check and existing tests**

Run: `cd desktop && pnpm exec svelte-check --threshold error && pnpm exec vitest run`
Expected: `0 ERRORS`; all tests pass.

- [ ] **Step 6: Commit**

```bash
git add desktop/src/components/ClipPlayer.svelte desktop/src-tauri/capabilities/default.json
git commit -m "feat(desktop): clip player starts by itself, own controls, fullscreen (F, double-click), cached source, favorite in the header"
```

### Task 6: Favorite in the player from the Feed and My clips

**Files:**
- Modify: `desktop/src/views/Feed.svelte` (the `<ClipPlayer>` at ~line 396), `desktop/src/views/Library.svelte` (the `<ClipPlayer>` at ~line 856, `playing` state)

**Interfaces:**
- Consumes: `ClipPlayer` props `favorite`, `onfavorite` (Task 5); existing `toggleFavorite` (Feed), `toggleFavorite(clip: ClipView)` and `toggleOnlineFavorite(upload: MyUpload)` (Library).

- [ ] **Step 1: Feed.** Pass the playing clip's `favorite` from the `clips` list (so it follows the card) and `onfavorite={() => toggleFavorite(clip)}`; omit `onfavorite` when `clip.favorite === undefined` (server without favorites).

- [ ] **Step 2: My clips.** Track what was opened: `playing` becomes `{ upload: MyUpload; local: ClipView | null } | null`. From a local card (uploaded, on this PC) the star shows and toggles the **local** clip's favorite via `toggleFavorite(local)` (the library syncs the server copy, as today); from an online card it uses `toggleOnlineFavorite(upload)`. Read the current value from `local`/`uploads` state so card and player stay in step.

- [ ] **Step 3: Type check and tests**

Run: `cd desktop && pnpm exec svelte-check --threshold error && pnpm exec vitest run`
Expected: `0 ERRORS`; all pass.

- [ ] **Step 4: Commit**

```bash
git add desktop/src/views/Feed.svelte desktop/src/views/Library.svelte
git commit -m "feat(desktop): favorite star in the clip player follows the card (Feed and My clips)"
```

### Task 7: "Link only" label on My clips cards

**Files:**
- Modify: `desktop/src/components/ClipCard.svelte`, `desktop/src/components/OnlineClipCard.svelte`, `desktop/src/components/clip-card.css`

**Interfaces:**
- Consumes: existing `onvisibility` props (Library's `toggleLocalVisibility` / `toggleVisibility`, which already toast "Now in the feed.").

- [ ] **Step 1: Implement.** On the thumbnail (top-left, under the selection check box's spacing), a small pill "Link only" for uploaded clips with `visibility === "link"` (local: `clip.serverClipId && clip.visibility === "link"`; online: `upload.visibility === "link"`). It's a `<button>` with `title="Only people with the link can see it. Click to show it in the feed"` that calls `onvisibility` and stops the click from opening the clip; disabled while `busy`. Feed clips show nothing. Remove the old "· Link only" text from `OnlineClipCard`'s meta line so it isn't shown twice.

- [ ] **Step 2: Type check and tests**

Run: `cd desktop && pnpm exec svelte-check --threshold error && pnpm exec vitest run`
Expected: `0 ERRORS`; all pass.

- [ ] **Step 3: Commit**

```bash
git add desktop/src/components/ClipCard.svelte desktop/src/components/OnlineClipCard.svelte desktop/src/components/clip-card.css
git commit -m "feat(desktop): \"Link only\" label on My clips cards (click puts it in the feed)"
```

### Task 8: Roadmap

**Files:**
- Modify: `docs/ROADMAP.md`

- [ ] **Step 1:** Add a "Where Klipt is today" row dated the day it lands: "Clip player rework": own overlay bar (seek, volume, speed, fullscreen via button, F or double-click), starts by itself, server clips through a 1 GB read-ahead cache (first open ~1–2 s instead of 2–8 s, re-opens instant), ★ in the header, "Link only" label on My clips cards. Also add the two missing earlier items (albums picked at upload `80a9d6c`, "All games" count `7adafa0`). Do not add redeploy or "test in the app" notes.

- [ ] **Step 2: Commit**

```bash
git add docs/ROADMAP.md
git commit -m "docs: roadmap: clip player rework"
```
