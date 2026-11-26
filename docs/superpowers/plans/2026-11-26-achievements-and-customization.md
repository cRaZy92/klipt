# Achievements, profile pictures and customization Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Profile pictures and a free set of accent colours for everyone; achievements earned automatically (server + desktop activity, retroactive, no toast flood) that unlock accents, themes, avatar frames and profile banners, shown in the app and on the website.

**Architecture:** One catalog in `packages/shared` (cosmetics, achievement definitions, a pure `earnedIds()` evaluator, token maths) used by the server, the website and the desktop UI. The server stores pictures, equipped looks, reported desktop activity and earned achievements, and evaluates after every change and once at startup. The desktop computes its activity **from `library.db`** (no separate counters: counts, edit columns and save times are already there; a new `origin` column marks combined/part/downloaded clips) and reports it, debounced, with `backfill: true` the first time per account.

**Tech Stack:** Fastify + drizzle (Postgres, pglite in tests) + kitajs JSX + vanilla `public/app.js`; Tauri 2 (Rust, rusqlite) + Svelte 5; vitest; ffmpeg.

**Spec:** `docs/superpowers/specs/2026-11-26-achievements-and-customization-design.md`

## Global Constraints

- Basics are free: a profile picture, the "Klipt" purple + 5 basic accents, the "Klipt" theme. Never gate these.
- Achievements are never removed once earned (deleting clips or edits changes nothing).
- Retroactive awards (startup run, desktop `backfill: true`) are stored `quiet`; the app shows **one** toast for all unseen quiet ones: `You've already earned N achievements` (N=1: `You've already earned an achievement`), which opens Profile. Unseen loud ones get one toast each: title `Achievement unlocked: <name>`, body `<description>` plus ` · unlocked <reward name>` when it has a reward.
- A notice counts as seen once shown (toast or system notification); no reminders, no progress popups.
- Accent/theme change only what **you** see; frame/banner are what **others** see. Same everywhere (app and website read the same server state).
- Server refuses equipping a locked cosmetic: `403 {error: 'not_unlocked'}`; unknown id: `400 invalid_request`.
- Avatar: `PUT /api/me/avatar`, raw JPEG/PNG/WebP body ≤ 10 MB, server writes a 256×256 JPEG to `<data>/avatars/<userId>.jpg`, bumps `users.avatar_version`; served at `/avatars/<userId>.jpg?v=<n>` with `cache-control: public, max-age=31536000, immutable`. `DELETE /api/me/avatar` goes back to the initial.
- Frames draw only where the avatar is ≥ 28 px. Frames/banners are CSS only (no image assets). Pulsing frame respects `prefers-reduced-motion`.
- No leaderboards, XP, levels or streaks. Light theme out of scope.
- Every accent passes white-text contrast ≥ 4.5 (buttons) and every theme passes `--text` on `--bg` ≥ 7 and `--text-muted` on `--surface` ≥ 4.5 (tested).
- "Active user" for *Seen by everyone* = not disabled and any session used in the last 30 days.

## Review Focus

1. **A picture the server can't read** (a renamed PDF, a 0-byte body, a GIF): 415 for a non-allowed content type, 422 `not_an_image` for undecodable bytes, and the old picture stays. → Task 2 tests.
2. **Stored looks that no longer make sense** (a cosmetic id removed from the catalog, garbage JSON in `users.looks`): reads fall back per field to the default, nothing crashes. → `sanitizeLooks` test in Task 1, Task 3 test.
3. **Two PCs, or a PC that deleted clips, reporting lower counters**: the stored value never goes down; firsts are a union. → Task 3 test.
4. **The startup run repeated on every restart**: no duplicate rows, nothing turns loud, earned_at of existing rows unchanged. → Task 3 test.
5. **The picture URL can't load** (server down, app offline, file removed): every Avatar falls back to the initial. → Task 5/6 (Avatar `onerror` / `<img onerror>`), checked in Task 6 test of `avatarState`.

---

### Task 1: Shared catalog: looks, achievements, evaluator, API types

