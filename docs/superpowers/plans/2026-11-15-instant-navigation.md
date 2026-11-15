# Instant Navigation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Switching between Clips, Feed and Settings in the desktop app is instant and keeps each screen as it was left.

**Architecture:** `App.svelte` mounts Library, Feed and Settings on first visit and afterwards only hides them, each in its own scroll area. Every screen gets an `active` prop: inactive screens ignore keyboard shortcuts and stop playing media; becoming active refreshes data in the background. Settings' server/system lookups go through a small in-memory cache so they show instantly.

**Tech Stack:** Svelte 5 (runes), TypeScript, Vitest, Tauri 2 (WebKitGTK on Linux).

**Spec:** `docs/superpowers/specs/2026-11-15-instant-navigation-design.md`

## Global Constraints

- No visual changes.
- No new dependencies (no router library).
- The editor is not kept alive: built fresh per clip, as now (`{#key view.clip.id}`).
- The "Discard changes?" guard keeps working for the editor and Settings.
- Logout (or session expiry) drops all kept screens and clears the cache: the next account must not see this one's data.
- All commands run from `desktop/`. Verification: `npm test` and `npm run check` both clean.

## Review Focus

1. Keyboard shortcuts on a hidden screen (Ctrl+A / Escape in Settings or the editor reaching the hidden Library): must do nothing. → Task 2, Step 3 guard + Task 5 check.
2. Logging out and logging in as another account: no clips, uploads, feed or "Your clips on the server" from the first account. → Task 1 test `forgetCache` + Task 2 Step 2 + Task 5 check.
3. Discarding Settings changes, leaving, and coming back: fields show the saved values, not the discarded draft. → Task 4 Step 3 + Task 5 check.
4. Saving an edit in the editor, then going back to Clips: the card reflects the new trim/title without a manual refresh. → Task 2 Step 3 (refresh on show) + Task 5 check.
5. A clip playing in Library's or Feed's player when the screen is left (e.g. via an event that switches screens): the sound stops. → Task 2 Step 3, Task 3 Step 2.

---

### Task 1: In-memory cache for lookups

**Files:**
- Create: `desktop/src/lib/cache.ts`
- Test: `desktop/src/lib/cache.test.ts`

**Interfaces:**
- Produces:
  - `cached<T>(key: string): T | undefined` — the last value stored under `key`.
  - `refresh<T>(key: string, load: () => Promise<T>): Promise<T>` — runs `load`, stores the result under `key`, returns it. Concurrent calls for the same key share one in-flight promise. A rejection leaves the previous value in place and rejects.
  - `forgetCache(): void` — clears every key and drops in-flight results (a load that finishes after `forgetCache` must not store its value).

- [ ] **Step 1: Write the failing tests** in `cache.test.ts`:
  - `cached returns undefined before any refresh`
  - `refresh stores and returns the loaded value` — `await refresh("a", async () => 1)` is `1`; `cached("a")` is `1`.
  - `concurrent refreshes share one load` — two `refresh("a", load)` calls before resolving; `load` called once; both resolve to the same value.
  - `a failed refresh keeps the old value` — store `1`, then `refresh("a", () => Promise.reject(new Error("x")))` rejects; `cached("a")` is still `1`.
  - `forgetCache clears values and ignores loads still running` — start a refresh with a deferred promise, call `forgetCache()`, resolve it; `cached("a")` is `undefined`.
  - `beforeEach(forgetCache)`.
- [ ] **Step 2: Run** `npx vitest run src/lib/cache.test.ts` — expected: FAIL (module not found).
- [ ] **Step 3: Implement** `cache.ts` with two `Map`s (values, in-flight) and a generation counter bumped by `forgetCache` to discard late results.
- [ ] **Step 4: Run** `npx vitest run src/lib/cache.test.ts` — expected: 5 passed.
- [ ] **Step 5: Commit** `feat(desktop): small cache for lookups the screens show`

### Task 2: App keeps screens alive; Library uses it

**Files:**
- Modify: `desktop/src/App.svelte` (view rendering ~L321–358, `go`/`discardAndLeave`/logout ~L120–170, `main` style ~L688)
- Modify: `desktop/src/views/Library.svelte` (props ~L26, memory/scroll effects ~L136–158, load effect ~L190–214, `onkeydown` ~L120)
- Delete: `desktop/src/lib/libraryMemory.ts`

**Interfaces:**
- Consumes: `forgetCache()` from Task 1.
- Produces: views accept `active?: boolean` (default `true`); App passes `active={view.name === "<name>"}`. App state `visited: Set<"library" | "feed" | "settings">` and `settingsEpoch: number` (used by Task 4).

