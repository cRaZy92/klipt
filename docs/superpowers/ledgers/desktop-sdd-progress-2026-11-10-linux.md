# SDD ledger — plan: docs/superpowers/plans/2026-11-09-klipt-desktop.md
Narrative ledger (committed): docs/superpowers/ledgers/desktop-sdd-ledger.md. Pre-flight scan ran in the Linux 2026-11-09 session (see committed ledger); not repeated.
Task 1: complete (8ac05fe..8f67c93, review fix wave done) — Linux Step 5 still open (build + manual run with owner)
Task 2: complete (Windows CI green)
Task 3: complete (docs)
Task 4: implemented 0e5d592; independent review interrupted on Windows — resuming review on Linux 2026-11-10
Task 4: Linux run: cargo test -p klipt-desktop 26/26 pass (CachyOS, 2026-11-10)
Task 4: minor (deferred): paths stored lossily via to_string_lossy (db.rs insert/list); UNIQUE path case-sensitive on Windows
Task 4: minor (deferred): corrupt library.db panics at startup (lib.rs setup ?/expect) — needs backup + friendly dialog
Task 4: minor (deferred): ffmpeg test fixture TempDir in static OnceLock never cleaned
Task 4: minor (deferred): settings with_invalid_fields_reset matches field names as strings (debug_assert only)
Task 4: minor (deferred): thumbnail temp name fixed (out.with_extension("tmp.jpg")) — concurrent calls clash
Task 4: minor (deferred): PATH lookup doesn't check exec bit
Task 4: minor (deferred): settings save leaves .tmp on failure; no dir fsync
Task 4: minor (deferred): tauri features=[] lacks protocol-asset + assetProtocol scope → thumbnails won't load (fix in Task 10 library UI)
Task 4: minor (deferred): list() holds mutex while stat-ing every file
Task 4: minor (deferred): ffmpeg test panic message mentions only WINDOWS-DEV.md
Task 4: complete (commits 0c30f83..0e5d592, review clean)
Ruling: Task 5 adds `cargo test -p klipt-desktop --locked games` to windows-recorder.yml so cfg(windows) code is compiled+tested in CI — plan says "compiled in CI" but no desktop Windows CI exists until Task 13 — cost if wrong: one extra CI step to move later
Ruling: Task 5 fixtures use the owner's real libraryfolders.vdf (3 libraries, not 2 as plan text says) — real file beats plan's guess — cost: none
Task 1 Linux Step 5 (build part, 2026-11-10): cargo build -p klipt-recorder OK on CachyOS only with SIMDE_INCLUDE_DIR=spikes/capture/third_party/simde (default third_party/simde under recorder-obs absent; no /usr/include/simde — Arch pkg `simde` exists in extra). 1 warning: unused import `Context` obs.rs:20 (Linux). Tests 34/35: selftest::parses_the_windows_test_command fails on Linux ("C:\\clips" is relative → made absolute against cwd).
Ruling: Linux fix batch after Task 5 — (a) test uses a platform-absolute --out path, (b) cfg the Windows-only import, (c) build.rs Linux SIMDe default: SIMDE_INCLUDE_DIR, else /usr/include if /usr/include/simde exists (pacman simde), else third_party/simde; error message says so — cost if wrong: trivial
Task 5: implemented 3aa6db8 (47/47 tests Linux; clippy not installed); review Approved, 0 Crit/Imp; pending: Windows CI run 42821736422 compiles proc_windows.rs
Task 5: minor (deferred): all_manifests read every call even when unused (Linux is_game_running every 3 s) — load lazily
Task 5: minor (deferred): Linux Detected.exe is first SteamAppId PID (reaper/pressure-vessel/wine64-preloader), so known/mapping fallbacks key on a wrapper exe; a mapping on wine64-preloader would hit every Proton game — key Linux mappings on appid
Task 5: minor (deferred): mappings::set after corrupt file silently overwrites all mappings — move corrupt file aside
Task 5: minor (deferred): detect_now_never_panics asserts nothing
Task 5: minor (deferred): Windows is_game_running counts any steamapps\common exe (Wallpaper Engine) — note for supervisor
Task 5: minor (deferred): legacy pre-2021 libraryfolders format ignored
Task 1 Linux fix: complete (bccc821, review Approved; 35 unit + 5 CLI pass on Linux, 0 warnings)
Task 1 Linux fix: minor (deferred): set-but-invalid SIMDE_INCLUDE_DIR silently falls through to /usr/include
Task 5: Windows CI run 42821736422 success (games tests compile+pass on windows-latest)
Task 5: complete (commits 5202733..3aa6db8, review clean)
Ruling: start Task 6 before the owner's C9 game test (overrides the Windows session's "Task 6 should wait for C9") — the RecorderBackend trait isolates the engine, gsr+supervisor are Linux/engine-independent, and this session is on Linux — cost if C9 fails: rewrite recorder/obs.rs only
Task 6: implemented 9fef2fe (77/77); review: Needs fixes — 1 Important (no "restarting" notice on unexpected gsr exit; test asserts silence — Review Focus 2)
Ruling: fix round 1 also includes review Minors 1 (start Err only logged → emit friendly error; no retry for RecorderMissing) and 4 (obs start-write failure leaves helper → restarts stuck) — same code path as the Important, real user-visible bugs — cost: slightly larger fix diff
Task 6: minor (deferred): PDEATHSIG best-effort (file caps clear it; no getppid check)
Task 6: minor (deferred): SIGUSR1 right after spawn can kill gsr before handler installed (crash path restarts)
Task 6: minor (deferred): relative output_dir never matches parse_saved_line (needs absolute)
Task 6: minor (deferred): SupervisorHandle::configure has no caller — settings change doesn't restart recorder (Task 7/12)
Task 6: minor (deferred): save pipeline (probe/thumbnail) runs on supervisor thread
Task 6: minor (deferred): supervisor.rs 877 lines; tests could move to supervisor/tests.rs
Task 6: note: no displayId → first --list-monitors line = HDMI-A-1 on owner's PC (main is DP-1); display picker in Task 12; Step 5 sets displayId=DP-1
Task 6: ⚠️ verify in Step 5: gsr flushes saved-path line to a piped stdout
Task 6: fix round 1/5 (3 addressed, 0 open — restart notices, start errors surfaced/RecorderMissing no retry, half-started helper shut down; commits 9fef2fe..c91e70e)
Task 6: minor (deferred): start failure → two toasts (cause + "restarting…")
Task 6: minor (deferred): dropping helper's recording_stopped relies on Exited always following
Task 6: minor (deferred): WhileGameRunning re-emits recorder_missing at every game start
Task 6: complete (commits bccc821..c91e70e, review clean) — manual Step 5 pending with owner
Ruling: batch owner manual checks (Task 1 Linux Step 5, Task 6 Step 5 gsr+save_clip, later Task 7) for one sitting instead of stopping per task — continuous execution; cost if wrong: a defect found later needs rework across tasks 6–7
Task 6: Windows CI run 42825342379 success (crate incl. recorder/obs.rs compiles on Windows)
Task 7: implemented fe6d825 (99/99 Linux)
Task 7: review Needs fixes — 1 Important (tray set_icon/tooltip/text block on main thread from supervisor thread → Quit deadlock with shutdown join)
Ruling: Task 7 fix round 1 also takes Minors 2 (tray status read/write race), 3 (portal stream end → silent dead hotkey; emit hotkey_unavailable), 5 (no single-instance: relaunch from menu spawns 2nd recorder/portal/tray — add tauri-plugin-single-instance) — same area, user-visible; cost: bigger fix diff
Task 7: minor (deferred): no feedback for presses hitting recording_paused/not_recording/save_failed while gaming
Task 7: minor (deferred): tray creation failure is fatal at startup
Task 7: minor (deferred): empty portal description → hotkey_taken mapping unverified (Step 5); which physical key ALT+grave binds on Slovak
Task 7: minor (deferred): hotkey-bound/hotkey-error may fire before webview listens — Task 12 keep binding in state
Task 7: Windows CI run 42826927607 success (hotkey/windows.rs + global-shortcut compile)
Task 7: fix round 1/5 (4 addressed, 0 open — non-blocking tray, pause race, hotkey_unavailable, single-instance; commits fe6d825..aa97066)
Task 7: minor (deferred): main-thread inline tray apply can overtake queued supervisor apply → icon briefly stale
Task 7: minor (deferred): notify() synchronous on supervisor thread (slow notification daemon stalls supervisor)
Task 7: complete (commits c91e70e..aa97066, review clean) — manual Step 5 pending with owner
Task 8: implemented f37bb2b (120/120)
Task 7: Windows CI run 42827857946 success on aa97066 (single-instance compiles)
Task 8: minor (deferred, triage at final): 120 s whole-PATCH timeout + retry budget not reset when HEAD offset advanced → slow uplinks (<~70 KB/s) exhaust retries while progressing
Task 8: minor (deferred): login wrong password maps to "session expired" message — login UI must special-case (Task 10)
Task 8: minor (deferred, security-ish): persisted absolute upload URL used with current token — check origin == base before resume
Task 8: minor (deferred): Auth derives Debug incl. token — redact
Task 8: complete (commits aa97066..f37bb2b, review clean)
Task 9: implemented 941648e (126/126; no separate RED run — disclosed)
Task 9: minor (deferred): fixed temp name <id>.tmp.mp4 collides for concurrent exports of same clip (caller must serialize per clip — Task 10)
Task 9: minor (deferred): no non-keyframe-start test (e.g. 2500–4500 → ~2.0 s); reviewer verified by hand output ends at requested end, pre-roll hidden via mp4 edit list
Task 9: minor (deferred): start past EOF → progress jumps; Windows rename fails if destination open
Task 9: note: exported MP4 relies on an edit list to hide keyframe pre-roll — check server probe/browser playback in final/manual test
Task 9: complete (commits f37bb2b..941648e, review clean)
Task 10: implemented 380edab (vitest 18/18, cargo 138/138); note for Task 11: Save edit must clear upload progress + delete exports/<id>.mp4
Task 10: review Needs fixes — 1 Important (late 401 from old job's token unconditionally clears a newer auth.json)
Ruling: Task 10 fix round 1 also takes Minors 2 (panic in run_upload → no upload-failed, card stuck), 3 (asset scope recursive on output dir → non-recursive), 5 (already_uploading resets card to Starting), 7 (late window ignores hotkey error), 8 (missing-file uploaded clip keeps Copy link) — cheap, user-visible
Task 10: minor (deferred): guard dropped before final event → narrow retry race
Task 10: minor (deferred): deliberate logout during upload shows "session expired" + auto-restart under next login
Task 10: minor (deferred): Modal no focus trap; role=menu popups lack keyboard handling
Task 10: note for Task 11: editor video asset scope must allow the clip's own file (allow_file) — clips saved before an outputDir change
Task 10: fix round 1/5 (6 addressed, 0 open — conditional auth clear + stale retry, panic-safe job, non-recursive scope, start state, hotkey toast, copy link on missing; commits 380edab..a7d5509)
Task 10: minor (deferred): clear_auth_if_current load/compare/clear not atomic; stale retry briefly shows "Starting… 0"
Task 10: note: foreign-account resume URL → server 404 → tus fresh upload once (handled by Task 8 Gone path)
Task 10: complete (commits 941648e..a7d5509, review clean) — manual Step 5 pending with owner (local server)
Task 10: Windows CI 42831862678 success on 380edab (clipboard-manager builds on Windows); a7d5509 run 42832706129 in progress
Task 11: implemented 71cbc6a (vitest 37/37, cargo 152)
Task 11: minor (deferred, fix before merge): save_edit runs update_edit before forget_upload_export — a failed delete then an identical re-save skips cleanup → stale export resumes old cut
Task 11: minor (deferred): play-with-seek audio blip (guard onplay on video.seeking); .mp4 appended after overwrite confirm; TrimBar double pointer handling + no lostpointercapture; <audio> errors silent; logout skips unsaved guard; export_copy busy says "already uploading"
Task 11: complete (commits a7d5509..71cbc6a, review clean)
Ruling: owner low on usage (2026-11-10) — goal now a working Linux prototype: finish Task 12 with one cheap review, skip Tasks 13–14 and the final whole-branch review for now, build a Linux release binary, write handoff — cost: deferred minors + final review still owed before merge to main
Task 12: implemented 3eb584f (vitest 54, cargo 161); review SKIPPED (owner usage) — owed before merge
