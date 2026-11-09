# Klipt Server & Website Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A self-hosted Klipt server: invite-only accounts, resumable clip uploads, a shared feed, Discord-embeddable share links. Deployed as one Docker Compose stack on Dokploy.

**Architecture:** Fastify (Node 22, TypeScript) serves both the JSON API (`/api/*`) and server-rendered HTML pages (TSX via `@kitajs/html`, plus a small vanilla `app.js`). Data lives in Postgres through Drizzle ORM, and clip files and thumbnails are kept on a volume. Uploads use the tus protocol (`@tus/server`) and are finalized by an API call that validates the file with ffprobe, remuxes it with faststart, and makes a thumbnail. Tests run against in-process Postgres (PGlite), so no Docker is needed for `pnpm test`.

**Tech Stack:** pnpm workspaces, Node 22, TypeScript, Fastify 5, Drizzle ORM (`pg` in prod, `@electric-sql/pglite` in tests), zod, `@node-rs/argon2`, `@tus/server` + `@tus/file-store`, `@fastify/cookie`, `@fastify/static`, `@fastify/rate-limit`, `@fastify/formbody`, `@kitajs/html`, nanoid, tsup, vitest, ffmpeg/ffprobe.

**Spec:** `docs/superpowers/specs/2026-11-09-medal-alternative-design.md`

## Global Constraints

- One Docker Compose stack (Postgres + server). HTTPS and the domain are handled by Dokploy's reverse proxy, and the server trusts `X-Forwarded-*`.
- Invite-only. The first admin is created from the `ADMIN_USERNAME`/`ADMIN_PASSWORD` env vars when the users table is empty.
- Web auth uses an HttpOnly session cookie (`klipt_session`, 30-day expiry, `Secure` when `BASE_URL` is https). Desktop auth uses `Authorization: Bearer <token>`, which never expires and can be revoked. Only the SHA-256 of a token is stored.
- Share URL format: `${BASE_URL}/c/<id>`, where `<id>` is 12 characters of nanoid's URL-safe alphabet.
- Visibility is `feed` (default) or `link`. A `link` clip appears only in its owner's lists, but anyone with the URL can view it.
- Only H.264 video (with AAC or no audio) is accepted. The stored file is always a faststart MP4.
- No storage quotas. The admin sees total storage used.
- Env config: `BASE_URL` (required), `DATABASE_URL` (required), `DATA_DIR` (default `/data`), `PORT` (default `3000`), `MAX_UPLOAD_MB` (default `2048`), `ADMIN_USERNAME`, `ADMIN_PASSWORD`.
- Usernames: 3–24 characters matching `[A-Za-z0-9_]`, unique case-insensitively. Passwords: at least 8 characters. Clip titles: trimmed, 1–100 characters.

## Review Focus

1. **Seeking on phones and in Discord**: media must answer `Range` requests with `206 Partial Content`, or videos won't seek on iOS. Test in Task 7.
2. **A disabled user still has a valid token**: disabling must cut off both web and desktop access immediately. Test in Task 4.
3. **Interrupted or abandoned uploads**: a partial tus upload resumes from its offset, and uploads older than 24h that were never finalized get deleted from disk. Tests in Task 6.
4. **A non-video or corrupt file gets uploaded**: finalize answers `422` with a readable message and deletes the file, and no clip row is created. Test in Task 6.
5. **Link-only clips leaking**: they must not appear in the feed, on user pages, or in the game filter for other users, while `/c/<id>` still works for anyone. A deleted clip's link shows a friendly 404 page. Tests in Tasks 7 and 9.

---

## File Structure

