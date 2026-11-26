# Achievements, profile pictures and customization

Date: 2026-11-26. Status: draft for the owner's approval.

## Why

The owner finds the app plain and wants something to earn and collect. The fix ties the two together:
profile pictures and a few basic colours are free from day one, and achievements unlock more
cosmetics: colours, themes, avatar frames, profile banners. Each badge you earn gives you something
your friends can see ("where did you get that gold frame?"), which is what makes collecting worth it.

## Goals

- Everyone can set a profile picture. It replaces the coloured initial everywhere: the app (account
  button, Feed cards, clip player, tag picker) and the website (top bar, profile, share page, albums,
  admin).
- Everyone can pick an accent colour from a free basic set, in the app and on the website.
- Achievements are earned automatically, collected as badges, and shown on your profile.
- Some achievements unlock cosmetics: more accent colours, full themes (surface colours too), avatar
  frames and profile banners. Frames and banners are seen by friends.
- An "Achievement unlocked" moment when one is earned, so earning feels like an event.

## Non-goals

- Leaderboards or rankings. This is about collecting, not competing.
- Likes, comments, XP, levels, currencies, streak penalties.
- A theme editor or custom CSS. Themes are a fixed, hand-made set.
- Animated profile pictures (GIF/video). Still images only.

## Anti-goals (it has failed if…)

- Basics feel locked away. A profile picture and some colour choice must never need earning.
- Old clips count for nothing. Achievements are retroactive: existing activity counts.
- It nags. No reminders, no "you're 2 clips away" popups, no flood of toasts on the first launch.
- Friends see different things. What someone has earned and wears is the same in every app and on
  the website.

## Constraints from the project

- The server only knows **uploaded** clips (`clips`, `clip_views`, `clip_tags`, `albums`,
  `album_clips`). Clips saved but never uploaded, hours recorded and editor use live only on the
  desktop (`library.db`, editor JSON).
- Avatars today: `desktop/src/components/Avatar.svelte` (initial on a gradient from `hueOf(name)`)
  and `Avatar` in `server/src/web/layout.tsx` (the same idea, its own hash). Both take a name only.
- Colours are tokens: `--accent*` and the surface tokens in `desktop/src/app.css`; the website copies
  them. A theme = a set of token values.
- Image upload precedent: `PUT /api/clips/:id/thumbnail` (raw body ≤10 MB, decoded and re-encoded on
  the server, `thumb_version` in the URL so caches refresh). Avatars follow the same pattern.
- Shared types and constants live in `packages/shared`.
- Small friend group, self-hosted: the server can trust what a logged-in desktop reports about its
  own user.

## Approaches considered (for where achievements are decided)

1. **Server only, from what it already knows.** Simple and cheat-proof, but loses every achievement
   about saving, recording and the editor, which is half the fun and the "tour of features" part.
2. **Desktop only.** Each app decides locally. Friends couldn't see your badges, and two PCs would
   disagree. Breaks "friends see the same thing".
