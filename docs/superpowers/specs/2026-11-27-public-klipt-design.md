# Public Klipt: open sign-up, friends, visibility, showcase page

Date: 2026-11-27. Status: reviewed, waiting on owner approval.

## Intent

Klipt has been invite-only for the owner's friend group. The owner wants it to work like a public app:
anyone opens klipt.adamko.tech, sees what Klipt does on a full showcase page, makes an account,
downloads the app and logs in, with no server address to type. There is one server
(https://klipt.adamko.tech) for now.

Opening sign-up must not open up the group's clips. Today every member sees every member's Feed clips,
on the Feed, on profiles, in tags and in search. With open sign-up that would include strangers, so the
group's privacy moves from "who has an account" to "who is your friend".

## Goals

1. **Open sign-up.** Anyone can register on the website. No invite and no share link needed.
2. **Friends (mutual).** One person sends a request and the other accepts. Only accepted friends see
   each other's Friends clips.
3. **Referral links.** Any user can make a link that works once. Someone who opens it and then signs
   up becomes that user's friend automatically.
4. **Three visibility levels per clip:** Link only, Friends, Public. The share link `/c/:id` works for
   anyone in all three. Visibility only decides where the clip is listed.
5. **Profile visibility:** Public or Friends. It is also the default visibility for new uploads, and
   it can be changed on each upload.
6. **Showcase page** at `/` for logged-out visitors: layout "C" from the mockups, headline
   **"Clips without bullshit."**
7. **App without a server step:** the app talks to https://klipt.adamko.tech by default, and the
   "Connect to your server" step is hidden.

## Non-goals (now)

- A Discover / public feed. Public clips are listed only on their owner's profile. It can be added
  later on top of the same `public` level.
- Reporting or moderating clips (only needed once Discover exists).
- Sign-up inside the app. The app links to the website's sign-up page.
- One-way follows or follower approval.
- Storage quotas. The owner doesn't want them. Abuse is handled with the existing admin tools
  (disable account, storage per user chart).
- More than one server, or choosing a server in the normal UI.

## Anti-goals (a failure even if it works)

- A stranger who just registered can see any of the group's Friends clips, names in tag lists, or
  search hits from them.
- Existing friends lose sight of each other's clips on launch day.
- Installed older apps break their uploads before they update.
- The public page shows friends' clips or names without their consent.
- The Medal comparison claims something that isn't true.

## Constraints

- Stack as is: Fastify + Drizzle + Postgres server with server-rendered TSX pages
  (`server/src/web/pages`). Desktop is Tauri + Svelte. Shared types live in `packages/shared`.
- One numbered Drizzle migration (next is 0017) for all schema changes.
- No prettier (it reformats whole files). Match the existing code style by hand.
- The owner redeploys the server. Old desktop builds stay in use until the updater runs.

## Approaches considered

**Privacy check: in the shared list query (chosen) vs a separate layer.**
Every list (Feed, profile, tags, search, game chips, profile stats) already filters through
`scopeCondition` and its helpers in `server/src/clips/repo.ts`. Putting "uploader is me / my friend /
clip is public" there means one place decides. A separate filter applied after each query would
mean several places to keep in sync, and one leak is enough to fail.

**Referrals: reuse `invites` (chosen) vs a new table.** `invites` already has single use
(`usedBy/usedAt`), expiry, revocation and `createdBy`. A new table would copy all of that.

**Showcase: a server-rendered page (chosen) vs a separate static site.** A separate site needs its own
deploy and its own idea of who's logged in. The page lives with the other pages and its images live in
`server/public/showcase/`.

**Showcase layout:** owner picked C (hero video + five pillars + "vs Medal") over A (long product tour)
and B (how-it-works steps + feature grid).

**Friends model:** owner picked mutual friends over one-way follow (a stranger can just follow and
watch) and approved followers (about as much work as mutual, a less familiar model).

## Design

### Data (migration 0017)

- **`friendships`**: `user_a uuid`, `user_b uuid` (stored with `user_a < user_b`, so a pair has one
  row), `requested_by uuid`, `status text` ('pending' | 'accepted'), `created_at`, `accepted_at`.
  Primary key (`user_a`, `user_b`). Both users cascade on delete. Index on `user_b`.
- **`clips.visibility`**: enum becomes `'link' | 'friends' | 'public'`. Migration: `'feed'` →
  `'friends'`. Default `'friends'`.
- **`users.profile_visibility`**: `'friends' | 'public'`, not null, default `'friends'`. Existing
  users get `'friends'`.
- **`invites`**: unchanged schema. Referrals are rows with `createdBy` = any user, expiry 14 days.
- **Backfill:** every pair of existing non-disabled users becomes an accepted friendship, so nothing
  changes for the group on launch day.

### Who sees what

Let *friend(viewer, owner)* = an accepted friendship between them.

| Place | Shows a clip when |
|---|---|
| Feed (`scope: 'feed'`) | the uploader is me, or visibility ∈ {friends, public} **and** friend(me, uploader) |
| Profile `/u/:name` | my own profile: all my clips. Someone else's: `public` clips, plus `friends` clips if we're friends |
| Tagged-in clips on a profile | the same rule, applied to the clip's uploader |
| Search, game chips, profile stats, album listing pages | built on the rules above (same helpers) |
| `/c/:id` share link, `/a/:id` album link | anyone with the link, as today |

Today's Feed shows everyone's Feed clips, so it now narrows to me + friends. Public clips from
non-friends don't appear on the Feed (no Discover).

**Profiles for logged-out and non-friend viewers.** `/u/:name` stops requiring a login.
- A Public profile shows name, avatar, banner, badges, stats over public clips, and the public clips.
- A Friends profile shows only name and avatar, plus "Add friend" (or "Log in to add").

**Public clips on a Friends profile** *(builder's call, confirm when reading)*: they show. A Friends
profile seen by a non-friend shows name, avatar, Add friend, and that user's own `public` clips.
Badges, banner, stats and tagged-in clips stay hidden. Otherwise Public would mean the same as
Friends for that user.

**Tags** *(builder's call)*: being tagged doesn't grant access.
- You can tag only your accepted friends. `resolveTags` rejects anyone else.
- The tag picker (`GET /api/users`) lists only your friends, so a stranger can't list member names.
- Existing tags stay after an unfriend. A tagged-in clip shows on the tagged person's profile only
  if the uploader and the tagged person are friends at the time it's viewed, so a stranger can't put
  clips on your profile.
- A tag list shown to a viewer who can't see a tagged person's profile just shows their name.

**Albums** *(builder's call)*: unchanged. Anyone with the album link sees every clip in it, as
today. Adding a friend's Friends clip to a shared album is the same as passing on that clip's own
link, which a friend can already do. You can add only clips you can see (`assertClips` checks the
same visibility rule).

**Favorites** *(builder's call)*: the favorites list applies the same visibility rule, so a clip
drops out after an unfriend (it comes back if you become friends again).

### Sign-up and referrals

- `POST /api/register`: `inviteCode` and `clipId` become optional and neither is required. The
  `refine` goes away. If `inviteCode` is given and active, it's claimed in the same transaction, and
  an accepted friendship with `createdBy` is created. An invalid or used code doesn't block sign-up:
  the account is made, and the response says `referral: 'invalid'` so the page can say "the link
  was already used". `clipId` is ignored for friendship (share sign-up is a plain sign-up).
  `SHARE_SIGNUP` config is removed.
- Referral flow on the web: `GET /r/:code` stores the code in a cookie (`klipt_ref`, 14 days,
  httpOnly) and redirects to `/` (the showcase) with a banner "*X* invited you. Sign up to be friends".
  The sign-up form sends the cookie's code. If the visitor is logged in, `/r/:code` claims it right
  away: it becomes a friendship (or confirms an existing one), and the browser goes to the sender's
  profile. Your own link or an expired/used link shows a short message.
- `/invite/:code` (old admin links) redirects to `/r/:code`.
- API: `POST /api/referrals` (any user) → `{ code, url, expiresAt }`; `GET /api/referrals` (my
  active ones); `DELETE /api/referrals/:code` (revoke mine). No limit on how many. Admin invite
  endpoints and the admin page's invite section are removed (admins use referrals like anyone).
- New `/signup` page (username, password, password again), linked from the showcase, the login page
  and the app. After sign-up it goes to `/get-app?welcome=1` as today.

### Friends

- API: `GET /api/friends` → accepted friends + incoming + outgoing requests;
  `POST /api/friends/:username` (send, or accept if they already asked me);
  `DELETE /api/friends/:username` (decline, cancel or unfriend: removes the row).
- Web: a Friends page (`/friends`) in the top bar with requests (accept/decline), the friends list,
  and "Make a referral link" (copy button, list of active links with revoke). Someone else's profile
  shows **Add friend / Requested / Accept / Friends ▾ (Unfriend)**.
- Desktop: a **Friends** screen in the rail (same contents, through the same API). The rail badge
  shows the incoming request count. Profile view in the app gets the same Add friend button.
- Nothing is sent on decline, and there's no block list (decline/unfriend is enough for now).

### Visibility in the UI

- Upload dialog (desktop): a three-way choice **Link only / Friends / Public**, preset from the
  user's profile visibility. Bulk uploads use the same choice. Clip edit (desktop + web clip page)
  can change it later.
- Account settings (web `/account` and desktop Settings → Account): **Profile: Public / Friends**,
  with one line explaining it's also the default for new uploads. Changing it doesn't change old
  clips.
- Card labels: "Link only" stays, plus "Public" (a small globe). Friends is the unmarked default.

### "Seen by everyone" achievement (owner's choice)

With open sign-up "everyone on the server" becomes impossible. It becomes **"Seen by all your
friends"**: every one of your accepted friends has watched one of your clips, and you have at least
3 friends. Same id (`seen-by-everyone`), same reward (`frame:rainbow`), and anyone who already
earned it keeps it. The name and description in `packages/shared/src/achievements.ts` change, and
`seenByEveryone` in `server/src/achievements/stats.ts` counts friends instead of all active users.

### Old app compatibility

Shared `Visibility` becomes `'link' | 'friends' | 'public'`. The server accepts `'feed'` on input as
an alias for `'friends'`, and keeps accepting it indefinitely, because older installed apps send it.
Responses send the new values. Older apps show a non-'link' value as a Feed clip, which is correct
enough until they update. (Builder: check how the current desktop code treats an unknown value, and
keep it from crashing.)

### Showcase page (`/` when logged out)

Server-rendered page `web/pages/home.tsx`, assets in `server/public/showcase/`, same tokens and
Bricolage font as the site. Logged-in visitors still get the Feed at `/`.

1. **Top bar:** logo, Features, Download, Log in, **Sign up** (primary).
2. **Hero:** headline **"Clips without bullshit."**, one line under it ("Records your games in the
   background, saves the last 30 seconds on a key, edits and shares with one link. No ads, no
   watermark, no limits."), buttons **Download for Windows** and **Create an account**. On the right,
   a muted looping video of one of the owner's own clips being trimmed and shared (an MP4 under ~4 MB,
   with a poster image).
3. **Five pillars** (cards with a screenshot each):
   - *Record:* replay buffer on a shortcut, screenshots, full recording, automatic game detection,
     low FPS cost.
   - *Edit:* trim, cuts, zoom, subtitles, custom thumbnail.
   - *Share:* one link that plays in Discord; Link only, Friends or Public.
   - *Friends:* a feed of just your friends, albums, tags, referral link.
   - *Make it yours:* avatars, banners, frames, themes, 32 achievements.
4. **Klipt vs Medal:** a short table with only claims that are true and checkable at writing time
   (e.g. ads, watermark, upload length/size limits, open source, Linux support). The builder checks
   Medal's current claims before writing them and leaves out anything unsure.
5. **Download block:** Windows installer button, "On Arch Linux?" command (as on `/get-app`), Sign up.
6. **Footer:** GitHub link, Log in.

If a referral cookie is set, a banner above the hero says who invited you.

**Screenshots and the hero video** are made by the builder from production
(klipt.adamko.tech) and the real desktop app. They show only the owner's own clips, names and avatar.
Friends' names, avatars and clips must not appear (blur or crop if one gets in). Images are
committed as optimized WebP/JPG.

### `/get-app` and the app's login

- `/get-app`: drop the "enter this server's address" step. Step 3 becomes "Open Klipt and log in".
- Desktop: `DEFAULT_SERVER = "https://klipt.adamko.tech"`. When no server is saved, Login skips the
  server step and shows the login form directly. The server row ("klipt.adamko.tech · Change") is
  hidden. The subtitle "With the account from your invite" becomes a **"No account? Create one"**
  link that opens `<server>/signup` in the browser. A saved server is kept. An env var
  `KLIPT_SERVER` (read by the Rust side and passed to the UI) overrides the default for development.
  Settings no longer shows a "change server" control in normal builds.

## Left to the builder

- Exact copy beyond the headline and what's listed above.
- Exact visual detail of the showcase within layout C and the existing site look.
- Which of the owner's clips is used for the hero (pick a good one and say which).
- Migration details, index choices, how `scopeCondition` and friends are factored.
- Whether the referral cookie also passes through `/login` → "Sign up" links (it should survive).
- Tests: repo-level tests for every row of the "who sees what" table (friend, non-friend, pending,
  logged out), register with/without/used referral, `'feed'` alias.