```
package.json, pnpm-workspace.yaml, .gitignore, .dockerignore, README.md, docker-compose.yml
packages/shared/src/api.ts      # zod schemas + TS types for every API body/response (reused by desktop app)
packages/shared/src/index.ts
server/package.json, tsconfig.json, tsup.config.ts, vitest.config.ts, drizzle.config.ts, Dockerfile
server/src/config.ts            # loadConfig(env)
server/src/app.ts               # buildApp(deps) -> FastifyInstance
server/src/main.ts              # prod entry: config, db, migrate, bootstrap, cleanup timer, listen
server/src/db/schema.ts         # drizzle tables
server/src/db/client.ts         # createDb(databaseUrl) / createTestDb()
server/src/db/migrations/       # drizzle-kit generated SQL
server/src/auth/passwords.ts    # hashPassword, verifyPassword
server/src/auth/sessions.ts     # createSession, resolveToken, revokeSession, revokeAllForUser
server/src/auth/plugin.ts       # request.user decoration, requireUser, requireAdmin
server/src/auth/routes.ts       # login, logout, me, sessions, register
server/src/admin/bootstrap.ts   # ensureAdmin
server/src/admin/routes.ts      # invites, users, stats
server/src/clips/media.ts       # probeVideo, remuxFaststart, makeThumbnail
server/src/clips/repo.ts        # listClips, listGames, getClip, ...
server/src/clips/routes.ts      # clips API + finalize + media serving
server/src/uploads/tus.ts       # mountTus, cleanupStaleUploads
server/src/web/layout.tsx, server/src/web/components.tsx
server/src/web/pages/{login,invite,feed,user,me,admin,share,notFound}.tsx
server/src/web/routes.ts
server/public/app.css, server/public/app.js
server/test/helpers.ts          # makeTestApp, loginAs, makeFixtureVideo, tusUpload
server/test/*.test.ts
```

---

### Task 1: Monorepo, shared package, server skeleton

**Files:**
- Create: root `package.json` (scripts `test`, `build`, `dev` delegating to `server`), `pnpm-workspace.yaml` (`packages/*`, `server`, later `desktop`), `.gitignore`, `.dockerignore`
- Create: `packages/shared/src/index.ts`, `packages/shared/src/api.ts` (start with `HealthResponse`), and `packages/shared/package.json`, which exports the TS source (`"main": "src/index.ts"`)
- Create: `server/package.json`, `tsconfig.json` (`"jsx": "react-jsx"`, `"jsxImportSource": "@kitajs/html"`, strict), `tsup.config.ts` (entry `src/main.ts`, `noExternal: ['@klipt/shared']`), `vitest.config.ts`
- Create: `server/src/config.ts`, `server/src/app.ts`
- Test: `server/test/config.test.ts`, `server/test/health.test.ts`

**Interfaces:**
- Produces: `loadConfig(env: Record<string,string|undefined>): Config`, where `Config = { baseUrl: string; databaseUrl: string; dataDir: string; port: number; maxUploadBytes: number; adminUsername?: string; adminPassword?: string; secureCookies: boolean }`
- Produces: `buildApp(deps: { config: Config; db: Db }): Promise<FastifyInstance>`. Task 1 accepts `db` as `unknown` until Task 2 defines `Db`. `trustProxy: true`.
- Produces: `GET /api/health` → `{ ok: true }`

- [ ] **Step 1:** `git init`, then create the workspace files and install the dependencies listed in Tech Stack.
- [ ] **Step 2: Write failing tests**
  - `loadConfig({BASE_URL:'https://clips.example.com', DATABASE_URL:'postgres://x'})` → `port === 3000`, `dataDir === '/data'`, `maxUploadBytes === 2048*1024*1024`, `secureCookies === true`; it strips a trailing slash from `BASE_URL`.
  - `loadConfig({})` throws an error whose message names `BASE_URL`.
  - `GET /api/health` via `app.inject` → 200 `{ ok: true }`.
- [ ] **Step 3:** Run `pnpm --filter @klipt/server test`. Expected: FAIL, because the modules don't exist yet.
- [ ] **Step 4:** Implement `config.ts` (a zod schema over env) and `app.ts`.
- [ ] **Step 5:** Run `pnpm --filter @klipt/server test`. Expected: PASS.
- [ ] **Step 6:** Commit with `chore: scaffold klipt monorepo and server skeleton`.

### Task 2: Database schema, migrations, test harness

**Files:**
- Create: `server/src/db/schema.ts`, `server/src/db/client.ts`, `server/drizzle.config.ts`, `server/src/db/migrations/*` (generated)
- Create: `server/test/helpers.ts` (`makeTestApp`)
- Test: `server/test/db.test.ts`

