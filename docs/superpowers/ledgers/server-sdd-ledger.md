# SDD ledger — plan: docs/superpowers/plans/2026-11-09-klipt-server.md
Spec: docs/superpowers/specs/2026-11-09-medal-alternative-design.md
Branch: feat/server (worktree .worktrees/server), base a29fe7f

## Preflight scan
| Tasks | Shared file/interface | Finding |
|---|---|---|
| T1→T2 | buildApp deps.db (unknown → Db) | consistent |
| T1→T2 | health.test uses makeTestApp after T2 | consistent |
| T2→T6 | makeTestApp() vs makeTestApp({maxUploadMb}) | signature ambiguity → Ruling R1 |
| T2→T10 | runMigrations folder in src vs dist | path ambiguity → Ruling R2 |
| T3→T4,T6-T9 | loginAs + 10/min login rate limit | later tests may exceed limit → Ruling R3 |
| T3→T4 | auth/routes.ts (register added in T4) | consistent |
| T5→T6 | MediaError, probeVideo, assertPlayable, remuxFaststart, makeThumbnail | consistent |
| T6→T7 | clips/routes.ts, Clip type | consistent |
| T6 | tusUpload via app.inject → tus.handle(req.raw,res.raw) | risk only; implementer may use app.listen + fetch if inject can't stream |
| T7→T8 | listClips/listGames(viewer AuthUser) | consistent |
| T8→T9 | notFound.tsx created T8, used T9 | consistent |
| T8 | @fastify/formbody in stack but pages use app.js fetch | Ruling R4 |
| All | module system unspecified | Ruling R5 |
| Each task self-consistency | tests vs code vs files | T1–T10 consistent |

Ruling R1: makeTestApp(opts?: { maxUploadMb?: number }) defined in T2 — plan text mentions both forms — cost if wrong: trivial helper change.
Ruling R2: runMigrations(db, folder = process.env.MIGRATIONS_DIR ?? <path resolved relative to the module>); T10 Dockerfile sets MIGRATIONS_DIR — prod and tests both work — cost: small path fix.
Ruling R3: helpers add sessionFor(db, userId, kind) via createSession for tests not exercising login itself; loginAs only for login tests — avoids rate-limit flakiness — cost: none.
Ruling R4: login/register forms submit via app.js fetch to JSON API; @fastify/formbody optional (drop if unused) — keeps one auth code path — cost: none.
Ruling R5: ESM everywhere ("type":"module", NodeNext), Node 22 — modern default — cost: config churn.