3. **Server decides; the desktop reports its own counters** (chosen, owner's call to confirm). The
   server keeps the list of earned achievements and evaluates them. For things only the desktop
   knows, the app sends a small summary of local counters and "firsts" (`POST /api/me/activity`);
   the server stores them and evaluates against them like any other data. Trusting the app is fine
   for this group.

## Design

### Profile pictures (free)

- Set in the app (Settings → Account, and the new Profile screen) and on the website (`/account`).
  Pick an image → a square crop (drag + zoom, round preview) → upload.
- `PUT /api/me/avatar` (raw image body, same size limit and decoding as the thumbnail route):
  the server makes a 256×256 JPEG at `<data>/avatars/<userId>.jpg` and bumps `users.avatar_version`.
  `DELETE /api/me/avatar` goes back to the initial.
- Served at `/avatars/<userId>.jpg?v=<version>`, cached as immutable. Every API payload that
  carries an owner/user (`owner: {username, displayName}` and friends lists) gains `avatarUrl`
  (null = draw the initial) and `frame` (the equipped frame id or null).
- Both `Avatar` components take an optional `url` and `frame`; with no picture they draw today's
  initial. Formats accepted: JPEG, PNG, WebP (as for thumbnails).

### Looks: accent colours, themes, frames, banners

- **Accent colours:** the current purple plus 5 free basics (e.g. blue, teal, green, orange, pink);
  more unlocked by achievements (e.g. gold, crimson, mint, a gradient "holo"). An accent sets the
  `--accent*` tokens (computed from one base colour).
- **Themes:** the current "Klipt" dark theme is free. Unlockable themes change the surface tokens
  too (e.g. "Midnight" deep blue, "OLED" pure black, "Ember" warm). Each is a fixed token set in
  `packages/shared` so the app and website match. Light theme is not in scope.
- **Avatar frames:** a ring/decoration drawn around the avatar (CSS, no images needed for v1:
  e.g. bronze, silver, gold rings, a rainbow ring, a pulsing ring). Seen by everyone, wherever the
  avatar shows at ≥28 px.
- **Profile banners:** a gradient/pattern strip behind the profile header on the website profile and
  the app Profile screen. Seen by everyone who opens the profile.
- Your equipped choices (`accent`, `theme`, `frame`, `banner`) are stored on the server
  (`users.looks jsonb`), so they follow you to other PCs and the website. Accent and theme only
  change what **you** see; frame and banner are what **others** see on you.
- The server refuses equipping something not unlocked (`403 not_unlocked`).

### Achievements

- **Definitions in code** (`packages/shared/achievements.ts`): id, name, description, icon,
  category, `hidden` flag, optional reward (a cosmetic id), and the rule (a counter + threshold, or
  a "first time" event). Adding one later = adding a definition; no migration.
- **Earned list** on the server: `user_achievements(user_id, achievement_id, earned_at,
  seen_at)`. Never removed once earned (deleting clips doesn't take badges away).
- **Desktop activity:** the app keeps local counters and firsts and sends them whenever they change
  (debounced) and at login: clips saved, screenshots taken, recorded hours, full recordings made,
  clips imported, and editor firsts (cut, zoom, subtitle, blur, mute part, combine, new clip from a
  part, thumbnail from a picture). Stored per user as `user_activity jsonb` (counters take the max
  of what's stored and what's sent, so two PCs don't fight; firsts are a union).
  Odd timed ones (saved at 3 a.m.) are reported as firsts by the app, which knows the local time.
- **Desktop backfill:** on its first report after the update (per account), the app rebuilds counters
  and firsts from what it already has: clips, screenshots, full recordings and imports in
  `library.db` (with their save times, for Night owl), recorded length from clip/recording
  durations, and editor firsts from the saved edits (cuts, zooms, subtitles, blur, muted parts,
  thumbnail pictures). That report carries `backfill: true`; the server marks what it awards as seen
  and adds it to the one "already earned" summary, so nothing arrives as a burst of toasts.
- **Whose activity:** activity belongs to the account logged in when it happens. While logged out
  the app keeps counting locally and sends it at the next login. The backfill counts the whole
  local library for the account doing it (a shared PC with several accounts is not a case here).
- **Views** count both friends' views (`clip_views`) and logged-out plays (`anon_views`), as the
  profile stats already do.
- **Evaluation:** on the server, after anything that can change a counter: finalize (upload), a
  view, a tag, album changes, an activity report, and once at startup for every user (that run
  also does the retroactive backfill when achievements are first deployed or new ones are added).
- **Retroactive without a flood:** achievements awarded by the startup run for activity that was
  already there are marked seen immediately. The first launch after the update shows one toast:
  "You've already earned 9 achievements" → opens the Profile screen.
- **Unlocked moment:** the app checks for unseen achievements on account refresh and right after it
  reports activity or finishes an upload; each shows a toast (window in front) or system
  notification ("Achievement unlocked: Night owl · unlocked the Midnight theme"). Opening it marks it
  seen. The website shows a small "New achievement" badge on the account button until viewed.
- **Hidden achievements** show as "???" with a lock until earned; the count of hidden ones is shown
  ("+4 hidden").

### Starting set (proposal; the owner can add, drop or rename)

Milestones
- First clip (save your first clip) · First upload
- 10 / 100 / 500 clips uploaded → bronze / silver / gold frame
- 10 hours recorded (desktop) → "Ember" theme
- Clips from 5 / 15 different games → accent colours
- First full recording · First screenshot · First import

Social
- 10 / 100 / 1,000 total views on your clips
- Seen by everyone: one clip watched by every other active user → rainbow frame
- Tagged in 10 clips by friends
- Tag a friend for the first time
- Curator: make an album with 10+ clips → a banner

Editor (doubles as a tour)
- First cut · First zoom · First subtitle · First blur · First muted part
- First combined clip · First new clip from a part · First custom thumbnail
- All of the above → "Editor" banner

Hidden
- Night owl: save a clip between 3:00 and 5:00 → "Midnight" theme
- Blink: upload a clip under 5 seconds
- Marathon: a full recording over 3 hours
- Happy new year: upload on 1 January
- Archivist: a clip that's still on the server a year after upload

Group-specific achievements (inside jokes, the group's main game) can be added later as definitions.

### Where it shows

- **App:** a new **Profile** screen on the rail (or from the account menu): your picture and
  banner, badges grid (earned / locked / hidden), and a "Looks" section to pick picture, accent,
  theme, frame and banner, with locked items showing what unlocks them. Settings → Account keeps a
  shortcut to the picture.
- **Website:** the profile page (`/u/<name>`) gets the banner, framed avatar and a badges strip
  (all earned badges, newest first, hover for name/description). `/account` gets picture upload and
  the looks picker. The website uses the viewer's own accent and theme when logged in.
- **Feed / share page / album page / player:** framed avatars only (no badges there).

## Decided by the owner

- Approach B: basics free (profile picture, a few colours), achievements unlock extra cosmetics.
- Achievements are something to earn and collect; the motivation is the app feeling plain.

## My calls (owner to check)

- Achievements count past activity; first-launch summary instead of one toast each.
- The server decides and stores achievements; the desktop reports local counters (approach 3).
- No leaderboards.
- Desktop backfill from `library.db` and saved edits on the first report, folded into the summary.
- Activity goes to whoever is logged in; logged-out activity waits for the next login.
- Views include logged-out plays.
- Picture upload in both the app and the website.
- Frames and banners are CSS-drawn for v1 (no image assets).
- Accent/theme apply to the website too when logged in.
- The starting achievement list above.

## Left to the builder

- Exact colour values for accents and themes (must pass contrast on text/buttons).
- Badge icon style (an icon set already in the app, or simple glyphs), frame and banner visuals.
- Crop UI details, and whether the app reuses `thumb_image.rs` decoding for the avatar.
- Migration numbering, endpoint payload shapes beyond what's named here, debounce timing.
- Whether "active user" for "Seen by everyone" means not disabled and seen in the last 30 days.