**Interfaces:**
- Produces tables (all timestamps are `timestamptz`):
  - `users(id uuid pk default random, username text, usernameKey text unique /*lowercase*/, passwordHash text, isAdmin bool default false, disabledAt null, createdAt)`
  - `sessions(id uuid pk, userId fk→users cascade, tokenHash text unique, kind 'web'|'desktop', label text null, createdAt, lastUsedAt, expiresAt null)`
  - `invites(code text pk, createdBy fk→users, createdAt, expiresAt, usedBy fk null, usedAt null, revokedAt null)`
  - `uploads(id text pk /*tus id*/, userId fk→users cascade, createdAt)`
  - `clips(id text pk, userId fk→users cascade, title text, gameName text null, steamAppId int null, visibility 'feed'|'link', durationMs int, width int, height int, sizeBytes bigint, createdAt)`. Index on `(createdAt desc, id desc)` and on `gameName`.
- Produces: `type Db` (the Drizzle instance type, shared by pg and PGlite), `createDb(databaseUrl: string): Promise<Db>`, `createTestDb(): Promise<Db>` (PGlite, migrated), `runMigrations(db: Db): Promise<void>`
- Produces: `makeTestApp(): Promise<{ app: FastifyInstance; db: Db; config: Config; dataDir: string; close(): Promise<void> }>`. It uses a fresh PGlite and a fresh temp `dataDir`, sets `BASE_URL=http://localhost`, and leaves `MAX_UPLOAD_MB` at its default unless overridden by `makeTestApp({ maxUploadMb })`.

- [ ] **Step 1: Write a failing test.** After `createTestDb()`, inserting two users whose `usernameKey` differs only in case-folding (`'adam'` twice) rejects with a unique violation.
- [ ] **Step 2:** Run it. Expected: FAIL.
- [ ] **Step 3:** Write the schema, run `pnpm drizzle-kit generate` to produce the migrations, then implement `client.ts` and `makeTestApp`.
- [ ] **Step 4:** Run it. Expected: PASS. `health.test.ts` now uses `makeTestApp` and still passes.
- [ ] **Step 5:** Commit with `feat(server): database schema and test harness`.

### Task 3: Passwords, sessions, login/logout

**Files:**
- Create: `server/src/auth/passwords.ts`, `server/src/auth/sessions.ts`, `server/src/auth/plugin.ts`, `server/src/auth/routes.ts`
- Modify: `packages/shared/src/api.ts`, adding `LoginRequest`, `LoginResponse`, `Me`, `SessionInfo`
- Test: `server/test/auth.test.ts`. Add `loginAs(app, username, password, client)` to `helpers.ts`, returning the cookie header for web or the token for desktop.

**Interfaces:**
- Produces: `hashPassword(pw: string): Promise<string>`, `verifyPassword(hash: string, pw: string): Promise<boolean>` (argon2id)
- Produces: `createSession(db, userId, kind: 'web'|'desktop', label?: string): Promise<{ token: string; session: SessionRow }>`. The token is 32 random bytes as base64url. Web sessions get `expiresAt = now + 30d`.
- Produces: `resolveToken(db, token): Promise<AuthUser | null>`, where `AuthUser = { id; username; isAdmin; sessionId }`. It returns null when the session is expired or missing, or the user is disabled. It updates `lastUsedAt` at most once per minute.
- Produces: `revokeSession(db, sessionId)`, `revokeAllForUser(db, userId)`
- Produces: the Fastify decoration `request.user: AuthUser | null`, read from the Bearer header first and then the `klipt_session` cookie. Also the preHandlers `requireUser` (401 `{error:'unauthorized'}`) and `requireAdmin` (403).
- Routes:
  - `POST /api/auth/login {username, password, client:'web'|'desktop', deviceName?}` → web: sets the cookie and returns `Me`. Desktop: returns `{ token, me }`. Bad credentials or a disabled user → 401 `{error:'invalid_credentials'}`. Rate limit is 10/min per IP → 429.
  - `POST /api/auth/logout` → revokes the current session and clears the cookie, returns 204.
  - `GET /api/me` → `Me = { id, username, isAdmin }`.
  - `GET /api/auth/sessions` → `SessionInfo[] = {id, kind, label, createdAt, lastUsedAt, current}`. `DELETE /api/auth/sessions/:id` revokes one of the user's own sessions only.

