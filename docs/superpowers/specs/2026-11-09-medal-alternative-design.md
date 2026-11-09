# Klipt — Design

Date: 2026-11-09
Status: Draft, awaiting approval

## Intent

**Klipt** is a self-hosted replacement for Medal.tv for a small friend group. You play a game, press a hotkey, and the last stretch of gameplay is saved as a clip. Later you can trim it, upload it to your own server, and send friends a link. Friends can also browse a shared feed of everyone's clips.

The reasons to build it instead of using Medal: you own the server and the clips, the client is light (native and not Electron), and it runs on Linux (CachyOS) as well as Windows.

It has to work for **casual users**. Friends on Windows install it, log in, and press a key. They should never have to touch a config file, a terminal, or codec settings.

## Goals

1. **Background replay buffer.** The app keeps the last N seconds or minutes of screen and audio. A configurable hotkey saves that window to disk as a clip.
2. **Local library** of saved clips, with thumbnails, game tags, a game filter, and sorting by date. You can delete clips from inside the app.
3. **Editor.** Trim the start and end, and adjust the volume of the system audio and mic tracks separately.
4. **Upload to the self-hosted server**, with a progress display. When it finishes you get a share link, which is copied to the clipboard automatically.
5. **Share links** that anyone can open in a browser without logging in. They embed properly when pasted into Discord.
6. **User accounts.** Invite-only, and you are the admin.
7. **Shared web feed.** Logged-in users see everyone's clips, can filter by game, and can visit per-user pages.
8. **Game auto-detection.** Each clip is tagged with the game being played.
9. **Low resource use.** Recording uses GPU encoding and must not noticeably cost frames in-game. The app is light when idle.

## Non-goals (v1)

- Steam Deck and SteamOS, including Gaming Mode / gamescope. These may come later.
- Linux desktops other than KDE Plasma on Wayland. GNOME and X11 might work by chance, but they aren't targets.
- macOS.
- Likes, comments, follows, notifications.
- Auto-generated titles, AI highlights, automatic clipping on kills.
- Editing beyond trim and per-track volume: no cuts in the middle, text, transitions, or merging clips.
- Public sign-up, or any use outside the friend group.
- Server-side transcoding into several qualities or adaptive streaming.
- **HDR** (my call). Clips are 8-bit SDR. If HDR is on, clips may look washed out. Proper tone-mapping can come later.

## Anti-goals (failure even if it "works")

- Recording causes stutter or a visible FPS drop in games.
- A casual Windows user has to do any technical setup beyond installing, entering the server address, and logging in.
- Clips are lost: the hotkey is pressed and nothing is saved, or the saved file is corrupt.
- Share links don't play in a normal browser or on a phone.
- The app sits at hundreds of MB of RAM when idle, as Electron apps do.

## Platforms & constraints

- **Windows 10/11.** Primary platform for friends.
- **CachyOS, KDE Plasma on Wayland.** The owner's machine. It must work there.
- **Server:** the owner runs Proxmox with Dokploy. It's deployed as one **Docker Compose** stack, and HTTPS/domain are handled by Dokploy's reverse proxy.
- **Languages:** TypeScript wherever possible. Rust is acceptable for native parts of the desktop app.
- **Hardware:**
  - The owner has an **RTX 3070 Ti on CachyOS KDE Wayland**, so NVIDIA's proprietary driver + PipeWire + NVENC is the Linux path that must work, and the spike proves it first.
  - Friends on Windows have **NVIDIA RTX 40xx** and **AMD** cards, so both NVENC and AMF must work on Windows. QuickSync is best-effort.
  - The owner's machine is the only one available for direct testing. Windows is validated by sending test builds to friends (my call).
- **License: open source (GPL is fine).** libobs and GPL FFmpeg builds (with x264) are both allowed.
- **No code signing.** Nothing gets paid for. Friends will see the Windows SmartScreen warning on first install ("More info → Run anyway"), and the install instructions or download page should show this step with a screenshot. Tauri's updater signs updates with its own free key, so auto-update still works.

## Decided architecture

### Desktop app

