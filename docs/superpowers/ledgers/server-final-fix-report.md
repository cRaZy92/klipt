# Final review fix wave — report

Status: DONE_WITH_CONCERNS (all 10 findings fixed; concerns listed at the end)

Branch feat/server, worktree /home/adam/Documents/Programming/medal-alternative/.worktrees/server. Not pushed. Docker not run.

## Commits
- d45fb16 fix(server): reject damaged videos with 422 corrupt_video  (finding 1)
- 4d6c095 fix(server): make finalize safely retryable after a lost response  (finding 2)
- 37c86a9 feat(server): let owners and admins delete a clip from its share page  (finding 3)
- 18d6098 chore: ignore .env files  (finding 4)
- 4db02fc fix(server): hardening batch from the final review  (findings 5–10)

---

## 1. Corrupt-but-probeable H.264 → 422 corrupt_video
**Changes**
- `server/src/clips/media.ts`: `MediaErrorCode` gains `'corrupt_video'`. `run()` now has a 10-minute execFile `timeout`. When ffmpeg/ffprobe exits with a non-zero numeric `err.code`, it rejects with `MediaError('corrupt_video', <stderr>)`. Anything else (a string code such as ENOENT/ENOSPC, a timeout kill with `code === null`, a maxBuffer overflow) stays a plain `Error` (with `cause`), which means a 500 and the upload is kept.
- `probeVideo` maps a MediaError from `run()` to `not_a_video` (ffprobe rejecting the file). It used to catch everything, so a missing ffprobe binary came back as a 422. Spawn errors now propagate as non-Media errors. A JSON parse failure is still `not_a_video`.
- `server/src/clips/routes.ts`: `mediaMessages.corrupt_video = "That video file is damaged and can't be played."`. The existing MediaError branch returns 422 and removes the upload.
- `server/test/helpers.ts`: `makeCorruptVideo(dir)` builds the fixture: 2 s fixture → `remuxFaststart` → truncated to 6 KB. ffprobe still works on it, remux exits 0, and the thumbnail step exits 234.

**Tests**
- media.test.ts: `maps a non-zero ffmpeg exit on a damaged file to corrupt_video`, `reports a missing binary as a non-media error` (FFPROBE_PATH/FFMPEG_PATH=/nonexistent)
- uploads.test.ts: `rejects a damaged (truncated) video with 422 corrupt_video and cleans up` (422 body, 0 clip rows, 0 upload rows, no tus files, empty clips dir), `answers 500 and keeps the upload when ffprobe is missing` (500, row kept with finalizingAt released, tus file kept, and a later finalize with ffprobe restored gives 201)

**RED** (`pnpm exec vitest run test/media.test.ts test/uploads.test.ts`)
```
     × maps a non-zero ffmpeg exit on a damaged file to corrupt_video 145ms
     × reports a missing binary as a non-media error 62ms
     × rejects a damaged (truncated) video with 422 corrupt_video and cleans up 714ms
     × answers 500 and keeps the upload when ffprobe is missing 596ms
 FAIL  test/media.test.ts > media > maps a non-zero ffmpeg exit on a damaged file to corrupt_video
AssertionError: expected Error: ffmpeg failed: [out#0/image2 @ 0x5… to be an instance of MediaError
 FAIL  test/media.test.ts > media > reports a missing binary as a non-media error
AssertionError: expected MediaError: not_a_video { code: 'not…' } to not be an instance of MediaError
 FAIL  test/uploads.test.ts > ... > rejects a damaged (truncated) video with 422 corrupt_video and cleans up
AssertionError: expected 500 to be 422 // Object.is equality
 FAIL  test/uploads.test.ts > ... > answers 500 and keeps the upload when ffprobe is missing
AssertionError: expected 422 to be 500 // Object.is equality
      Tests  4 failed | 16 passed (20)
```
**GREEN**
```
 ✓ test/media.test.ts > media > maps a non-zero ffmpeg exit on a damaged file to corrupt_video 140ms
 ✓ test/media.test.ts > media > reports a missing binary as a non-media error 48ms
 ✓ test/uploads.test.ts > uploads and finalize > rejects a damaged (truncated) video with 422 corrupt_video and cleans up 714ms
 ✓ test/uploads.test.ts > uploads and finalize > answers 500 and keeps the upload when ffprobe is missing 663ms
      Tests  20 passed (20)
```