- [ ] **Step 1: Write failing tests**
  - Login is case-insensitive on username (`ADAM` logs in `adam`).
  - Wrong password → 401 `invalid_credentials`. The 11th attempt within a minute → 429.
  - Web login sets an HttpOnly `klipt_session` cookie, and `GET /api/me` with it → 200 `{username:'adam'}`.
  - Desktop login returns a token, and `GET /api/me` with Bearer → 200. After `POST /api/auth/logout` with that Bearer, `/api/me` → 401.
  - A web session whose `expiresAt` is in the past → 401.
  - `DELETE /api/auth/sessions/<another user's session id>` → 404, and their session still works.
- [ ] **Step 2:** Run. Expected: FAIL.
- [ ] **Step 3:** Implement the passwords, sessions, plugin, and routes. Register `@fastify/cookie` and `@fastify/rate-limit` in `app.ts`. For tests, seed users directly through the db and `hashPassword`.
- [ ] **Step 4:** Run. Expected: PASS.
- [ ] **Step 5:** Commit with `feat(server): sessions and login`.

### Task 4: Admin bootstrap, invites, registration, user management

**Files:**
- Create: `server/src/admin/bootstrap.ts`, `server/src/admin/routes.ts`
- Modify: `server/src/auth/routes.ts` (register), `packages/shared/src/api.ts` (`Invite`, `RegisterRequest`, `AdminUser`, `AdminStats`)
- Test: `server/test/admin.test.ts`

**Interfaces:**
- Produces: `ensureAdmin(db, config): Promise<'created'|'exists'>`. If the users table is empty it creates an admin from the config; if the table is empty and the credentials are missing it throws `'ADMIN_USERNAME and ADMIN_PASSWORD must be set on first start'`.
- Routes (admin only unless noted):
  - `POST /api/admin/invites` → `Invite = { code, url: ${baseUrl}/invite/<code>, expiresAt }`. The code is 16 characters of nanoid, single-use, and expires in 7 days.
  - `GET /api/admin/invites` → the invites that are unused, unrevoked, and unexpired. `DELETE /api/admin/invites/:code` → revoke.
  - `POST /api/register {inviteCode, username, password}` (public, rate limited at 10/min). It validates the username and password rules, consumes the invite atomically, creates the user, and logs them in on the web (sets the cookie). Errors: `invalid_invite` (400), `username_taken` (409), `invalid_username` / `weak_password` (400).
  - `GET /api/admin/users` → `AdminUser[] = {id, username, isAdmin, disabled, clipCount, createdAt}`. `PATCH /api/admin/users/:id {disabled: boolean}`: disabling calls `revokeAllForUser`. An admin can't disable themselves (400).
  - `GET /api/admin/stats` → `{ users, clips, storageBytes }`, where `storageBytes` is the sum of `clips.sizeBytes`.

- [ ] **Step 1: Write failing tests**
  - `ensureAdmin` on an empty db creates an admin, a second call returns `'exists'`, and on an empty db without credentials it throws.
  - Register with a valid invite → 200 and logged in. Reusing the same invite → 400 `invalid_invite`. An expired or revoked invite → 400.
  - Registering `Adam` when `adam` exists → 409 `username_taken`. A username `a b` → 400 `invalid_username`. A 7-character password → 400 `weak_password`.
  - A non-admin calling `/api/admin/*` → 403.
  - **Review Focus 2:** a user logged in on both web and desktop is disabled by the admin. Both the cookie and the Bearer token then get 401 on `/api/me`, and login → 401.
- [ ] **Step 2:** Run. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run. Expected: PASS.
- [ ] **Step 5:** Commit with `feat(server): invites, registration, admin users`.

### Task 5: Media tools (ffprobe / ffmpeg)

