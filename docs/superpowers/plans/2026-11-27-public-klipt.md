# Public Klipt Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Open sign-up on klipt.adamko.tech with mutual friends, single-use referral links, Link
only / Friends / Public clips, Public / Friends profiles, a logged-out showcase page, and a desktop
app with the server built in.

**Architecture:**
- **Server:** a `friendships` table plus one SQL helper (`canSee`) used by every clip list in
  `server/src/clips/repo.ts`.
- **Referrals:** rows in the existing `invites` table, made by any user.
- **Showcase:** a server-rendered TSX page at `/` for logged-out visitors.
- **Desktop:**
  - The Rust enum and the Svelte UI learn the three visibility levels.
  - A Friends screen.
  - A built-in default server.

**Tech Stack:** Fastify 5, Drizzle + Postgres (pglite in tests), @kitajs/html TSX pages, vitest,
Tauri 2 (Rust) + Svelte 5, `packages/shared` types.

**Spec:** `docs/superpowers/specs/2026-11-27-public-klipt-design.md` (read it first. "Who sees what"
is the contract for Task 3).

## Global Constraints

- One migration: `server/src/db/migrations/0017_*.sql`. Generate it with `pnpm drizzle-kit generate`
  from `server/`, then hand-edit the data statements into it.
- Visibility values: `'link' | 'friends' | 'public'`. The server accepts `'feed'` on input as an
  alias for `'friends'`, forever. Responses never send `'feed'`.
- Profile visibility: `'friends' | 'public'`, default `'friends'`.
- Referral links expire 14 days after creation, work once, and have no per-user limit.
- Referral cookie: `klipt_ref`, httpOnly, sameSite lax, 14 days, secure when `config.secureCookies`.
- Default server: `https://klipt.adamko.tech`. The dev override env var is `KLIPT_SERVER`.
- Showcase headline, exactly: **"Clips without bullshit."**
- Showcase images show only the owner's own clips, names and avatar.
- Never run prettier. Match the style around each edit.
- Server tests: `cd server && pnpm test`. Desktop UI tests: `cd desktop && pnpm test`. Rust:
  `cd desktop/src-tauri && cargo test`. Typecheck: `pnpm -r typecheck` from the root, if present.
- Commit messages end with `Claude-Session: https://claude.ai/code/session_01REby7VkkFk3UUfnGb1fzNv`.

## Review Focus

1. **A stranger with a fresh account** must get zero rows from someone else's Friends clips:
   - on the Feed
   - on profiles, tag lists and search
   - in game chips, profile stats and `/api/users`
   - in favorites (a clip starred while they were friends)

   Test owner: Task 3 `stranger sees nothing` sweep.
2. **Existing users after the migration** are all friends with each other, and old `'feed'` clips
   show as Friends. Test owner: Task 1, the backfill SQL test.
3. **An old app** sends `visibility: 'feed'` on finalize and on PATCH, and gets a Friends clip.
   Test owner: Task 3, the `feed alias` test.
4. **A used, expired or own referral code** at sign-up still creates the account. It does not make a
   friendship, and the response says `referral: 'invalid'`. Test owner: Task 5.
5. **A pending request** grants nothing. Neither side sees the other's Friends clips until it's
   accepted. Test owner: Task 3, the `pending` row.

---

### Task 1: Schema, migration and shared types

**Files:**
- Modify: `server/src/db/schema.ts`
  - `users`: add `profileVisibility`.
  - `clips.visibility`: enum becomes `['link','friends','public']`, default `'friends'`.
  - New `friendships` table.
- Create: `server/src/db/migrations/0017_*.sql` (generated, then edited), plus its meta snapshot.
- Modify: `packages/shared/src/api.ts`:
  - `Visibility = 'link' | 'friends' | 'public'`.
  - Add `ProfileVisibility = 'friends' | 'public'`.
  - `Me.profileVisibility: ProfileVisibility`.
- Test: `server/test/db.test.ts`

**Interfaces:**
- Produces: the Drizzle table `friendships`. Its columns:
  - `userA uuid`, `userB uuid`: FK users with cascade delete, stored with `userA < userB`.
  - `requestedBy uuid`
  - `status text enum ['pending','accepted']`
  - `createdAt`, `acceptedAt` (nullable)
  - Primary key (`userA`, `userB`); index on `userB`.