## 2. Finalize safely retryable after a lost response (tombstones)
**Changes**
- `server/src/db/schema.ts`: `uploads.clipId` = nullable `clip_id text`, FK → clips.id ON DELETE CASCADE. When a clip is deleted, its tombstone goes with it and a retry gets 404. New migration `server/src/db/migrations/0003_uploads_clip_tombstone.sql` was generated with `pnpm drizzle-kit generate --name uploads_clip_tombstone`, with its snapshot and journal entry. No old migrations were edited.
- `server/src/clips/routes.ts` POST /api/clips:
  - If the owned upload already has `clipId`, it answers 200 with `toClip(...)` for that clip (same body shape as the 201).
  - The claim UPDATE also requires `clip_id IS NULL`. If the claim fails, the row is re-read: a tombstone gives 200 with the existing clip, a vanished row gives 404, and an in-progress claim gives 409 `upload_finalizing`.
  - On success, the clip INSERT and the tombstone UPDATE (`clip_id = id, finalizing_at = now()`, guarded by `clip_id IS NULL`) run in one transaction. If the guard matches 0 rows (another finalize took over a stale claim and won), the transaction rolls back, this request's outputs are deleted, and the response is the winner's clip. So there is never a second clip.
  - After success only the tus files are removed (`removeTusFiles`). The row stays.
- `server/src/uploads/tus.ts` `cleanupStaleUploads`: unfinished rows age from `created_at` (unchanged). Tombstones age from `finalizing_at`, which is the finalize time, so a retry always has a full 24 h window.

**Tests** (uploads.test.ts)
- `uploads fully then finalizes into a clip` (updated: expects one tombstone row `{id, clipId: clip.id}`)
- `a repeated finalize after success returns the same clip with 200 (lost-response retry)`: 200 with a body equal to the first 201, one clip row, another user gets 404, and after the clip is deleted the tombstone cascades and a retry gets 404
- `removes finalized tombstones after the max age`: kept at +1 h, removed at +25 h, then 404
- `is race-safe: concurrent finalizes of one upload yield one clip, never a 422` (updated): exactly one 201. Losers get 409 `upload_finalizing` or 200 with the same clip id. One clip row. A later retry gives 200 with the same id.

**RED**
```
     × uploads fully then finalizes into a clip 894ms
     × a repeated finalize after success returns the same clip with 200 (lost-response retry) 659ms
     × removes finalized tombstones after the max age 647ms
     × is race-safe: concurrent finalizes of one upload yield one clip, never a 422 689ms
 FAIL  ... > uploads fully then finalizes into a clip
TypeError: Cannot convert undefined or null to object      (uploads.clipId column does not exist yet)
 FAIL  ... > a repeated finalize after success returns the same clip with 200 (lost-response retry)
AssertionError: expected 404 to be 200 // Object.is equality
 FAIL  ... > removes finalized tombstones after the max age
AssertionError: expected [] to have a length of 1 but got +0
 FAIL  ... > is race-safe: concurrent finalizes of one upload yield one clip, never a 422
AssertionError: expected 404 to be 200 // Object.is equality
      Tests  4 failed | 10 passed (14)
```
**GREEN**
```
 ✓ test/uploads.test.ts > uploads and finalize > uploads fully then finalizes into a clip 914ms
 ✓ test/uploads.test.ts > uploads and finalize > a repeated finalize after success returns the same clip with 200 (lost-response retry) 665ms
 ✓ test/uploads.test.ts > uploads and finalize > removes finalized tombstones after the max age 661ms
 ✓ test/uploads.test.ts > uploads and finalize > is race-safe: concurrent finalizes of one upload yield one clip, never a 422 696ms
      Tests  14 passed (14)
```

## 3. Delete on the share page for owner/admin
**Changes**
- `server/src/web/pages/share.tsx`: the actions are wrapped in `<div class="actions" data-clip-id=...>`. When `canDelete` is set, a `<button class="btn danger" data-action="delete" data-redirect="/">Delete</button>` is rendered.
- `server/src/web/routes.ts` /c/:id: `canDelete = viewer is the owner || viewer.isAdmin`. Logged-out viewers never get it.
- `server/public/app.js`: the existing delete handler (with its confirm) navigates to `safeNext(btn.dataset.redirect)` after a successful DELETE when the button has `data-redirect`. Otherwise it removes the row as before.