**Files:**
- Create: `server/src/clips/media.ts`
- Modify: `server/test/helpers.ts`, adding `makeFixtureVideo(dir, opts?: { seconds?: number; codec?: 'libx264'|'mpeg4'; audio?: boolean }): Promise<string>`. It generates a file with `ffmpeg -f lavfi -i testsrc=size=320x240:rate=30` (+ `-f lavfi -i sine` when `audio`). Defaults: 2 s, libx264, audio. MP4 output, *without* faststart.
- Test: `server/test/media.test.ts`

**Interfaces:**
- Produces: `probeVideo(path): Promise<VideoInfo>`, where `VideoInfo = { durationMs: number; width: number; height: number; videoCodec: string; audioCodec: string | null }`. It throws `MediaError('not_a_video')` when ffprobe fails or there's no video stream.
- Produces: `assertPlayable(info: VideoInfo): void`. It throws `MediaError('unsupported_codec')` unless `videoCodec === 'h264'` and `audioCodec` is `'aac'` or `null`.
- Produces: `remuxFaststart(input, output): Promise<void>` (`-c copy -movflags +faststart`), and `makeThumbnail(input, output, durationMs): Promise<void>`, a JPEG 640px wide taken at `min(1000, durationMs/2)` ms.
- Produces: `class MediaError extends Error { code: 'not_a_video' | 'unsupported_codec' }`

- [ ] **Step 1: Write failing tests**
  - A 2 s fixture probes to `durationMs` within ±100 of 2000, 320×240, `h264`, `aac`.
  - A text file → `not_a_video`. An mpeg4 fixture → `unsupported_codec`. A file with no audio → `audioCodec === null` and still playable.
  - After `remuxFaststart`, the `moov` atom comes before `mdat`: read the file bytes and compare `indexOf('moov')` with `indexOf('mdat')`.
  - `makeThumbnail` produces a JPEG starting with bytes `FF D8`.
- [ ] **Step 2:** Run. Expected: FAIL.
- [ ] **Step 3:** Implement with `child_process.execFile`. The binary paths can be overridden with `FFMPEG_PATH` / `FFPROBE_PATH`.
- [ ] **Step 4:** Run. Expected: PASS.
- [ ] **Step 5:** Commit with `feat(server): media probing, faststart remux, thumbnails`.

### Task 6: Resumable uploads and finalize

**Files:**
- Create: `server/src/uploads/tus.ts`
- Create: `server/src/clips/ids.ts` (`newClipId(): string`, 12 characters of nanoid)
- Create: `server/src/clips/routes.ts`, starting with finalize only
- Modify: `server/src/app.ts`, `packages/shared/src/api.ts` (`FinalizeRequest`, `Clip`)
- Modify: `server/test/helpers.ts`, adding `tusUpload(app, token, bytes: Buffer, opts?: { chunkSize?: number; stopAfterChunks?: number }): Promise<{ uploadId: string; location: string }>`. It speaks the tus 1.0 protocol through `app.inject` (POST create, then PATCH chunks with `Upload-Offset`).
- Test: `server/test/uploads.test.ts`

**Interfaces:**
- Produces: `mountTus(app, { db, config })`. It mounts the tus server at `/api/uploads/` with a `FileStore` at `${dataDir}/tus` and `maxSize: config.maxUploadBytes`. Its `onIncomingRequest` requires an authenticated user, answering 401 otherwise. `onUploadCreate` inserts an `uploads` row with the user id. Fastify must pass `application/offset+octet-stream` bodies through raw, and all tus methods (POST, HEAD, PATCH, OPTIONS, DELETE) are forwarded to `tus.handle(req.raw, res.raw)`.
- Produces: `cleanupStaleUploads(db, dataDir, now: Date, maxAgeMs = 24h): Promise<number>`. It deletes the tus files plus `.json` sidecars and the `uploads` rows older than `maxAgeMs`, and returns how many were removed.
- Produces: `POST /api/clips {uploadId, title, gameName?: string|null, steamAppId?: number|null, visibility?: 'feed'|'link'}` → 201 `Clip`. It checks that the upload belongs to the caller (otherwise 404) and is complete (otherwise 409 `upload_incomplete`). It probes the file, calls `assertPlayable`, remuxes to `${dataDir}/clips/<id>.mp4`, makes the thumbnail `<id>.jpg`, inserts the clip, and deletes the tus file and row. On a `MediaError` it answers 422 `{error: code, message}`, where the message is human-readable (e.g. "That file isn't a video Klipt can play.").
- Produces `Clip = { id, title, gameName, steamAppId, visibility, durationMs, width, height, sizeBytes, createdAt, owner: { username }, shareUrl, videoUrl: ${baseUrl}/media/<id>.mp4, thumbnailUrl: ${baseUrl}/media/<id>.jpg }`