## Progress
Task 1: dispatched implementer (sonnet), BASE a29fe7f
Task 1: ⚠️ .gitignore resolved — exists at base a29fe7f (controller-created), not a gap
Task 1: minor (deferred): @types/node ^26 vs Node 22 target — pin ^22
Task 1: minor (deferred): no packageManager/engines field in root package.json (matters for T10 Dockerfile)
Task 1: minor (deferred): .gitignore lacks .env and local data dir entries
Task 1: complete (commits a29fe7f..208a810, review clean)
Task 2: dispatched implementer (sonnet), BASE 208a810
Task 2: review → 1 Important (db.test asserts any error, not unique violation); fix round 1 dispatched (resume implementer); FIX_BASE 0710f77
Task 2: ⚠️ runMigrations untested on node-postgres — covered by Task 10 docker smoke test (login after migrate + restart)
Task 2: minor (deferred): createDb doesn't expose pg.Pool (no close / no pool error handler)
Task 2: minor (deferred): makeTestApp.close() doesn't close PGlite
Task 2: minor (deferred): runMigrations relies on drizzle internals (dialect/session)
Task 2: minor (deferred): no DB CHECK constraints for kind/visibility
Task 2: fix round 1/5 (1 addressed, 0 open; commits 0710f77..b23e54f)
Task 2: complete (commits 208a810..b23e54f, review clean)
Task 3: dispatched implementer (sonnet), BASE b23e54f
Task 3: review (opus) → 1 Important: trustProxy:true lets forged X-Forwarded-For bypass login rate limit (reproduced). FIX_BASE 0e5d81d
Task 3: Ruling: trustProxy:true (plan T1 Interfaces) → configurable hop count TRUST_PROXY_HOPS default 1 — spec only requires trusting the reverse proxy; true lets clients forge req.ip — cost if wrong: if a second proxy (e.g. Cloudflare) sits in front, rate limit keys on proxy IP until owner sets TRUST_PROXY_HOPS=2
Task 3: fix round 1 dispatched (resume implementer)
Task 3: minor (deferred): auth.test tokenHash assertion weak (assert === sha256(token))
Task 3: minor (deferred): no tests for lastUsedAt throttle, requireAdmin, revokeAllForUser
Task 3: minor (deferred): sessions list shows expired web sessions; no cleanup of expired rows
Task 3: minor (deferred): DELETE own current session leaves dead cookie
Task 3: minor (deferred): 429 body shape differs from {error:code} (use errorResponseBuilder → rate_limited)
Task 3: minor (deferred): login body defined twice (TS interface in shared + zod in routes)
Task 3: minor (deferred): blank deviceName stored as "" not null
Task 3: minor (deferred): sessions.ts:320 sql`` wrapper/+1ms boundary unclear; auth.test dynamic imports of users
Task 3: fix round 1/5 (1 addressed, 0 open; commits 0e5d81d..445c149) — trustProxy fn (_addr,hop)=>hop<hops verified against proxy-addr semantics
Task 3: carry to Task 10: document TRUST_PROXY_HOPS (default 1 = behind Traefik; 0 when no proxy) in compose/README
Task 3: complete (commits b23e54f..445c149, review clean)
Task 4: dispatched implementer (sonnet), BASE 445c149
Task 4: review (opus) → 2 Important: disable test can't detect missing revokeAllForUser; cookie options duplicated. FIX_BASE 1a350c3. Fix round 1 dispatched (resume) incl. tx for disable+revoke (from minor 2, same code path)
Task 4: minor (deferred): concurrency test on PGlite doesn't truly test concurrency (add comment)
Task 4: minor (deferred): ensureAdmin doesn't validate env username/password against rules
Task 4: minor (deferred): coverage gaps (non-admin 403 for PATCH user/DELETE invite; register rate limit; username length bounds; invalid_invite body on revoked/nonexistent)
Task 4: minor (deferred): class Fail declared inside handler; invalid_request vs specific codes for missing username/oversize password; cannot_disable_self also on enable
Task 4: fix round 1/5 (2 addressed, 0 open; commits 1a350c3..4f089e8)
Task 4: complete (commits 445c149..4f089e8, review clean)
Task 5: dispatched implementer (sonnet), BASE 4f089e8
Task 5: minor (deferred): missing ffprobe binary (ENOENT) maps to not_a_video (4xx) instead of 500
Task 5: minor (deferred): no execFile timeouts (crafted file could hang finalize)
Task 5: minor (deferred): no leading '-' guard on paths; thumbnail width untested; ffmpeg failure paths untested; ?? vs || env fallback
Task 5: complete (commits 4f089e8..de12013, review clean)
Task 6: dispatched implementer (sonnet), BASE de12013
Task 6: review (opus) → 2 Important: finalize not race-safe/idempotent; catch deletes media of committed clip. FIX_BASE a8a64d2. Fix round 1 dispatched (resume)
Task 6: Ruling: tusUpload uses app.listen+fetch instead of app.inject (plan text) — inject can't drive @tus/server PATCH (light-my-request + srvx web stream; reviewer reproduced) — cost if wrong: tus tests need a loopback port
Task 6: minor (deferred, FLAG FOR FINAL): unknown ids fall through to tus store — DELETE /api/uploads/<A-id>.json by B could unlink A's sidecar (needs 128-bit id); fix: 404 for non-POST unless owned row exists
Task 6: minor (deferred): tus.handle errors after hijack unhandled; bodyLimit on tus routes ineffective; gameName "" → 400 vs null; ownership tests only HEAD; 401 only on POST; fixture temp dirs leak; helper doc says inject
Task 6: fix round 1/5 (2 addressed, 0 open; commits a8a64d2..0ebf95b); controller verified suite 44/44 (report lacked output)
Task 6: minor (deferred, FLAG FOR FINAL): no fencing after 30-min claim TTL + no execFile timeouts → possible duplicate if ffmpeg hangs >30min (fix: execFile timeout < TTL)
Task 6: minor (deferred): mkdir(clipsDir) after claim outside try → 30-min lock on failure; failed post-insert removeUpload could allow duplicate after TTL; cleanupStaleUploads ignores finalizingAt
Task 6: complete (commits de12013..0ebf95b, review clean)
Task 7: dispatched implementer (sonnet), BASE 0ebf95b
Task 7: review (opus) → 1 Important: cursor ms vs Postgres µs precision skips rows (reproduced). FIX_BASE 5b4ffa7. Fix round 1 dispatched (resume)
Task 7: Ruling: fix by clips.created_at precision 3 (migration) rather than text cursor — makes JS Date round-trip exact — cost if wrong: sub-ms ordering lost (irrelevant for clips)
Task 7: carry to Task 8: listClips returns ClipWithOwner[] ({clip, owner}) not ClipRow[]; @fastify/static already registered at root by clips media (decorates reply.sendFile) → Task 8 must register public/ static with decorateReply:false (or scope it)
Task 7: minor (deferred): limit>60 → 400 in route vs clamp in repo; doesnotexist id hits regex not DB; no 404 tests for PATCH/DELETE unknown; owner self-delete doesn't assert files removed; gameName null leaves steamAppId; DELETE rm error after row delete → 500
Task 7: fix round 1/5 (1 addressed, 0 open; commits 5b4ffa7..5f51006)
Task 7: complete (commits 0ebf95b..5f51006, review clean)
Task 8: dispatched implementer (sonnet), BASE 5f51006
Task 8: carry to Task 10: Dockerfile must copy server/public (publicDir checks PUBLIC_DIR, ../../public, ../public, cwd/public)
Task 8: review (opus) → 1 Important: admin/session queries duplicated from API routes. FIX_BASE e980ec2. Fix round 1 dispatched (resume) incl. shared invite-validity predicate
Task 8: minor (deferred, FLAG FOR FINAL): attributes not &-escaped (kitajs escapes only ") → rename prompt/alt text entity-decoded (title fidelity bug)
Task 8: minor (deferred, FLAG FOR FINAL): logged-in /login?next=%2F%E2%82%AC → 500 (non-Latin1 Location); fix encodeURI or ASCII-only safeNext
Task 8: minor (deferred): NotFoundPage message unescaped; Safari clipboard after await on invite create; visibility button label unclear; raw error codes; no empty state; logged-in /invite shows signup; test gaps (logged-out /u /admin, pager, owner /u shows link clips); custom escape.ts instead of kitajs helpers
Task 8: fix round 1/5 (1 addressed, 0 open; commits e980ec2..f308b34)
Task 8: manual browser check (brief Step 5) deferred to controller after Task 10 (docker smoke)
Task 8: complete (commits 5f51006..f308b34, review clean)
Task 9: dispatched implementer (sonnet), BASE f308b34
Task 9: minor (deferred): logged-in test weak (class="who"); escape test not pinned to og:title / no ' case; import order; web imports toClip from API route module; no Cache-Control on /c/:id; twitter:card player without twitter:player
Task 9: complete (commits f308b34..3d7cb7d, review clean)
Task 10: dispatched implementer (sonnet), BASE 3d7cb7d
Task 10: review (opus) → 1 Important: POSTGRES_PASSWORD raw in DATABASE_URL breaks on reserved chars. FIX_BASE 702c007. Fix round 1 dispatched (resume) + package.json copy (minor 1, same file, one line)
Task 10: Ruling: implementer made PGlite import dynamic in db/client.ts (outside brief) — prod container crashed on devDependency import; reviewer confirmed sound — cost if wrong: none
Task 10: minor (deferred): Dockerfile layer caching (copy manifests first); .dockerignore **/node_modules **/dist; shutdown re-entry guard/timeout + pool.end; README backup command concrete; README "8 chars" vs ensureAdmin not enforcing
Task 10: fix round 1/5 (2 addressed, 0 open; commits 702c007..b911b81)
Task 10: complete (commits 3d7cb7d..b911b81, review clean)
FINAL: dispatching whole-branch review (opus), MERGE_BASE a29fe7f
FINAL: review (opus) → 4 Important (corrupt video 500; finalize retry tombstone; admin delete on share page; .env gitignore) + 6 hardening one-liners → single fix wave dispatched, FIX_BASE b911b81