**Test** (share.test.ts): `shows Delete to the owner and admins only`. The owner and an admin see the delete button inside the data-clip-id wrapper. Another user and an anonymous viewer do not.

**RED**
```
 × shows Delete to the owner and admins only 535ms
 FAIL  test/share.test.ts > share page > shows Delete to the owner and admins only
AssertionError: expected '<!doctype html><html lang="en"><head>…' to match /<div[^>]*data-clip-id="shareclip005"…/"
      Tests  1 failed | 4 passed (5)
```
**GREEN**
```
 ✓ test/share.test.ts > share page > shows Delete to the owner and admins only 537ms
      Tests  5 passed (5)
```

## 4. .gitignore .env
**Change:** `.gitignore` adds `.env`, `.env.*`, `!.env.example` (no .env.example exists today; the rule keeps it trackable).
**Check** (`git check-ignore`, no test framework applies):
```
RED:   .env NOT ignored / server/.env NOT ignored / .env.production NOT ignored / .env.example NOT ignored
GREEN: .env ignored / server/.env ignored / .env.production ignored / .env.example NOT ignored
```

## 5–10. Hardening batch
**Changes**
- (5) User strings in attributes now go through `e()`. In me.tsx: `data-title`, `data-url`. In components.tsx: game chip `title`/`alt`, plus the chip and pager `href` so the `&` separators are encoded. In invite.tsx: hidden `inviteCode` value (it comes from the URL). In admin.tsx: `data-code`, `data-url`. kitajs only escapes `"` in attribute values, so a stored `R&amp;D` used to decode as `R&D`.
- (6) `safeNext` (routes.ts, and the matching copy in app.js) now allows only printable ASCII `[\x20-\x7e]` minus backslash after a single leading `/` (still no `//` or `/\`). `/login?next=%2F%E2%82%AC` for a logged-in user now redirects to `/` instead of failing.
- (7) tus `onIncomingRequest`: every non-POST method (HEAD/PATCH/DELETE; tus doesn't call the hook for OPTIONS) throws 404 unless an `uploads` row with that exact id exists, is owned by the caller, and is not a tombstone. Unknown ids like `<id>.json` never reach the FileStore.
- (8) README.md line 3: "or the website" removed.
- (9) `@fastify/formbody` removed (`pnpm --filter @klipt/server remove`, lockfile updated). Nothing imported it.
- (10) `app.setErrorHandler` is registered in `buildApp` before any plugin or route. Errors with `statusCode < 500` are passed through `reply.send(err)` to Fastify's default handler, so the zod 400s (sent directly by handlers), bad-JSON 400, 413 and rate-limit 429 keep their current shapes. 5xx errors are logged with `req.log.error`. `/api/*` and `/media/*` answer `500 {"error":"internal_error"}`. Other paths render the new `server/src/web/pages/serverError.tsx` HTML page ("Something went wrong", `cache-control: no-store`). err.message is never sent. Because Fastify's logger was off everywhere, logging would have done nothing in production. `AppDeps` gained `logger?: boolean` (default false, so tests stay quiet) and `main.ts` passes `logger: true`.

**Tests**
- web.test.ts: `drops non-ASCII and non-printable next paths instead of failing the redirect`, `escapes & in attributes so user strings round-trip`
- uploads.test.ts: `answers 404 to tus requests for ids without an owned upload row (no store fall-through)`. Bob's DELETE /api/uploads/<A-id>.json gets 404 and A's sidecar is intact. A's own DELETE on `<id>.json` also gets 404.
- errors.test.ts (new): `answers API errors with a generic internal_error`, `renders an HTML 500 page for page routes`, `keeps 4xx responses unchanged (validation, bad JSON, rate limit)`. The last one is a guard. It passed on the baseline by design and still passes after the handler was added.
- README/formbody: checked with grep (`grep -rn formbody` finds nothing outside docs; README line 3 updated).

**RED** (`pnpm exec vitest run test/web.test.ts test/uploads.test.ts test/errors.test.ts`)
```
     × answers API errors with a generic internal_error
     × renders an HTML 500 page for page routes
     ✓ keeps 4xx responses unchanged (validation, bad JSON, rate limit)   <- guard, passes on baseline by design
     × drops non-ASCII and non-printable next paths instead of failing the redirect
     × escapes & in attributes so user strings round-trip
     × answers 404 to tus requests for ids without an owned upload row (no store fall-through)
 FAIL  test/errors.test.ts > error handler > answers API errors with a generic internal_error
AssertionError: expected { statusCode: 500, …(2) } to deeply equal { error: 'internal_error' }
+   "error": "Internal Server Error",   (default handler leaked message: 'secret db detail')
 FAIL  test/errors.test.ts > error handler > renders an HTML 500 page for page routes
AssertionError: expected 'application/json; charset=utf-8' to contain 'text/html'
 FAIL  test/uploads.test.ts > ... > answers 404 to tus requests for ids without an owned upload row (no store fall-through)
AssertionError: expected 204 to be 404 // Object.is equality      (bob's DELETE deleted alice's sidecar)
 FAIL  test/web.test.ts > web pages > drops non-ASCII and non-printable next paths instead of failing the redirect
AssertionError: "/€": expected '/€' to be '/' // Object.is equality
 FAIL  test/web.test.ts > web pages > escapes & in attributes so user strings round-trip
AssertionError: expected '<!doctype html>…' to contain 'data-title="R&amp;amp;D"'
   (rendered: data-title="R&amp;D" ... title="A&amp;B" ... alt="A&amp;B")
```
**GREEN**
```
 ✓ test/errors.test.ts > error handler > answers API errors with a generic internal_error 903ms
 ✓ test/errors.test.ts > error handler > renders an HTML 500 page for page routes 583ms
 ✓ test/errors.test.ts > error handler > keeps 4xx responses unchanged (validation, bad JSON, rate limit) 660ms
 ✓ test/web.test.ts > web pages > drops non-ASCII and non-printable next paths instead of failing the redirect 530ms
 ✓ test/web.test.ts > web pages > escapes & in attributes so user strings round-trip 548ms
 ✓ test/uploads.test.ts > uploads and finalize > answers 404 to tus requests for ids without an owned upload row (no store fall-through) 609ms
      Tests  31 passed (31)
```

## Final verification
`pnpm --filter @klipt/server test`
```
 Test Files  11 passed (11)
      Tests  83 passed (83)
   Start at  22:11:02
   Duration  14.66s (tests 94%, import 6%)

```
The suite output is clean: no warnings or stderr noise.

`pnpm --filter @klipt/server exec tsc --noEmit`
```
tsc --noEmit: clean (exit 0)
```

## Files changed
.gitignore, README.md, pnpm-lock.yaml, server/package.json, server/public/app.js, server/src/app.ts, server/src/main.ts, server/src/clips/media.ts, server/src/clips/routes.ts, server/src/db/schema.ts, server/src/db/migrations/0003_uploads_clip_tombstone.sql (+ meta/0003_snapshot.json, meta/_journal.json), server/src/uploads/tus.ts, server/src/web/components.tsx, server/src/web/routes.ts, server/src/web/pages/{admin,invite,me,share}.tsx, server/src/web/pages/serverError.tsx (new), server/test/{helpers,media.test,uploads.test,share.test,web.test}.ts, server/test/errors.test.ts (new)

## Concerns / notes
- Enabling the Fastify logger in production (`main.ts` → `buildApp({ ..., logger: true })`) goes slightly beyond finding 10 as written. Without it, "log the error" (and main.ts's existing `app.log` calls) did nothing in production.
- If ffmpeg dies from a signal (for example SIGSEGV on hostile input), `err.code` is null, so per the finding it stays a 500 and the upload is kept. A crashing input would still be retried until stale cleanup removes it.
- The redirect after deleting from the share page lives in app.js. There is no browser test harness, so it is checked only by `node --check` and by the server-side HTML test.
- The corrupt fixture (truncating a faststart remux at 6 KB) depends on how the local ffmpeg build behaves. It reproduces exactly the reviewer's case here (thumbnail exit 234).
- The 422 path still deletes the upload row (no tombstone), so a client that loses a 422 response gets 404 when it retries.
- The migration adds an FK `uploads.clip_id → clips.id ON DELETE CASCADE`.