- Produces: `users.profileVisibility`, the TS types above.

- [ ] **Step 1: Write the failing test `backfill makes every active pair friends`** in `db.test.ts`.
  1. Read the 0017 SQL file.
  2. Split it on `--> statement-breakpoint`.
  3. Pick the statements that contain the marker comments `-- klipt:backfill-friends` and
     `-- klipt:feed-to-friends`.
  4. Seed three active users and one disabled user, and run the backfill statement.

  Expect exactly 3 rows in `friendships`, all `status='accepted'`, each with `userA < userB`, and
  none involving the disabled user.

  Then, on a fresh DB, test the feed statement:
  1. Insert a clip with raw SQL `visibility='feed'`.
  2. Run the feed-to-friends statement.
  3. Expect `'friends'`.
- [ ] **Step 2: Run it.** `cd server && pnpm vitest run test/db.test.ts`. Expected: FAIL, because
  the file doesn't exist.
- [ ] **Step 3: Edit the schema, run `pnpm drizzle-kit generate`, then append the data statements
  to the generated SQL.** Each data statement is preceded by its marker comment:
  - `update clips set visibility = 'friends' where visibility = 'feed'`
  - The backfill:

    ```sql
    insert into friendships (user_a, user_b, requested_by, status, accepted_at)
    select a.id, b.id, a.id, 'accepted', now()
    from users a join users b on a.id < b.id
    where a.disabled_at is null and b.disabled_at is null
    on conflict do nothing
    ```

  Make sure the column default for `clips.visibility` becomes `'friends'`.
- [ ] **Step 4: Update the shared types.** Fix compile errors where `'feed'` was a visibility
  (server `clips/routes.ts` zod enums: see Task 3; for now make them
  `z.enum(['link','friends','public','feed'])`).
- [ ] **Step 5: Run `pnpm vitest run test/db.test.ts`.** Expected: PASS. Run the full server
  suite. Failures caused only by `'feed'` assertions get updated to `'friends'`.
- [ ] **Step 6: Commit.** `feat(server): friendships table, three visibility levels, profile visibility`

### Task 2: Friends repo and API

**Files:**
- Create: `server/src/friends/repo.ts`, `server/src/friends/routes.ts`
- Modify: `server/src/app.ts` (register `friendRoutes`)
- Modify: `packages/shared/src/api.ts`: `FriendList`, `FriendState`
- Test: `server/test/friends.test.ts`

**Interfaces:**
- Produces, in `friends/repo.ts`:
  - `pairOf(a: string, b: string): [string, string]`: the sorted pair.
  - `friendsExpr(viewerId: SQL | string, ownerCol: SQL | Column): SQL<boolean>`: `exists(...)`
    for an accepted row between them. Task 3 builds on this.
  - `friendState(db, viewerId, otherId): Promise<FriendState>`, where
    `FriendState = 'none' | 'outgoing' | 'incoming' | 'friends' | 'self'`.
  - `requestFriend(db, fromId, toId): Promise<FriendState>`:
    - no row: insert pending, return `'outgoing'`
    - an incoming pending row: accept it, return `'friends'`
    - an existing outgoing or accepted row: no change
  - `removeFriend(db, a, b): Promise<void>`: deletes the row, whatever its state.
  - `makeFriends(tx, a, b): Promise<void>`: upserts the row as accepted (used by referrals).
  - `listFriends(db, userId): Promise<FriendList>`. The shape is
    `{ friends: UserRef[]; incoming: UserRef[]; outgoing: UserRef[] }`, each sorted by username key.
- Routes, all `requireUser`:
  - `GET /api/friends` → `FriendList`.
  - `POST /api/friends/:username` → `{ state: FriendState }`. 404 `unknown_user` for a missing or
    disabled user, 400 `self` for yourself.
  - `DELETE /api/friends/:username` → 204.

- [ ] **Step 1: Write failing tests:**
  - `request then accept makes friends`: A posts B → `outgoing`. B's `GET` lists A under `incoming`.
    B posts A → `friends`. Both lists show the other under `friends`.
  - `delete declines, cancels and unfriends`.
  - `cannot friend yourself or unknown users` (400 / 404).
  - `friends of a disabled user are not listed`.