- [ ] **Step 1: Write failing tests**
  - A full upload of a fixture followed by finalize → 201, `shareUrl` matches `^http://localhost/c/[A-Za-z0-9_-]{12}$`, and both `<id>.mp4` and `<id>.jpg` exist in `dataDir/clips`.
  - An upload without auth → 401.
  - **Review Focus 3:** upload 2 of 4 chunks, then `HEAD` → `Upload-Offset` equals 2×chunkSize. PATCH the rest from that offset, then finalize → 201.
  - Finalize after only partial chunks → 409 `upload_incomplete`.
  - User B finalizing user A's uploadId → 404.
  - **Review Focus 4:** upload a text file, then finalize → 422 `not_a_video`. The tus file is gone and the clip count is 0.
  - `makeTestApp({ maxUploadMb: 1 })`, creating an upload with `Upload-Length` of 2 MB → 413.
  - **Review Focus 3:** `cleanupStaleUploads` with `now` 25h after an unfinalized upload removes it (returns 1, file gone). With `now` 1h after, it returns 0.
  - Title `'   '` → 400. A 101-character title → 400.
- [ ] **Step 2:** Run. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run. Expected: PASS.
- [ ] **Step 5:** Commit with `feat(server): resumable uploads and clip finalize`.

### Task 7: Clips API and media serving

**Files:**
- Create: `server/src/clips/repo.ts`
- Modify: `server/src/clips/routes.ts`, `packages/shared/src/api.ts` (`ClipList`, `GameCount`, `UpdateClipRequest`)
- Test: `server/test/clips.test.ts`

**Interfaces:**
- Produces: `listClips(db, viewer: AuthUser, q: { scope: 'feed'|'mine'|'user'; username?: string; game?: string; cursor?: string; limit?: number /*default 24, max 60*/ }): Promise<{ items: ClipRow[]; nextCursor: string | null }>`
  - `feed`: all `visibility='feed'` clips.
  - `mine`: the viewer's clips, both visibilities.
  - `user`: that user's `feed` clips, or all of them when the viewer is that user.
  - Clips are ordered by `(createdAt desc, id desc)`. The cursor is base64url of `<createdAt ISO>|<id>`.
- Produces: `listGames(db, viewer, scope, username?): Promise<GameCount[]>`, where `GameCount = {gameName, steamAppId, count}`. It applies the same visibility rules, skips null games, and sorts by count desc.
- Routes (all require a user except media):
  - `GET /api/clips?scope&username&game&cursor` → `ClipList = {items: Clip[], nextCursor}`
  - `GET /api/games?scope&username` → `GameCount[]`
  - `GET /api/clips/:id` → `Clip`. Visible to any logged-in user who has the id.
  - `PATCH /api/clips/:id {title?, visibility?, gameName?, steamAppId?}`, owner only (403 otherwise)
  - `DELETE /api/clips/:id`, owner or admin. It deletes the row and both files and returns 204.
  - `GET /media/:id.mp4` and `GET /media/:id.jpg` are public (the id is the secret). They're served by `@fastify/static` from `${dataDir}/clips` with Range support and `Cache-Control: public, max-age=31536000, immutable`. An unknown id → 404.