- **Tauri v2.** UI in TypeScript and the native core in Rust. The choice of UI framework is the builder's, as long as it stays light.
- Lives in the **system tray**. Starts with the OS (this can be turned off). Closing the window minimizes it to the tray.
- **Packaging:** a Windows installer (NSIS or MSI) and an **AppImage or Flatpak** for Linux, which is the builder's choice. It must be self-contained and not rely on system FFmpeg or codecs.
- **Auto-update** via Tauri's updater (my call, because casual users won't update by hand). The update feed can be served by the same server or by GitHub Releases. The builder chooses.

### Recording

- **Capture engine:** either FFmpeg or libobs, bundled with the app. **The first task is a spike that settles this**, judged on these criteria:
  - Windows: capture a chosen display (DXGI or Windows Graphics Capture), capture system audio through WASAPI loopback, and capture the mic.
  - CachyOS KDE Wayland: capture a chosen display through the **xdg-desktop-portal ScreenCast + PipeWire** path. KDE shows a permission prompt once, and the app should store the portal restore token so it doesn't ask again. Capture system audio (the PipeWire/Pulse monitor) and the mic.
  - Hardware encoding through NVENC, AMF, QuickSync, or VAAPI, with a software (x264) fallback if no GPU encoder works.
  - Measured CPU, GPU, and RAM cost while a game is running.
  - Note: FFmpeg doesn't handle WASAPI loopback or the Wayland portal on its own, so either the capture side gets written in Rust and fed to FFmpeg, or libobs is used, which already handles both. That trade-off is what the spike should settle.
- **Recording mode is a user setting:**
  - **Always (default):** the buffer runs whenever the app is running, so anything on the chosen display can be clipped.
  - **Only while a game is running:** the buffer starts when a game from the detection rules below is running and stops when it closes. This needs a lightweight background watcher (polling every few seconds is fine).
  - In both modes the tray icon shows clearly whether the buffer is recording, and a tray menu item pauses and resumes it.