- [ ] **Step 2: Run them.** `pnpm vitest run test/friends.test.ts`. Expected: FAIL.
- [ ] **Step 3: Implement the repo and routes.** Use the `userRefColumns`/`toUserRef` from
  `users/ref.ts` for list items.
- [ ] **Step 4: Run them.** Expected: PASS.
- [ ] **Step 5: Commit.** `feat(server): friend requests (send, accept, decline, unfriend)`

### Task 3: Who sees what (clip lists, tags, favorites, albums, uploads)

**Files:**
- Modify: `server/src/clips/repo.ts`:
  - `scopeCondition`, `listClips`, `getProfileStats`, `listGames`: the viewer becomes
    `AuthUser | null`.
  - `resolveTags`
  - `listUsernames` → `listFriendRefs`
- Modify: `server/src/clips/routes.ts`:
  - visibility zod enums, with the `'feed'` alias mapped to `'friends'`
  - finalize default = uploader's `profileVisibility`
  - `/api/users` → the viewer's friends
- Modify: `server/src/albums/repo.ts`: `assertClips` takes the viewer and checks `canSee`.
- Test: `server/test/visibility.test.ts` (new); adjust `clips.test.ts`, `favorites.test.ts` and
  `albums.test.ts` where they assumed everyone sees Feed clips.

**Interfaces:**
- Consumes: `friendsExpr` (Task 2).
- Produces:
  - `canSee(viewerId: string | null): SQL`: a clip row is visible in lists when one of these holds:
    - the uploader is the viewer
    - `visibility='public'`
    - `visibility='friends'` and `friendsExpr(viewer, clips.userId)`
  - `listClips(db, viewer: AuthUser | null, q)`. Logged-out callers can only use `scope:'user'`.
  - `listFriendRefs(db, userId): Promise<UserRefRow[]>`
  - `normalizeVisibility(v: 'link'|'friends'|'public'|'feed'): Visibility`