- [ ] **Step 1: Write failing tests**
  - **Review Focus 5:** A has one `feed` clip and one `link` clip. B's `scope=feed` returns 1 item and B's `scope=user&username=a` returns 1, while A's `scope=mine` returns 2. B's `GET /api/games?scope=feed` counts only the feed clip. B can still `GET /api/clips/<link id>`, which returns 200.
  - Paging: with 30 clips, the first page has 24 and a `nextCursor`, the second has 6 and `nextCursor: null`, and the pages don't overlap.
  - `game=Overwatch 2` filters exactly.
  - B PATCHing A's clip → 403. A PATCHing the title to `'New'` → 200, and it's reflected in GET.
  - The admin DELETEs B's clip → 204, and both files are removed. B DELETEing A's clip → 403.
  - **Review Focus 1:** `GET /media/<id>.mp4` with `Range: bytes=0-99` → 206, `content-length: 100`, `accept-ranges: bytes`. `/media/nope.mp4` → 404.
- [ ] **Step 2:** Run. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run. Expected: PASS.
- [ ] **Step 5:** Commit with `feat(server): clips API and media serving`.

### Task 8: Website — layout, login, invite signup, feed, user page, my clips, admin

**Files:**
- Create: `server/src/web/layout.tsx` (`Layout({title, user, head?, children})`), `server/src/web/components.tsx` (`ClipCard`, `GameFilter`, `Pager`), `server/src/web/pages/{login,invite,feed,user,me,admin,notFound}.tsx`, `server/src/web/routes.ts`
- Create: `server/public/app.css`, `server/public/app.js` (vanilla: copy-link buttons, inline rename, visibility toggle, delete with confirm, create/revoke invite, disable user, revoke session; each calls the JSON API with `fetch` and reloads or updates the DOM)
- Test: `server/test/web.test.ts`

**Interfaces:**
- Consumes: `listClips`, `listGames` (Task 7) and the auth decoration (Task 3).
- Pages:
  - `GET /login` and `GET /invite/:code` are public. An invalid or used invite renders "This invite link is no longer valid."
  - `GET /` (feed), `GET /u/:username`, `GET /me`, and `GET /admin` (admin only, otherwise 404) redirect to `/login?next=<path>` when logged out.
  - The feed, user, and me pages take `?game=` and `?cursor=`. The game filter is a row of chips built from `listGames`, where each chip shows Steam header art when `steamAppId` is set (`https://cdn.cloudflare.steamstatic.com/steam/apps/<appid>/header.jpg`) and the name otherwise.
  - `ClipCard` shows the thumbnail, duration as `m:ss`, title, game, uploader, and relative date, and links to `/c/<id>`.
  - `/me` lists the user's clips (with rename, visibility, delete, copy link) and their sessions (with revoke).
  - `/admin` shows stats (storage formatted as GB), the invite list with a "Create invite" button (copies the URL), and the users list with disable/enable.
  - The look is dark by default, modern, and responsive down to a 360px width. The visual details are the builder's call.

- [ ] **Step 1: Write failing tests** (with `app.inject`, asserting on HTML substrings)
  - Logged out, `GET /` → 302 to `/login?next=%2F`.
  - Logged in as B, `GET /` contains A's feed clip title and not A's link-only title.
  - `GET /?game=Valorant` contains only the Valorant clip titles.
  - A non-admin `GET /admin` → 404. The admin gets 200 containing "Create invite".
  - `GET /invite/<used code>` contains "no longer valid".
  - A title containing `<script>` is rendered escaped (`&lt;script&gt;`).
- [ ] **Step 2:** Run. Expected: FAIL.
- [ ] **Step 3:** Implement the pages and `app.js`. Use `@kitajs/html` escaping (`safe` attribute / `Html.escapeHtml`) for all user strings.
- [ ] **Step 4:** Run. Expected: PASS.
- [ ] **Step 5:** Check it manually: `pnpm --filter @klipt/server dev` (tsx watch, against a local `docker run postgres`), log in as admin, create an invite, sign up in a private window, and upload a clip with a small script that uses `tus-js-client`. Confirm the feed, game filter, rename, and delete all work in the browser.
- [ ] **Step 6:** Commit with `feat(server): website pages`.

### Task 9: Share page with Discord embed

**Files:**
- Create: `server/src/web/pages/share.tsx`
- Modify: `server/src/web/routes.ts`
- Test: `server/test/share.test.ts`