- [ ] **Step 1: Replace the single `<main>` with one scroll area per screen.** In `App.svelte`, render each of Library / Feed / Settings inside its own `<div class="screen" hidden={view.name !== name}>` once `visited` has that name (add the name in `go` and for the initial `library`). The editor renders in its own `.screen` when `view.name === "editor"` (Library is then hidden, not destroyed). Settings is wrapped in `{#key settingsEpoch}`. Move the old `main` styles to `.screen` and add `.screen[hidden] { display: none; }` so the `display: flex` rule doesn't override `hidden`.
- [ ] **Step 2: Logout drops everything.** Replace the `$effect` that calls `forgetLibrary()` with one that, when `account` becomes null, resets `visited` to `new Set(["library"])`, sets `view = { name: "library" }`, and calls `forgetCache()`. (Views are already unmounted because they sit under `{:else}` of `!account`.)
- [ ] **Step 3: Library takes `active`.**
  - Delete `libraryMemory` usage: initial state uses the old `fresh()` defaults directly; remove the two memory-sync effects and the scroll-restore effect (the per-screen scroll area keeps scroll now). Delete `src/lib/libraryMemory.ts`.
  - Move `load()` out of the listener `$effect` into `$effect(() => { if (active) untrack(load); })` — loads on mount and every time Clips is shown again, without clearing what's shown.
  - `onkeydown`: return early when `!active`.
  - `$effect(() => { if (!active) { playing = null; viewing = null; } })`.
- [ ] **Step 4: Verify** `npm run check` — 0 errors; `npm test` — all pass; `grep -rn libraryMemory src` — no matches.
- [ ] **Step 5: Commit** `feat(desktop): screens stay alive between tab switches`

### Task 3: Feed refreshes only when stale

**Files:**
- Modify: `desktop/src/views/Feed.svelte` (props ~L14, `load` ~L59, `onMount(refresh)` ~L125)

**Interfaces:**
- Consumes: `active` prop from Task 2.

- [ ] **Step 1: Replace `onMount(refresh)`** with `$effect(() => { if (active) untrack(() => { if (Date.now() - loadedAt > 30_000) refresh(); }); })`, where `let loadedAt = 0` is set to `Date.now()` after a successful non-`more` load. The existing refresh is already silent when clips are shown (only the Refresh icon spins) — keep it that way.
- [ ] **Step 2:** `$effect(() => { if (!active) playing = null; })`.
- [ ] **Step 3: Verify** `npm run check` — 0 errors.
- [ ] **Step 4: Commit** `feat(desktop): the feed shows at once and refreshes when stale`

### Task 4: Settings shows lookups at once; discard resets it

**Files:**
- Modify: `desktop/src/views/Settings.svelte` (`onMount` ~L147, `{#await}` blocks at ~L552, ~L672, ~L746, ~L762)
- Modify: `desktop/src/App.svelte` (`discardAndLeave`)

**Interfaces:**
- Consumes: `cached`, `refresh` (Task 1); `active` prop and `settingsEpoch` (Task 2).
- Cache keys: `"appInfo"`, `"folderSize"`, `"storage"`, `"displays"`, `"mics"`.

- [ ] **Step 1: Replace the four `{#await}` blocks and the `onMount` fetches** with `$state` initialised from `cached(key)` and filled by `refresh(key, api.<call>)`. Errors keep today's wording ("Can't measure the folder.", "Can't reach the server", the `displaysFailed`/`micsFailed` help text) and only show when there is no cached value. While no value is cached yet the same placeholders as today show ("Measuring the folder…", "…").
- [ ] **Step 2: Refresh on show:** `$effect(() => { if (active) untrack(refreshAll); })` where `refreshAll` refreshes all five keys. `appInfo` only needs loading once (skip when cached).
- [ ] **Step 3: Discard resets.** In `App.svelte`'s `discardAndLeave`, when the current view is `settings`, bump `settingsEpoch` so the `{#key}` rebuilds Settings from the saved `settings` (its draft is `{ ...settings }` on creation).
- [ ] **Step 4: Verify** `npm run check` — 0 errors; `npm test` — all pass.
- [ ] **Step 5: Commit** `feat(desktop): settings show at once; discarding resets them`

### Task 5: Check it in the running app

- [ ] **Step 1:** Launch the app (use the `run` skill / `npm run tauri dev`) and check each, noting pass/fail:
  - Clips → Feed → Settings → Clips: no spinner or empty flash after the first visit; Clips scroll position and filters kept.
  - In Settings, press Ctrl+A in an empty spot, then go to Clips: nothing is selected.
  - Change a setting, go to Feed, choose Discard, go back to Settings: the saved value shows.
  - Open a clip in the editor, change the trim, save, go back: the card shows the new length.
  - Log out, log in: the Clips and Feed screens start fresh.
- [ ] **Step 2:** Fix anything that fails in the task that owns it, re-run `npm test` and `npm run check`, commit.