Rules (the spec's table, made exact):
- **Feed:** `uploader = viewer OR (visibility <> 'link' AND friendsExpr)`. Public clips from
  non-friends are not on the Feed.
- **Profile `user` scope, own clips:** self → everything. Others → `canSee` minus `'link'`.
- **Profile `user` scope, tagged-in part:** included only when all of these hold:
  - the profile is visible in full to the viewer
  - `canSee` holds for the viewer
  - the uploader and the profile owner are friends

  A profile is visible in full when it's yours, it's Public, or you're friends with its owner.
- **Friends profile seen by a non-friend or logged out:** only the owner's `public` clips. No
  tagged part. Stats are computed over just those clips (the page hides the stats; Task 4).
- **Favorites list:** `favoriteExpr AND (canSee OR visibility='link')`. Starred link-only clips stay;
  a Friends clip drops out after an unfriend.
- **Search, game chips and `withMe`:** they AND onto the scope above, which is already the case.
- **`resolveTags`:** every name must be an accepted friend of the owner. A non-friend throws
  `UnknownUserError` (the same message as an unknown name, so it doesn't reveal who exists).
- **`assertClips`:** the adder can see the clip (`canSee` or uploader = adder or `visibility='link'`).
  Link-only clips of others can still be added, since the adder has the link.

- [ ] **Step 1: Write failing tests in `visibility.test.ts`.**

  Set up users `me`, `friend`, `pending` (a request from me, not accepted), `stranger` and
  `pubOwner` (a Public profile). Each uploads one clip at each of `link`/`friends`/`public` (insert
  rows directly). Then test:
  - `feed shows own and friends' non-link clips only`: me's Feed = me×3 + friend's friends+public.
  - `pending grants nothing`: nothing from `pending` on me's Feed, and the reverse.
  - `stranger sees nothing` sweep, from stranger's view:
    - the Feed has only stranger's own clips
    - me's profile (Friends) lists only me's public clip
    - `listGames` over me's profile shows only that clip's game
    - `/api/users` is empty
    - after stranger stars me's friends clip and then is not a friend, the favorites list doesn't
      have it
  - `logged out sees public clips of any profile`: `listClips(db, null, {scope:'user', username:'me'})`
    = me's public clip only.
  - `tagged clip shows on profile only when uploader and tagged are friends`.
  - `tagging a non-friend is refused` (PATCH → 400, `UnknownUserError` message).
  - `feed alias`: finalize with `visibility:'feed'` and PATCH with `'feed'` store `'friends'`.
    Responses say `'friends'`.
  - `upload default follows profile visibility`: a `public` profile with no visibility in finalize
    gives a `public` clip.
- [ ] **Step 2: Run them.** `pnpm vitest run test/visibility.test.ts`. Expected: FAIL.
- [ ] **Step 3: Implement the repo and route changes.**
- [ ] **Step 4: Run the full server suite.** `pnpm test`. Expected: PASS, after updating old tests
  whose fixtures relied on everyone seeing everyone. In those, make the users friends with
  `makeFriends` instead of weakening the assertions.
- [ ] **Step 5: Commit.** `feat(server): clips are listed to friends (or everyone when Public); tags only between friends`

### Task 4: Profile visibility and public profiles on the web

**Files:**
- Modify: `server/src/auth/routes.ts` (`updateMeBody.profileVisibility`), `server/src/users/ref.ts`
  (`meOf` returns `profileVisibility`)
- Modify: `server/src/web/routes.ts`: `/u/:username` no longer guards. Pass `friendState` and
  `fullProfile`.
- Modify: `server/src/web/pages/user.tsx`:
  - limited view (name, avatar, public clips, Add friend or "Log in to add")
  - friend button states: Add friend / Requested / Accept / Friends ▾ Unfriend
- Modify: `server/src/web/pages/account.tsx`: Profile Public/Friends radio, with the line "Also the
  default for new uploads."
- Modify: `server/public/app.js`:
  - `data-action="friend-add" | "friend-remove"` → `/api/friends/:username`, then reload
  - the account radio → `PATCH /api/me`
- Test: `server/test/web.test.ts`, `server/test/account.test.ts`

**Interfaces:**
- Consumes: `friendState`, `requestFriend`, `removeFriend` (Task 2); `listClips(db, null, …)`
  (Task 3).
- Produces: `Me.profileVisibility` set and returned. The page prop
  `UserPage({ …, friendState: FriendState | null, full: boolean })`.

- [ ] **Step 1: Write failing tests:**
  - `PATCH /api/me profileVisibility public` → `Me.profileVisibility === 'public'`. An invalid
    value → 400.
  - `GET /u/me logged out, Friends profile`: 200. The HTML contains me's public clip title, not the
    friends clip title, no stats block, and a "Log in to add" link.
  - `GET /u/me logged out, Public profile`: badges and stats are present.
  - `GET /u/friend as me`: it shows "Friends" and the friend's friends clip.
- [ ] **Step 2: Run them.** Expected: FAIL.
- [ ] **Step 3: Implement.** For a non-full view, `getProfileStats`, achievements and banner are
  not loaded.
- [ ] **Step 4: Run** `pnpm vitest run test/web.test.ts test/account.test.ts`. Expected: PASS.
- [ ] **Step 5: Commit.** `feat(web): public profiles, Add friend button, profile visibility setting`

### Task 5: Open sign-up and referral links

**Files:**
- Modify: `server/src/auth/routes.ts`:
  - `registerBody`: `inviteCode` optional; `clipId` accepted and ignored; the `refine` removed.
  - The register transaction.
- Create: `server/src/referrals/routes.ts`:
  - `POST /api/referrals`, `GET /api/referrals`, `DELETE /api/referrals/:code`
  - `GET /r/:code`
- Modify: `server/src/admin/routes.ts`, `server/src/admin/queries.ts`,
  `server/src/web/pages/admin.tsx`: remove the invite endpoints and section. Keep `activeInvite`,
  which referrals use.
- Modify: `server/src/config.ts`, `server/test/helpers.ts`, `server/test/config.test.ts`: remove
  `SHARE_SIGNUP` / `shareSignup`.
- Modify: `server/src/web/routes.ts`:
  - `/invite/:code` → 302 to `/r/:code`
  - new `/signup`
  - `/c/:id` signup flag = logged out
- Create: `server/src/web/pages/signup.tsx`. Delete `invite.tsx`.
- Modify: `server/src/web/pages/login.tsx`, adding "No account? Sign up". Modify
  `server/src/web/pages/share.tsx`: the sign-up link goes to `/signup?next=/c/<id>`.
- Modify: `server/public/app.js`: the signup form posts `{ username, password }`, and the server
  reads the cookie.
  - On `referral: 'invalid'`, show "That invite link was already used or expired. Your account is
    ready anyway."
  - Then go to `/get-app?welcome=1&next=…`.
  - Remove `create-invite`/`revoke-invite` and the `invalid_invite`/`invalid_share` messages.
- Modify: `packages/shared/src/api.ts`: `Referral { code; url; expiresAt }`; the register response
  is `Me & { referral?: 'invalid' }`.
- Test: `server/test/referrals.test.ts`; update `auth.test.ts`, `share.test.ts`, `admin.test.ts`.

**Interfaces:**
- Consumes: `makeFriends` (Task 2), `activeInvite` (existing).
- Produces:
  - `REFERRAL_DAYS = 14`
  - `REF_COOKIE = 'klipt_ref'`
  - The referral URL is `${baseUrl}/r/${code}` (code: `nanoid(16)`).
- Register: the code comes from `body.inviteCode ?? cookie klipt_ref`.
  - If it's active and `createdBy` is not disabled: claim it (set `usedAt`/`usedBy`) and call
    `makeFriends` in the same transaction.
  - Otherwise: register anyway and add `referral:'invalid'`.
  - Clear the cookie in both cases.
- `GET /r/:code`:
  - **Logged out:** for an active code, set the cookie and redirect `/?ref=1`. For an inactive one,
    redirect `/?ref=invalid`.
  - **Logged in, someone else's active code:** claim it, call `makeFriends`, redirect
    `/u/<creator>`.
  - **Logged in, own code:** redirect `/friends?ref=own`.
  - **Logged in, inactive code:** redirect `/friends?ref=invalid`.

- [ ] **Step 1: Write failing tests in `referrals.test.ts`:**
  - `anyone can register without a code` → 200. The account has no friends.
  - `register with a referral cookie makes friends and uses the code`. A second register with the
    same cookie → 200, `referral:'invalid'`, no friendship.
  - `expired referral` (set `expiresAt` in the past) → `referral:'invalid'`.
  - `GET /r/:code logged out sets klipt_ref and redirects /?ref=1`.
  - `GET /r/:code logged in befriends and redirects to the creator's profile`. Own code →
    `/friends?ref=own`.
  - `POST /api/referrals` twice → two different active codes, each expiring 14 days out (±1 min).
    `DELETE` revokes one; `GET` lists the other.
  - `GET /invite/abc` → 302 to `/r/abc`.
  - `GET /api/admin/invites` → 404.
- [ ] **Step 2: Run them.** Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run the full server suite.** Expected: PASS.
- [ ] **Step 5: Commit.** `feat(server): open sign-up; single-use referral links that make you friends`

### Task 6: Web Friends page, nav, clip visibility on the web

**Files:**
- Create: `server/src/web/pages/friends.tsx`:
  - incoming requests (Accept/Decline)
  - outgoing (Cancel)
  - friends (Unfriend)
  - "Add by username" form
  - "Make a referral link" button, then the active links with Copy and Revoke
  - `?ref=own|invalid` notices
- Modify: `server/src/web/layout.tsx`: `NavSection` gets `'friends'`. A top-bar link "Friends" with
  the incoming-count badge (`AuthUser` gains `friendRequests: number`, loaded in `auth/plugin.ts`
  next to `newAchievements`).
- Modify: `server/src/web/pages/share.tsx` (owner's controls) and `server/public/app.js`: the
  visibility control becomes Link only / Friends / Public.
- Modify: `server/public/app.css`: styles for the new page and the badges.
- Test: `server/test/web.test.ts`

**Interfaces:**
- Consumes: the Task 2 API, the Task 5 referral API.
- Produces: the `/friends` route (guarded). `AuthUser.friendRequests`.

- [ ] **Step 1: Write failing tests:**
  - `/friends logged out` → redirect to login.
  - `/friends` lists an incoming request with an Accept button.
  - The top bar shows a badge `1` for one incoming request.
- [ ] **Step 2: Run them.** Expected: FAIL.
- [ ] **Step 3: Implement.** Copy buttons reuse the existing `data-action="copy"`.
- [ ] **Step 4: Run them.** Expected: PASS.
- [ ] **Step 5: Commit.** `feat(web): Friends page with requests and referral links`

### Task 7: "Seen by all your friends" achievement

**Files:**
- Modify: `packages/shared/src/achievements.ts` (id `seen-by-everyone` stays):
  - name: `Seen by all your friends`
  - description: `All your friends (at least 3) watched one of your clips`
- Modify: `server/src/achievements/stats.ts`. `seenByEveryone` counts my clips that every accepted,
  non-disabled friend has a `clip_views` row for. It is 0 when I have fewer than 3 such friends.
- Test: `server/test/achievements.test.ts`

- [ ] **Step 1: Write failing tests:**
  - `seen by all friends needs 3 friends`: 2 friends who both watched → stat 0. 3 friends who all
    watched one clip → stat 1.
  - `strangers don't count`: a non-friend who never watched doesn't block it.
- [ ] **Step 2: Run them.** Expected: FAIL.
- [ ] **Step 3: Implement the SQL.**
- [ ] **Step 4: Run them.** Expected: PASS.
- [ ] **Step 5: Commit.** `feat(server): Seen by everyone becomes Seen by all your friends`

### Task 8: Desktop: three visibility levels and profile visibility

**Files:**
- Modify: `desktop/src-tauri/src/upload/api.rs`. `enum Visibility { Link, Friends, Public }` with
  `#[serde(alias = "feed")]` on `Friends`, since `pending_uploads` rows and the old library hold
  `"feed"`.
- Modify: `desktop/src-tauri/src/commands.rs:1040` (the string mapping) and
  `server_clips.rs:445`. The default for a missing value becomes `"friends"`.
- Modify: `desktop/src-tauri/src/account.rs` / `update_profile` path: a new command
  `set_profile_visibility(visibility: String) -> Account`, which PATCHes `/api/me`. `Account`
  carries `profileVisibility`.
- Modify: `desktop/src/lib/api.ts` (types), and:
  - `desktop/src/components/UploadDialog.svelte`: three options. The default is
    `clip.visibility ?? account.profileVisibility`; map a stored `"feed"` to `"friends"`.
  - `desktop/src/views/Library.svelte`: the bulk upload default, and the toggle at 488/505 becomes
    a small menu Link only / Friends / Public, with toasts "Now link-only.", "Now friends only.",
    "Now public.".
  - `desktop/src/components/OnlineClipCard.svelte`, `ClipCard.svelte`, `clip-card.css`: labels
    "Link only" and "Public" (globe icon).
  - `desktop/src/views/Settings.svelte`: Account → Profile Public/Friends.
- Test: `desktop/src/lib/uploads.svelte.test.ts`, `desktop/src/lib/myClips.test.ts`, and a Rust unit
  test in `upload/api.rs`.

- [ ] **Step 1: Write a failing Rust test `visibility_reads_old_feed_as_friends`:**
  `serde_json::from_str::<Visibility>("\"feed\"")` gives `Friends`. Serializing `Friends` gives
  `"friends"`.
- [ ] **Step 2: Run it.** `cd desktop/src-tauri && cargo test visibility_reads_old_feed`.
  Expected: FAIL.
- [ ] **Step 3: Implement the Rust changes, then the Svelte changes.** Update the TS tests whose
  fixtures say `"feed"`.
- [ ] **Step 4: Run** `cargo test` and `cd desktop && pnpm test && pnpm check` (svelte-check, if
  present). Expected: PASS.
- [ ] **Step 5: Commit.** `feat(desktop): Link only / Friends / Public uploads, profile visibility setting`

### Task 9: Desktop Friends screen

**Files:**
- Create: `desktop/src-tauri/src/friends.rs`. Commands follow the `server_clips.rs` pattern
  (`login(&app, LIST_TIMEOUT)`, then `client.get_json/post_json/delete`):
  - `list_friends() -> Value`
  - `add_friend(username: String) -> Value`
  - `remove_friend(username: String) -> ()`
  - `make_referral() -> Value`
  - `list_referrals() -> Value`
  - `revoke_referral(code: String) -> ()`

  Validate `username` with `^[A-Za-z0-9_]{3,24}$` and `code` with `^[A-Za-z0-9_-]{1,100}$`.
- Modify: `desktop/src-tauri/src/lib.rs` (register the commands), `desktop/src/lib/api.ts`.
- Create: `desktop/src/views/Friends.svelte`. It has the same sections as the web page, plus "Make
  link" (copies the URL, toast "Link copied. It works once, for 14 days.").
- Modify: `desktop/src/App.svelte`:
  - `Screen` gets `"friends"`
  - a rail button below Albums, with a badge = `incoming.length`
  - the count refreshes when the window gains focus and on the existing account refresh
- Test: a Rust unit test for the validators.

- [ ] **Step 1: Write failing tests:** `checked_username` rejects `a`, `a/b` and `../x`, and
  accepts `Bob_1`.
- [ ] **Step 2: Run them.** `cargo test checked_username`. Expected: FAIL.
- [ ] **Step 3: Implement the commands and the screen.**
- [ ] **Step 4: Run** `cargo test` and `pnpm check`. Expected: PASS.
- [ ] **Step 5: Commit.** `feat(desktop): Friends screen with requests and referral links`

### Task 10: Desktop: built-in server, no server step

**Files:**
- Modify: `desktop/src-tauri/src/commands.rs`:
  - `pub const DEFAULT_SERVER: &str = "https://klipt.adamko.tech";`
  - `fn default_server() -> String`: `KLIPT_SERVER` if set and non-empty (normalized with
    `normalize_server_url`), else `DEFAULT_SERVER`.
  - A command `get_default_server() -> String`.
- Modify: `desktop/src/views/Login.svelte`:
  - The server is `initialServer ?? await api.getDefaultServer()`, so the login form shows
    directly.
  - The server row and "Change" are hidden. The `checkServer` form stays in the file only for the
    "server can't be reached" error path.
  - The subtitle becomes a "No account? Create one" link that opens `<server>/signup` via
    `open_url` (it must work while logged out: add `open_signup(server: String)`, which allows only
    `https://` or `http://localhost` URLs).
- Modify: `desktop/src/App.svelte:316` and `desktop/src/views/Settings.svelte:974`. Settings shows
  the server as plain text without a change control.
- Modify: `server/src/web/pages/getApp.tsx`: remove the server address step. Step 3 becomes "Open
  Klipt and log in with your username **x** and password."
- Test: a Rust unit test, and `server/test/web.test.ts`.

- [ ] **Step 1: Write failing tests:**
  - Rust `default_server_uses_env_override`: with the env unset → `https://klipt.adamko.tech`; with
    it set to `http://localhost:3000/` → `http://localhost:3000`. Serialize env-var tests with the
    existing test mutex, if there is one, or in one test.
  - Web: `/get-app` HTML does not contain "server's address".
- [ ] **Step 2: Run them.** Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test`, `pnpm check`, and `pnpm vitest run test/web.test.ts`.
  Expected: PASS.
- [ ] **Step 5: Commit.** `feat(desktop): klipt.adamko.tech is built in; login opens straight to the form`

### Task 11: Showcase page

**Files:**
- Create: `server/src/web/pages/home.tsx`, the layout C sections in order:
  1. Top bar: Features, Download, Log in, **Sign up**.
  2. Optional referral banner: "*Name* invited you. Sign up to be friends." For `?ref=invalid`:
     "That invite link was already used or expired. You can still sign up."
  3. Hero: `<h1>Clips without bullshit.</h1>`, the sub line from the spec, Download for Windows
     (`/get-app/windows`) and Create an account (`/signup`), and the hero video
     `<video autoplay muted loop playsinline poster>`.
  4. Five pillar cards (copy from the spec), each with an `<img loading="lazy">`.
  5. Klipt vs Medal table. Rows come from a `COMPARISON` array at the top of the file. Task 12
     fills in the verified rows; ship with the Klipt-side facts only, and the Medal column empty
     and hidden until filled.
  6. Download block: the Windows button, the Arch command (reuse `LINUX_INSTALL`; move it to a
     shared spot in `getApp.tsx` as an export), and Sign up.
  7. Footer: GitHub `https://github.com/cRaZy92/klipt`, Log in.
- Create: `server/public/showcase/`, with placeholder `.webp` files (a solid `--surface-2` colour
  with a label) and `hero.mp4`/`hero.jpg` placeholders. Task 12 replaces them; the file names are
  fixed here:
  - `hero.mp4`, `hero.jpg`
  - `record.webp`, `edit.webp`, `share.webp`, `friends.webp`, `looks.webp`
- Modify: `server/src/web/routes.ts`. `GET /` logged out renders `HomePage({ referrer })`, where
  `referrer` is the display name of the `klipt_ref` code's creator when it's active. Logged in, it's
  the feed as today.
- Modify: `server/public/app.css`: a `.home` section of styles, using existing tokens. Phone width:
  single column, 16px gutters, no horizontal scroll.
- Test: `server/test/web.test.ts`

- [ ] **Step 1: Write failing tests:**
  - `GET / logged out` → 200, the HTML contains `Clips without bullshit.`, `/signup` and
    `/get-app/windows`.
  - `GET / logged in` → the feed (existing assertion).
  - `GET / with an active klipt_ref cookie` → it contains "invited you" and the creator's display
    name.
- [ ] **Step 2: Run them.** Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run them.** Expected: PASS. Then run the server (`pnpm dev` with a local
  Postgres/pglite per README), open `/` logged out in Chrome at 1440px and 390px wide, and check the
  layout and that there's no horizontal scroll.
- [ ] **Step 5: Commit.** `feat(web): showcase page for logged-out visitors`

### Task 12: Showcase screenshots, hero video, Medal comparison

Manual task: no tests. Inputs: production at https://klipt.adamko.tech, viewed through Chrome
(claude-in-chrome) logged in as the owner, and the desktop app (`pnpm tauri dev`).

- [ ] **Step 1: Pick the hero clip.** It must be one of the owner's own clips on production (good
  action, ≤ 30 s). Note its id in the commit message.
- [ ] **Step 2: Make `hero.mp4`.**
  1. Download that clip's video.
  2. Cut 8–12 s.
  3. Encode it:
     ```
     ffmpeg -ss <start> -t 10 -i in.mp4 -an -vf scale=1280:-2,fps=30 -c:v libx264 -crf 28 -preset slow -movflags +faststart hero.mp4
     ```
  4. Check it's ≤ 4 MB.
  5. Make `hero.jpg` from its first frame.
- [ ] **Step 3: Take the pillar screenshots, each at 1600px wide, as WebP q80 ≤ 250 KB.**
  - **Before each shot, filter or crop so only the owner's clips, name and avatar show.** On the
    Feed or Library, filter to the owner's clips. Blur anything else.
  - `record.webp`: the desktop Library, or a "Clip saved" toast over a game.
  - `edit.webp`: the Editor with the trim bar, cuts and zoom lanes.
  - `share.webp`: the clip page, or a Discord embed of an owner clip.
  - `friends.webp`: the Friends screen. Blur friends' names and avatars, or use test accounts.
  - `looks.webp`: the owner's profile with a banner, frame and achievements.
- [ ] **Step 4: Fill in the Klipt vs Medal rows.**
  1. Look up Medal's current pricing/FAQ pages (web search).
  2. Keep only rows where both sides are clearly checkable today. Candidates: ads, watermark on
     free clips, upload length/size limits on the free tier, Linux app, open source.
  3. Write each Medal claim with the page it came from as a code comment.
- [ ] **Step 5: Look at `/` in Chrome at 1440px and 390px with the real assets.**
- [ ] **Step 6: Commit.** `feat(web): showcase screenshots, hero clip and Medal comparison`

---

## Self-review notes

- Every spec section has a task:
  - data → T1
  - who sees what → T3
  - profiles → T4
  - sign-up and referrals → T5
  - friends → T2, T6 and T9
  - visibility UI → T6 and T8
  - achievement → T7
  - old-app compatibility → T1, T3 and T8
  - showcase → T11 and T12
  - `/get-app` and app login → T10
- The desktop has no view of other people's profiles today, so "Add friend" in the app lives on the
  Friends screen (add by username). Profiles with Add friend are on the web.