**Files:**
- Create: `packages/shared/src/looks.ts`, `packages/shared/src/achievements.ts`
- Modify: `packages/shared/src/index.ts` (re-export both), `packages/shared/src/api.ts`
- Test: `server/test/catalog.test.ts` (shared has no test runner; the server's vitest imports `@klipt/shared`)

**Interfaces (Produces):**

`looks.ts`
```ts
export type ThemeTokens = Record<'--bg'|'--bg-2'|'--surface'|'--surface-2'|'--surface-3'|'--border'|'--border-strong'|'--text'|'--text-muted'|'--text-faint', string>;
export interface AccentDef { id: string; name: string; color: string /* #rrggbb */; free?: true }
export interface ThemeDef  { id: string; name: string; tokens: ThemeTokens; free?: true }
export interface FrameDef  { id: string; name: string }
export interface BannerDef { id: string; name: string }
export const ACCENTS: AccentDef[];   // free: klipt #7c5cff, ocean, teal, forest, sunset, rose; locked: gold, crimson, mint, ice
export const THEMES: ThemeDef[];     // free: klipt (today's app.css values); locked: midnight, oled, ember
export const FRAMES: FrameDef[];     // bronze, silver, gold, rainbow, pulse (none free)
export const BANNERS: BannerDef[];   // curator, editor, squad (none free)
export type CosmeticId = `accent:${string}` | `theme:${string}` | `frame:${string}` | `banner:${string}`;
export interface Looks { accent: string; theme: string; frame: string | null; banner: string | null }
export const DEFAULT_LOOKS: Looks;   // { accent: 'klipt', theme: 'klipt', frame: null, banner: null }
export function sanitizeLooks(raw: unknown): Looks;                 // per field: unknown/invalid → default
export function accentTokens(color: string): Record<string, string>; // --accent, --accent-hover (mix 15% white), --accent-text (mix 55% white), --accent-soft (α .16), --accent-softer (α .08), --accent-glow (α .45)
export function looksTokens(looks: Looks): Record<string, string>;   // theme tokens + accent tokens
export function cosmeticName(id: CosmeticId): string | null;
export function contrastRatio(a: string, b: string): number;         // WCAG relative luminance
```
For `--accent-text`, the mix must keep contrast ≥ 4.5 on `--surface`. For klipt purple it must give values close to today's (`#9076ff`, `#c4b6ff`), so the default look doesn't change.

`achievements.ts`
```ts
export const ACTIVITY_COUNTERS = ['clipsSaved','screenshots','recordedMs','recordings','imports','games'] as const;
export const ACTIVITY_FIRSTS = ['cut','zoom','subtitle','blur','mutePart','combine','part','thumbnailImage','nightOwl','marathon'] as const;
export type ActivityCounter = typeof ACTIVITY_COUNTERS[number];
export type ActivityFirst = typeof ACTIVITY_FIRSTS[number];
export type ServerStat = 'uploads'|'anyUploads'|'totalViews'|'taggedIn'|'tagsGiven'|'maxAlbumClips'|'seenByEveryone'|'blinkUploads'|'newYearUploads'|'yearOld';
export type StatKey = ServerStat | ActivityCounter;
export type Rule = { stat: StatKey; min: number } | { first: ActivityFirst } | { allOf: string[] };
export interface AchievementDef { id: string; name: string; description: string; icon: string /* one emoji */; category: 'milestones'|'social'|'editor'|'hidden'; hidden?: true; reward?: CosmeticId; rule: Rule }
export const ACHIEVEMENTS: AchievementDef[];
export function achievementById(id: string): AchievementDef | undefined;
export function earnedIds(stats: Partial<Record<StatKey, number>>, firsts: ReadonlySet<string>): string[]; // allOf resolved after the others, in catalog order
export function unlockedCosmetics(earned: Iterable<string>): Set<CosmeticId>; // free ones + rewards of earned
```

Starting set (ids → rule → reward):

| id | rule | reward |
|---|---|---|
| `first-clip` | clipsSaved ≥ 1 | |
| `first-upload` | anyUploads ≥ 1 | |
| `clips-10` / `clips-100` / `clips-500` | uploads ≥ 10/100/500 | frame:bronze / frame:silver / frame:gold |
| `hours-10` | recordedMs ≥ 36 000 000 | theme:ember |
| `games-5` / `games-15` | games ≥ 5 / 15 | accent:gold / accent:crimson |
| `first-recording` / `first-screenshot` / `first-import` | recordings / screenshots / imports ≥ 1 | |
| `views-10` / `views-100` / `views-1000` | totalViews ≥ 10/100/1000 | — / theme:oled / frame:pulse |
| `seen-by-everyone` | seenByEveryone ≥ 1 | frame:rainbow |
| `tagged-10` | taggedIn ≥ 10 | banner:squad |
| `first-tag` | tagsGiven ≥ 1 | |
| `curator` | maxAlbumClips ≥ 10 | banner:curator |
| `first-cut`, `first-zoom`, `first-subtitle`, `first-blur`, `first-mute`, `first-combine`, `first-part`, `first-thumbnail` | first: cut, zoom, subtitle, blur, mutePart, combine, part, thumbnailImage | |
| `editor-all` | allOf the 8 above | banner:editor |
| `night-owl` (hidden) | first: nightOwl | theme:midnight |
| `blink` (hidden) | blinkUploads ≥ 1 | |
| `marathon` (hidden) | first: marathon | accent:mint |
| `new-year` (hidden) | newYearUploads ≥ 1 | |
| `archivist` (hidden) | yearOld ≥ 1 | accent:ice |

Names/descriptions are the builder's copy, short, in the spec's tone (e.g. Night owl: "Saved a clip between 3 and 5 in the morning").

`api.ts` additions/changes:
```ts
export interface UserRef { username: string; displayName: string; avatarUrl: string | null; frame: string | null }
// Clip.owner, AlbumSummary.owner, FeedbackItem.user: UserRef. ClipViewer gains avatarUrl, frame. AdminUser gains avatarUrl.
// Me gains: avatarUrl: string | null; looks: Looks; newAchievements: number (unseen count)
export interface ActivityReport { counters: Partial<Record<ActivityCounter, number>>; firsts: ActivityFirst[]; backfill?: boolean }
export interface EarnedAchievement { id: string; name: string; description: string; icon: string; earnedAt: string; seen: boolean; quiet: boolean; reward: CosmeticId | null; rewardName: string | null }
export interface MyAchievements { earned: EarnedAchievement[] /* newest first */; looks: Looks; unlocked: CosmeticId[] }
export type UpdateLooksRequest = Partial<Looks>;
```

- [ ] **Step 1: Write the failing tests** in `server/test/catalog.test.ts`:
  - `every accent has white-text contrast ≥ 4.5`: `ACCENTS.every(a => contrastRatio(a.color, '#ffffff') >= 4.5)`
  - `every theme is readable`: `contrastRatio(t.tokens['--text'], t.tokens['--bg']) >= 7` and `contrastRatio(t.tokens['--text-muted'], t.tokens['--surface']) >= 4.5`
  - `the free set`: exactly 6 free accents including `klipt`, theme `klipt` free, no free frame/banner; `unlockedCosmetics([])` equals the free set
  - `rewards exist`: every `reward` resolves via `cosmeticName`, every `allOf` id exists, ids unique
  - `earnedIds`: `earnedIds({uploads: 10, anyUploads: 10}, new Set())` contains `clips-10`,`first-upload`, not `clips-100`; with all 8 editor firsts it contains `editor-all`; with 7 it doesn't
  - `sanitizeLooks`: `sanitizeLooks({accent: 'nope', theme: 'midnight', frame: 42, banner: 'curator'})` → `{accent:'klipt', theme:'midnight', frame:null, banner:'curator'}`; `sanitizeLooks(null)` → `DEFAULT_LOOKS`
  - `accentTokens('#7c5cff')['--accent']` is `#7c5cff`; `--accent-soft` is `rgba(124, 92, 255, 0.16)`
- [ ] **Step 2:** `cd server && pnpm vitest run test/catalog.test.ts` → FAIL (module members missing)
- [ ] **Step 3:** Implement `looks.ts`, `achievements.ts`, API types. Note: `sanitizeLooks` only checks ids exist; *unlocked* is enforced by the server on write.
- [ ] **Step 4:** Tests pass; `pnpm -r typecheck` (server) fails only where `owner` now needs `avatarUrl`/`frame`, which is fixed in Task 2. If you'd rather keep main green, land Task 1 together with Task 2.
- [ ] **Step 5:** Commit `feat(shared): looks and achievements catalog`

---

### Task 2: Server: schema, profile pictures, `UserRef` everywhere, looks

**Files:**
- Modify: `server/src/db/schema.ts`; generate migration `0015_achievements_and_looks` (`cd server && pnpm drizzle-kit generate --name achievements_and_looks`, so the snapshot is updated too)
- Create: `server/src/users/ref.ts`, `server/src/users/routes.ts` (avatar + looks routes; register in `app.ts`)
- Modify: `server/src/clips/media.ts` (`makeAvatar`), `server/src/clips/repo.ts` (`ownerExpr`, `listUsernames`, `listViewers`), `server/src/clips/routes.ts` (`toClip`), `server/src/albums/repo.ts`+routes (owner), `server/src/feedback/routes.ts`, `server/src/admin/queries.ts`, `server/src/auth/sessions.ts` (`AuthUser`), `server/src/auth/routes.ts` (`toMe`)
- Test: `server/test/avatar.test.ts`, `server/test/looks.test.ts`

Schema:
- `users`: `avatarVersion integer not null default 0` (0 = no picture), `looks jsonb` (nullable; read through `sanitizeLooks`), `activity jsonb` (nullable; `{counters, firsts}`, used in Task 3)
- `user_achievements(user_id uuid fk cascade, achievement_id text, earned_at timestamptz default now, seen_at timestamptz null, quiet boolean not null default false)`, PK `(user_id, achievement_id)`

**Interfaces (Produces):**
```ts
// users/ref.ts
export const userRefColumns: { id; username; displayName; avatarVersion; frame /* sql looks->>'frame' */ } // spread into selects joined on users
export function avatarUrl(u: { id: string; avatarVersion: number }, baseUrl: string): string | null; // null when 0
export function toUserRef(r: { id: string; username: string; displayName: string; avatarVersion: number; frame: string | null }, baseUrl: string): UserRef; // frame passes through only if it's in FRAMES
// media.ts
export async function makeAvatar(input: string, output: string): Promise<void>; // scale=256:256:force_original_aspect_ratio=increase,crop=256:256, mjpeg
// AuthUser gains: avatarVersion: number; looks: Looks; newAchievements: number  (one subquery in resolveToken: count of the user's unseen user_achievements)
// ClipWithOwner.owner becomes the userRefColumns row shape; toClip maps it with toUserRef
```
Routes (all `requireUser`, all return `Me` from `toMe`, which now adds `avatarUrl`, `looks`, `newAchievements`):
- `PUT /api/me/avatar`: content types `image/jpeg|png|webp` (the parser is already registered for the thumbnail route: share the constant; register once), `bodyLimit` 10 MB. Write the body to `<data>/avatars/.<id>.in.tmp`, `probeImage` → 422 `not_an_image` with no change, `makeAvatar` into `.<id>.jpg.tmp`, rename to `<id>.jpg`, `avatar_version + 1`.
- `DELETE /api/me/avatar`: remove the file, set `avatar_version = 0`.
- `GET /avatars/:file`: `^[0-9a-f-]{36}\.jpg$` else 404; public; immutable cache header; `@fastify/static` decorator `sendFile` with `root` of the avatars dir (follow the clips media pattern: a second `fastifyStatic` registration with `serve: false` and a `decorateReply: false`, using `reply.sendFile(file, avatarsDir)`).
- `PUT /api/me/looks` `UpdateLooksRequest`: zod with each field optional (`frame`/`banner` nullable). Unknown id → 400 `invalid_request`; known but not in `unlockedCosmetics(earned ids)` → 403 `not_unlocked`; null frame/banner is always allowed. Merges into the stored looks.

- [ ] **Step 1: Write the failing tests**
  - avatar: PUT a PNG made with ffmpeg (helpers already run ffmpeg) → 200, `me.avatarUrl` = `http://localhost/avatars/<id>.jpg?v=1`; `GET /avatars/<id>.jpg` → 200, `image/jpeg`, immutable cache-control; ffprobe says 256×256. A second PUT → `?v=2`.
  - avatar: PUT `text/plain` → 415; PUT `image/png` with `not an image` bytes → 422 `not_an_image`, and `avatarUrl` is still `?v=1` from before (Review Focus 1). DELETE → `avatarUrl` null and the file is gone; `GET /avatars/../x` → 404.
  - avatar shows up on others: a clip by alice in `GET /api/clips` has `owner.avatarUrl` set and `owner.frame` null; `GET /api/users` items carry `avatarUrl`.
  - looks: `PUT /api/me/looks {accent:'ocean'}` → 200, `me.looks.accent==='ocean'`; `{theme:'midnight'}` → 403 `not_unlocked`; `{accent:'nope'}` → 400; after inserting a `user_achievements` row for `night-owl` → `{theme:'midnight'}` 200. `{frame:'bronze'}` after `clips-10` → 200 and alice's clips show `owner.frame==='bronze'`.
  - looks: raw `update users set looks = '{"accent":"removed","theme":7}'` → `GET /api/me` looks equal `DEFAULT_LOOKS` (Review Focus 2).
- [ ] **Step 2:** Run `pnpm vitest run test/avatar.test.ts test/looks.test.ts` → FAIL
- [ ] **Step 3:** Implement schema + migration, ref helpers, routes, owner mapping. Update the existing tests that `toEqual` an exact `owner`/`Me` object (`grep -rn "owner: {" server/test`) to include the new fields.
- [ ] **Step 4:** `pnpm test && pnpm typecheck` in `server/` → all green
- [ ] **Step 5:** Commit `feat(server): profile pictures, looks, avatars in every user payload`

---

### Task 3: Server: achievements engine, activity, triggers, startup run

**Files:**
- Create: `server/src/achievements/stats.ts`, `server/src/achievements/engine.ts`, `server/src/achievements/routes.ts` (register in `app.ts`)
- Modify: `server/src/main.ts` (startup run), `server/src/clips/routes.ts` (finalize, view, PATCH tags), `server/src/albums/routes.ts` (clip add / album update)
- Test: `server/test/achievements.test.ts`

**Interfaces (Produces):**
```ts
// stats.ts
export async function serverStats(db: Db, userId: string, now?: Date): Promise<Record<ServerStat, number> & { distinctGames: number }>;
//  uploads: own clips kind='clip'; anyUploads: all own clips; totalViews: sum(anon_views + clip_views) over own clips;
//  taggedIn: clip_tags rows for the user on clips owned by others; tagsGiven: clip_tags rows on own clips;
//  maxAlbumClips: max clip count over own albums; seenByEveryone: 1 if some own clip was viewed by every *active* other user (≥ 1 such user);
//  blinkUploads: own kind='clip' with duration_ms < 5000; newYearUploads: own clips created on 1 January (server time zone);
//  yearOld: own clips with created_at ≤ now − 365 days
export function mergeStats(server: Record<ServerStat, number> & { distinctGames: number }, activity: StoredActivity): Partial<Record<StatKey, number>>;
//  activity counters as they are, plus clipsSaved = max(activity.clipsSaved, server.anyUploads), games = max(activity.games, distinct own game names)
export interface StoredActivity { counters: Partial<Record<ActivityCounter, number>>; firsts: ActivityFirst[] }
export function mergeActivity(stored: StoredActivity | null, report: ActivityReport): StoredActivity; // counters: max per key; firsts: union, sorted
// engine.ts
export async function evaluateUser(db: Db, userId: string, opts: { quiet: boolean }): Promise<string[]>; // inserts newly earned (onConflictDoNothing), returns their ids
export async function evaluateAll(db: Db): Promise<number>;            // quiet, every non-disabled user, returns rows added
export async function myAchievements(db: Db, userId: string): Promise<MyAchievements>;
/** Never throws: a failure is logged, the request that triggered it still succeeds. */
export async function touch(app: FastifyInstance, userIds: (string | null | undefined)[]): Promise<void>;
```
Routes (`requireUser`):
- `GET /api/me/achievements` → `MyAchievements` (rows whose id is no longer in the catalog are skipped)
- `POST /api/me/achievements/seen` `{ ids?: string[] }` (absent = all) → 204; sets `seen_at = now()` where null
- `POST /api/me/activity` `ActivityReport` → `MyAchievements`. zod: counters are `int ≥ 0 ≤ 1e13`, unknown counter keys and unknown firsts are dropped (a newer app may know more), a negative or non-integer value → 400. Stores `mergeActivity`, then `evaluateUser(quiet: report.backfill === true)`.

Triggers (await `touch`, before replying): finalize → uploader + tagged users; `POST /api/clips/:id/view` → clip owner; `PATCH /api/clips/:id` with tags → owner + newly tagged; album create/add/update → album owner. `main.ts`: after `ensureAdmin`, `await evaluateAll(db)` with a log line `awarded N achievements for past activity` when N > 0 (failure logged, not fatal).

- [ ] **Step 1: Write the failing tests** (`achievements.test.ts`; insert clips directly into the DB where uploads aren't the point)
  - `first upload is loud`: finalize a clip through the real upload path (the helpers' upload function) → `GET /api/me/achievements` has `first-upload` and `first-clip` with `quiet:false, seen:false`; `GET /api/me` `newAchievements ≥ 2`; POST seen `{}` → `newAchievements` 0.
  - `startup run is quiet and idempotent` (Review Focus 4): 10 clips inserted for alice → `evaluateAll` → `clips-10` `quiet:true`; run `evaluateAll` again → returns 0, `earned_at` unchanged, still quiet.
  - `badges stay`: delete the clips → `clips-10` still earned.
  - `activity merge` (Review Focus 3): report `{counters:{recordedMs: 40e6}, firsts:['cut']}` → `hours-10` + `first-cut` earned loud; report `{counters:{recordedMs: 5}, firsts:['zoom']}` → stored `recordedMs` still 40e6, firsts `['cut','zoom']`.
  - `backfill is quiet`: report with `backfill:true` → awarded rows `quiet:true`.
  - `report validation`: `{counters:{clipsSaved:-1}, firsts:[]}` → 400; `{counters:{future: 3}, firsts:['someday']}` → 200 and nothing stored for those.
  - `views`: bob's view on alice's clip → alice's `seenByEveryone` earned when bob is the only other active user; a disabled carol doesn't block it; ten anon views + bob → `views-10` (anon counts).
  - `editor-all` from 8 firsts reported over two reports.
  - `curator` via the albums API with 10 clips.
  - `night-owl` hidden def unlocks `theme:midnight` in `unlocked`.
- [ ] **Step 2:** Run → FAIL
- [ ] **Step 3:** Implement stats (one SQL query per stat group is fine, readability over cleverness), engine, routes, triggers, startup.
- [ ] **Step 4:** `pnpm test && pnpm typecheck` → green
- [ ] **Step 5:** Commit `feat(server): achievements from uploads, views, tags, albums and app activity`

---

### Task 4: Website: framed avatars, looks applied, profile badges, account picture + looks

**Files:**
- Modify: `server/src/web/layout.tsx` (`Avatar`, `Layout`), `server/src/web/components.tsx`, `server/src/web/pages/{user,share,albums,admin,account}.tsx`, `server/src/web/routes.ts`, `server/public/app.css`, `server/public/app.js`
- Create: `server/src/web/badges.tsx` (`BadgeStrip`, `Banner`)
- Test: `server/test/web.test.ts` (extend)

**Interfaces:**
- `Avatar(props: { name: string; url?: string | null; frame?: string | null; size?: 'sm'|'md'|'lg' })`: draws `<img src={url} alt="" onerror="this.remove()">` inside the existing span, so the initial underneath shows when it fails (Review Focus 5). Adds class `frame frame-<id>` only for `md`/`lg` and the top bar (all ≥ 28 px; `sm` is 26 px today: make the top bar and card avatars 28 px).
- `Layout`: when `user`, render `<style>:root{…looksTokens(user.looks)…}</style>` in `<head>` after the stylesheet (values are catalog values; still pass through `esc`). Top bar `.who`: avatar with url+frame, and a `<span class="new-dot" title="New achievement">` when `user.newAchievements > 0`.
- `BadgeStrip({ earned })`: all earned badges newest first, each `<span class="badge-medal" title="<name>: <description>">icon</span>`; nothing when none.
- `/u/:username`: banner strip (`<div class="profile-banner banner-<id>">`) behind `.profile-head` when the profile owner has one, framed `lg` avatar, `BadgeStrip`. When the viewer opens their **own** profile, mark all unseen as seen after rendering (the badge on the account button goes away on the next page).
- `/account`: new "Picture" panel (preview avatar, "Choose picture", "Remove") and "Looks" panel (radio swatches for accent, theme cards, frame and banner choices incl. "None"; locked ones `disabled` with `title="Unlocks with <achievement name>"` and a lock glyph). Saving looks: `PUT /api/me/looks` then `location.reload()`.
- Crop dialog in `app.js`: `<dialog class="avatar-crop">` with a 280 px round preview canvas, drag to pan, wheel/slider to zoom (1–4×); "Save" draws the visible square to a 512×512 canvas → `toBlob('image/jpeg', 0.9)` → `fetch('/api/me/avatar', {method:'PUT', headers:{'content-type':'image/jpeg'}, body})`. Error 422 → the existing `friendly()` message path.
- CSS (`app.css`, and the same rules are copied into the desktop in Task 6): `.frame-bronze/-silver/-gold` (2px gradient ring + soft glow via `box-shadow`/`outline`), `.frame-rainbow` (conic-gradient ring via a `::before` with `inset:-3px`, mask), `.frame-pulse` (animated glow, `@media (prefers-reduced-motion: reduce)` static), `.banner-curator/-editor/-squad` (gradients/patterns, 120 px tall on the profile), `.badge-medal`, `.new-dot`.

- [ ] **Step 1: Write the failing tests** (web.test.ts style: inject the page, assert on the HTML)
  - logged-in page contains `--accent:#2563eb` (or the ocean colour) after `PUT looks {accent:'ocean'}`; logged-out page has no `:root{` override
  - `/u/alice` with an avatar has `<img src="http://localhost/avatars/` … `?v=1"`; with `frame:bronze` equipped, `frame-bronze`; with `banner:curator`, `banner-curator`; with earned `first-upload`, a `badge-medal` titled with its name
  - top bar shows `new-dot` while alice has unseen achievements; after alice opens `/u/alice`, the next page has none
  - `/account` lists every accent; `gold` is `disabled` with `Unlocks with` in its title until `games-5` is earned
- [ ] **Step 2:** Run → FAIL
- [ ] **Step 3:** Implement pages, CSS, `app.js` handlers (picture choose/remove, crop dialog, looks form)
- [ ] **Step 4:** `pnpm test && pnpm typecheck` → green. Then open the site with `pnpm dev` against pglite (or the dev DB) and check the crop dialog, a frame and a banner by eye, at phone width too.
- [ ] **Step 5:** Commit `feat(web): profile pictures, looks, frames, banners and badges on the website`

---

### Task 5: Desktop core: activity from the library, reporting, notices, picture and looks commands

**Files:**
- Modify: `desktop/src-tauri/src/library/db.rs` (migration `origin TEXT NULL` with comment `-- 'combine', 'part' or 'download': made from other clips or fetched from the server, not recorded`; `NewClip.origin: Option<&'static str>`, default None at every existing site; `Library::activity()`), `combine.rs`, `parts.rs` (both insert sites), `server_clips.rs` (download insert) set origin
- Create: `desktop/src-tauri/src/activity.rs`
- Modify: `desktop/src-tauri/src/account.rs` (refresh triggers an immediate report; new commands), `upload/auth.rs` + `commands.rs` `Account` (+`avatar_url`, `looks: serde_json::Value`, `serde(default)`), `state.rs` (reporter), `lib.rs` (register commands, module), the clip-saved emit sites, the save-edit command, import finish, combine/part commands, `thumb_image::set_thumbnail_image` (each calls `activity::changed(&app)`)
- Test: unit tests in `library/db.rs` and `activity.rs`

**Interfaces (Produces):**
```rust
// library
pub struct LocalActivity { pub clips_saved: u64, pub screenshots: u64, pub recorded_ms: u64, pub recordings: u64, pub imports: u64, pub games: u64, pub firsts: Vec<&'static str> }
impl Library { pub fn activity(&self) -> Result<LocalActivity> }
//  recorded clips = kind 'clip', source_path NULL, origin NULL. clips_saved = recorded and is_recording 0; recorded_ms = sum of recorded durations
//  (recordings included); recordings = is_recording 1; imports = source_path NOT NULL; screenshots = kind 'screenshot' and origin NULL;
//  games = distinct non-null game_name. Firsts: cut (removed_ranges != '[]'), zoom, subtitle, blur (blur_ranges), mutePart (audio_ranges),
//  combine/part (origin), nightOwl (a recorded clip with strftime('%H', created_at/1000, 'unixepoch', 'localtime') in ('03','04')),
//  marathon (is_recording 1 and duration_ms > 10_800_000). thumbnailImage is added by activity.rs: any `*.image.jpg` in the thumbs dir.
// activity.rs
pub fn report_body(a: &LocalActivity, thumb_image: bool, backfill: bool) -> serde_json::Value; // camelCase ActivityReport
pub struct Backfilled; // <data>/activity.json {"backfilled": ["<serverUrl>|<username lowercase>"]}; load/contains/insert+save (atomic like auth.rs)
#[derive(Deserialize)] pub struct Earned { id, name, description, earned_at, seen, quiet, reward_name: Option<String> }
pub struct AchievementNotice { pub title: String, pub body: String }
pub fn notices(earned: &[Earned]) -> (Vec<AchievementNotice>, Vec<String> /* ids to mark seen */);
//  all unseen quiet → one notice "You've already earned N achievements" (N=1: "…an achievement"), body "See them in your profile.";
//  each unseen loud → "Achievement unlocked: <name>", body "<description>[ · unlocked <reward name>]". Seen ones: none.
pub fn changed(app: &AppHandle);      // debounced 5 s (generation counter in AppState), then report_now
pub async fn report_now(app: &AppHandle) -> Result<()>; // no login → Ok (waits for the next login); 404 → Ok (old server)
//  POST /api/me/activity (backfill: !Backfilled.contains), mark backfilled on success, then announce:
//  window focused → emit "achievement" {title, body} per notice; else feedback::notify(title, body). Then POST /api/me/achievements/seen {ids}.
// commands
#[tauri::command] pub async fn my_achievements(app) -> Result<serde_json::Value>;       // GET /api/me/achievements
#[tauri::command] pub async fn set_looks(app, looks: serde_json::Value) -> Result<Account>; // PUT /api/me/looks; refusal "not_unlocked" → "Earn the achievement first."
#[tauri::command] pub async fn set_avatar(app, jpeg: Vec<u8>) -> Result<Account>;      // PUT /api/me/avatar (image/jpeg)
#[tauri::command] pub async fn remove_avatar(app) -> Result<Account>;
#[tauri::command] pub async fn read_picture(path: PathBuf) -> Result<String>;            // data:image/...;base64, ≤ 10 MB, png/jpg/jpeg/webp by extension, else InvalidInput
```
`save_name` becomes "save me": also keeps `avatarUrl` and `looks` from the `me` JSON in `auth.json`, so the app starts with the right picture and colours before it reaches the server.

- [ ] **Step 1: Write the failing tests**
  - `db.rs`: `activity_counts_recorded_clips_only`: one buffer clip at 03:30 local time (build `created_at` from a local `NaiveDateTime`… or use SQLite: insert with a created_at computed by `SELECT strftime('%s','2026-01-01 03:30','utc')*1000` so the test is time-zone proof), one recording of 3 h 1 min, one import, one `origin: Some("combine")`, one screenshot, one with removed_ranges set → `clips_saved 1, recordings 1, imports 1, screenshots 1, recorded_ms = buffer + recording`, firsts contain `nightOwl, marathon, combine, cut`, not `zoom`.
  - `activity.rs`: `notices_one_summary_for_quiet`: 3 unseen quiet + 1 seen loud + 1 unseen loud with reward → titles `["You've already earned 3 achievements", "Achievement unlocked: Night owl"]`, body of the second ends ` · unlocked Midnight`, ids = the 4 unseen.
  - `notices_singular`: 1 quiet → `You've already earned an achievement`.
  - `report_body_shape`: camelCase keys `clipsSaved`, `recordedMs`, `backfill: true`, `firsts` includes `thumbnailImage` when passed true.
  - `backfilled_roundtrip`: insert, reload from disk, contains (case-insensitive username).
- [ ] **Step 2:** `cd desktop/src-tauri && cargo test activity` → FAIL
- [ ] **Step 3:** Implement. Wire `changed()` at the listed sites; `refresh_account` spawns `report_now` after the device report.
- [ ] **Step 4:** `cargo test && cargo clippy --all-targets -- -D warnings` → green
- [ ] **Step 5:** Commit `feat(desktop): report activity from the library, announce achievements, picture and looks commands`

---

### Task 6: Desktop UI: avatars with pictures and frames, looks, Profile screen, crop, toasts

**Files:**
- Modify: `desktop/package.json` (`"@klipt/shared": "workspace:*"`, then `pnpm install`), `desktop/src/lib/api.ts` (Account `avatarUrl`, `looks`; owner/user types gain `avatarUrl?`, `frame?`; commands from Task 5; `on("achievement", …)`), `desktop/src/components/Avatar.svelte`, `desktop/src/App.svelte`, `desktop/src/views/Feed.svelte`, `desktop/src/components/ClipPlayer.svelte`, the tag picker (`grep -rn "listUsers\|api.users" desktop/src`), `desktop/src/views/Settings.svelte` (Account tab: picture row with "Change picture" + "Open profile")
- Create: `desktop/src/views/Profile.svelte`, `desktop/src/components/AvatarCropDialog.svelte`, `desktop/src/lib/avatarCrop.ts`, `desktop/src/lib/looks.ts`, `desktop/src/components/frames.css` (the website's frame/banner rules, imported by Avatar and Profile)
- Test: `desktop/src/lib/avatarCrop.test.ts`, `desktop/src/lib/looks.test.ts`

**Interfaces:**
```ts
// lib/avatarCrop.ts: the visible square in image pixels for a round preview of `view` px
export function cropRect(img: { w: number; h: number }, zoom: number /* 1..4 */, pan: { x: number; y: number } /* px in view space */, view: number): { sx: number; sy: number; size: number };
//  at zoom 1 the square is the image's shorter side, centred, moved by pan; always clamped inside the image
export function clampPan(img: { w: number; h: number }, zoom: number, pan: { x: number; y: number }, view: number): { x: number; y: number };
// lib/looks.ts
export function applyLooks(looks: Looks | null | undefined, root?: HTMLElement): void; // sets every looksTokens() property on documentElement; null → DEFAULT_LOOKS
export function avatarState(url: string | null | undefined, failed: boolean): 'image' | 'initial';
```
- `Avatar.svelte` props `{ name, size = 28, url = null, frame = null }`: `<img>` over the initial; `onerror` → initial (Review Focus 5); frame class only when `size >= 28`.
- `App.svelte`: `$effect(() => applyLooks(account?.looks))`; a `profile` screen (kept alive like the others) reached from the account menu ("Profile", first item) and from clicking the menu's header; listen to `achievement` → `toast(title + body…, "success", { label: "View", run: () => go({ name: "profile" }) })`.
- `Profile.svelte`: header (banner strip, framed 88 px avatar, name), badges grouped by category: earned (icon + name + date), locked (dimmed, description), hidden unearned shown only as a count `+N hidden` and as `???` tiles with a lock. "Looks" section: Picture (Change / Remove), Accent swatches, Themes, Frames, Banners, locked ones disabled with "Unlocks with <name>". Picking calls `api.setLooks` and updates `account`. Data from `api.myAchievements()` + the shared catalog.
- `AvatarCropDialog.svelte`: pick with the dialog plugin (png/jpg/jpeg/webp) → `api.readPicture` → canvas preview using `cropRect`/`clampPan`, drag + wheel/slider zoom → `canvas.toBlob('image/jpeg', 0.9)` at 512×512 → `api.setAvatar(Array.from(bytes))`.
- Feed cards, player header, tag picker and the account button pass `url`/`frame` from `owner`/user rows.

- [ ] **Step 1: Write the failing tests**
  - `cropRect({w:2000,h:1000}, 1, {x:0,y:0}, 280)` → `{sx:500, sy:0, size:1000}`; zoom 2 → `size:500, sx:750, sy:250`; a pan far right is clamped so `sx + size <= 2000`.
  - `applyLooks({accent:'ocean', theme:'klipt', frame:null, banner:null}, el)` sets `--accent` to ocean's colour and `--bg` to the klipt theme's; `applyLooks(null, el)` gives the defaults.
  - `avatarState('http://x/a.jpg', true)` → `'initial'`; `(null, false)` → `'initial'`; `('http://x', false)` → `'image'`.
- [ ] **Step 2:** `cd desktop && pnpm vitest run src/lib/avatarCrop.test.ts src/lib/looks.test.ts` → FAIL
- [ ] **Step 3:** Implement the libs, components, screen and wiring.
- [ ] **Step 4:** `pnpm test && pnpm check` → green; `pnpm tauri dev` against a local server: set a picture (crop), equip an accent, see the toast after saving a clip on a fresh account.
- [ ] **Step 5:** Commit `feat(desktop): profile screen, pictures, looks and achievement toasts`

---

### Task 7: Docs

- [ ] Roadmap entry in `docs/ROADMAP.md` (what shipped; the deferred items found along the way), and commit the spec with its status set to "approved, built".
- [ ] Commit `docs: achievements and customization`