**Interfaces:**
- `GET /c/:id` is public. It shows a `<video controls playsinline preload="metadata" poster=thumbnail src=videoUrl>`, plus the title, uploader, game, date, and a "Copy link" button.
- The head carries:
  - `og:title` (the title), `og:description` (`"<game> · clipped by <username>"`, or just the username when there's no game), `og:site_name` = `Klipt`, `og:url`, `og:image` (the thumbnail)
  - `og:type` = `video.other`, `og:video` and `og:video:secure_url` (the mp4), `og:video:type` = `video/mp4`, `og:video:width`/`height`
  - `twitter:card` = `player`, `theme-color`
- **Review Focus 5:** an unknown or deleted id → 404 rendering `notFound.tsx`: "This clip doesn't exist or was deleted."

- [ ] **Step 1: Write failing tests**
  - Logged out, `GET /c/<feed id>` → 200, and the HTML contains `<meta property="og:video" content="http://localhost/media/<id>.mp4"` and `og:image`.
  - Logged out, `GET /c/<link-only id>` → 200 (link-only is still shareable).
  - After the clip is DELETEd, `GET /c/<id>` → 404 containing "doesn't exist or was deleted".
- [ ] **Step 2:** Run. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run. Expected: PASS.
- [ ] **Step 5:** Commit with `feat(server): share page with Discord embeds`.

### Task 10: Production entry, Docker, Dokploy deploy

**Files:**
- Create: `server/src/main.ts`
- Create: `server/Dockerfile`, multi-stage, build context = repo root. The build stage uses `node:22-bookworm-slim` with corepack pnpm, runs `pnpm install --frozen-lockfile`, and runs `pnpm --filter @klipt/server build` (tsup). The runtime stage is `node:22-bookworm-slim` plus `apt-get install -y ffmpeg`, copies `dist`, `public`, `src/db/migrations`, and the prod `node_modules` from `pnpm deploy`, and runs as a non-root `node` user with `CMD ["node","dist/main.js"]` and a `HEALTHCHECK` on `/api/health`.
- Create: `docker-compose.yml`:
  - `db`: `postgres:17-alpine`, volume `pgdata`, healthcheck `pg_isready`.
  - `server`: `build: {context: ., dockerfile: server/Dockerfile}`, `depends_on: db (service_healthy)`, `expose: ["3000"]`, volume `klipt-data:/data`, env vars passed through from Dokploy (`BASE_URL`, `ADMIN_USERNAME`, `ADMIN_PASSWORD`, `POSTGRES_PASSWORD`, `MAX_UPLOAD_MB`), and `DATABASE_URL=postgres://klipt:${POSTGRES_PASSWORD}@db:5432/klipt`.
- Create: `README.md` with sections on what Klipt is, deploying on Dokploy (create a Compose service from the repo, set the env vars, add the domain → service `server`, port `3000`, enable HTTPS), first login, and inviting friends.

**Interfaces:**
- `main.ts`: `loadConfig(process.env)` → `createDb` → `runMigrations` → `ensureAdmin` → `buildApp` → `cleanupStaleUploads` on start and then every hour → `listen({ host: '0.0.0.0', port })`. It exits non-zero with a clear message on config errors.

- [ ] **Step 1:** Write `main.ts`, the `Dockerfile`, and `docker-compose.yml`.
- [ ] **Step 2:** Smoke test: `BASE_URL=http://localhost:3000 ADMIN_USERNAME=admin ADMIN_PASSWORD=changeme123 POSTGRES_PASSWORD=dev docker compose up --build -d`, with a temporary `ports: ["3000:3000"]` override set via `docker compose -f docker-compose.yml -f <scratch override>`. Then:
  - `curl -s localhost:3000/api/health` → `{"ok":true}`
  - `curl -s -X POST localhost:3000/api/auth/login -H 'content-type: application/json' -d '{"username":"admin","password":"changeme123","client":"desktop"}'` → JSON with `token`
  - `docker compose restart server`, then log in again → still works (data persisted, migrations idempotent).
- [ ] **Step 3:** `docker compose down -v`.
- [ ] **Step 4:** Run the full suite with `pnpm test`. Expected: all pass.
- [ ] **Step 5:** Commit with `feat(server): docker compose deployment and README`.