- **Replay buffer:** keep **encoded** packets in a rolling buffer, not raw frames. Pressing the hotkey writes the buffer out to a file without re-encoding.
- **Buffer location is a user setting: RAM or Disk.**
  - **RAM (default):** the buffer lives in memory. Settings shows an estimate of the memory it will use, based on bitrate × buffer length. For example, 1080p60 at about 20 Mbps uses roughly 75 MB for 30 s and roughly 750 MB for 5 min.
  - **Disk:** the buffer is a rolling set of short segment files in a folder the user can choose (default: a temp folder under the app's data directory). Old segments are deleted as new ones are written, so disk use stays bounded and is shown in settings. The folder is cleaned up when the app starts and when it exits.
  - Changing the setting restarts the buffer, which discards what was in it, and the UI says so.
- **Clip file format:** one video track and **two separate audio tracks** (track 1 for system audio, track 2 for mic, included only if the mic is enabled). The video codec is **H.264** so files play everywhere. The container is the builder's choice (MP4 or MKV), but it has to be safe if the app crashes during a save.
- **Settings:** recording mode (Always / Only while a game is running), buffer location (RAM / Disk + folder), display to record, resolution (default 1080p, or native), FPS (default 60), quality/bitrate (simple presets such as Low/Medium/High plus an advanced value), buffer length (default 30 s, up to at least 5 minutes), mic on or off plus which mic device, the hotkey, and the output folder (default Videos/Klipt).
- **Feedback when a clip is saved:** a short sound plus a tray or desktop notification. Both can be turned off.
- **Global hotkey:** on Windows, a normal global hotkey that also works while a fullscreen game has focus. On KDE Wayland, X11-style global hotkeys don't work, so use the **XDG GlobalShortcuts portal**, which KDE supports. Tauri's built-in global-shortcut plugin is likely not enough on Wayland.

### Game detection

At the moment of the clip, detect the game in this order:

1. **Steam.** Windows: the foreground or fullscreen process whose executable lives under a `steamapps/common/...` folder, matched to an app ID through the `appmanifest_*.acf` files in all Steam library folders. Linux: processes with a `SteamAppId` (or `SteamGameId`) environment variable in `/proc/<pid>/environ`, which also covers Proton games. The name and box art come from Steam (the app ID gives the Steam CDN art URL).
2. **Built-in list of known executables** for Battle.net and Riot games (Overwatch, Valorant, League of Legends, CoD, WoW, Diablo, Hearthstone, and so on), plus a few other popular games. It ships with the app and is easy to extend.
3. **Manual mapping.** If nothing matches, the clip is tagged "Unknown" along with the executable name. The user can set the game in the library, and the app remembers that exe-to-game mapping for future clips.

The game tag (name plus Steam app ID if one exists) is sent with the upload so the web feed can filter by game too.

### Local library

- Grid of clips with a thumbnail, game, date, duration, and an "uploaded" badge with a copy-link button.
- Filter by game, sort by newest or oldest.
- Actions: open the editor, upload, copy link, rename, change game, show in folder, delete. Deleting asks for confirmation and removes the file from disk; a clip that has been uploaded stays on the server unless you delete it there separately.
- Clips stay on disk after upload. Local storage is never cleaned up automatically in v1.

### Editor

- Video preview with a timeline and start/end trim handles.
- Two volume sliders (Game, Mic) that affect the preview live. Mute is the same as 0.
- **It's non-destructive:** edits are stored as metadata. A trimmed or mixed output file is produced only when you save a copy or upload.
- Implementation note: the webview can't reliably adjust the volume of separate audio tracks inside one file. A suggested approach is to play the video muted and play the two audio tracks as separate synced streams (pulled out next to the clip, or through Web Audio), but the builder decides. WebKitGTK (the Linux webview) depends on GStreamer for video playback, so H.264 preview must work inside the packaged Linux app.

### Upload

- The app applies the trim, **mixes both audio tracks into one** at the chosen volumes (browsers only play one track), and produces an **H.264 + AAC MP4 with faststart**.
- The upload can resume after a dropped connection (tus or chunked upload, builder's choice). Progress is shown in the app.
- Options set at upload time: title (defaults to "<Game> — <date>") and **Feed or Link only** (defaults to Feed).
- When it succeeds, the share link is copied to the clipboard and saved with the local clip.

### Server (Docker Compose)

- **Node + TypeScript** API (the framework is the builder's choice), **Postgres**, and clip files plus thumbnails on a **Docker volume**. FFmpeg in the container generates thumbnails and checks uploads.
- The same service also serves the **website**: the share page, feed, user pages, login, and admin.
- **Accounts:**
  - The first admin is created from environment variables when the server first starts.
  - The admin creates **invite links**. A friend opens one and picks a username and password.
  - The web uses session cookies. The desktop app logs in with username and password and gets a long-lived token that can be revoked.
  - The admin can list and disable users and revoke invites.
- **Clips:** an owner can rename, toggle Feed/Link only, and delete their own clips. The admin can delete any clip.
- **Share links:** `https://<domain>/c/<short-unguessable-id>`. The page has a video player, title, uploader, game, and date. Open Graph and Twitter player meta tags make **Discord embed the video inline**, with a thumbnail. The page plays on phones. No login is needed.
- **Feed (login required):** everyone's Feed clips, newest first, with a game filter. Per-user pages list a user's Feed clips. "Link only" clips appear only to their owner (in "My clips") and to anyone who has the link.
- **Config:** domain/base URL, admin credentials, max upload size, and the storage path, all set through environment variables in the compose file.
- No storage quotas in v1. The admin page shows total storage used.

### First-run experience (desktop)

1. Enter the server address (for example `clips.example.com`).
2. Log in. There's a link to sign up with an invite in the browser.
3. On Linux, approve the KDE screen-share prompt once.
4. Pick the display, keep the mic off by default, and see the hotkey. Then it's ready and sits in the tray.

## Left to the builder

- Choosing between FFmpeg and libobs (through the spike above), and how the replay buffer is put together.
- UI framework for the desktop and web frontends, the server framework, and the ORM/migrations.
- AppImage or Flatpak; MP4 or MKV for local clips; tus or custom chunked upload.
- The visual design. It should feel clean and modern, and be obvious enough for non-technical users.

## Open risks

- **Wayland capture plus global hotkeys** is the least-trodden path. Prove both early on the owner's CachyOS machine.
- **WebKitGTK video playback** inside the packaged Linux app (needs the GStreamer H.264 decoder).
- **Anti-cheat** (Valorant's Vanguard, CoD's Ricochet): screen capture through DXGI or WGC is generally tolerated, as OBS and Medal show. Hooking into a game's graphics (game-capture injection) is out of scope; capture the display only.
