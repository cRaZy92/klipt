# Testing klipt-recorder on Windows

This checks whether Klipt can record games on your PC without hurting your frame rate. It takes about 10 minutes. There's no app yet: you run a small program from a terminal.

## 1. Unpack

Extract the zip anywhere, for example `C:\klipt-test`. Keep the folders as they are; the program needs them.

## 2. See what it detects

Open PowerShell in the extracted folder and run:

```powershell
.\bin\64bit\klipt-recorder.exe --list
```

You'll see three lines:
- `ready` lists the video encoders it found. NVIDIA shows `obs_nvenc_h264_tex`, AMD shows `h264_texture_amf`.
- `displays` lists your monitors.
- `audioDevices` lists your speakers and microphones.

Copy all three lines into your message.

## 3. Record while playing

Pick a game that runs **uncapped** (no FPS limit, no V-Sync) and a spot you can return to where the FPS is steady.

1. **Without recording:** stand in that spot for 60 seconds and note the average FPS (the game's FPS counter, or [CapFrameX](https://www.capframex.com/)).
2. In PowerShell, start the recorder:
   ```powershell
   .\bin\64bit\klipt-recorder.exe --selftest --mic --hotkey ALT+F9 --exit-after 180 --out "$env:USERPROFILE\Videos\Klipt-test"
   ```
   It records for 3 minutes, keeping the last 30 seconds in memory.
3. Switch to the game (fullscreen) and stand in the same spot for 60 seconds. Note the average FPS again.
4. Still in the game, say something into your mic and press **Alt+F9**. Wait a few seconds and press it again.
5. When the 3 minutes are up, the recorder prints a `stats` line and stops.

## 4. Check the clips

Open `Videos\Klipt-test`. Each Alt+F9 press should have made a file called `Klipt <date> <time>.mp4`:
- It plays, and shows the last ~30 seconds of the game.
- The first audio track is the game sound only, the second is your mic only. (VLC: Audio → Audio Track.)

## 5. Send back

- The three lines from step 2.
- The FPS without and with recording.
- Everything the recorder printed in step 3, especially the `started`, `saved` and `stats` lines.
- Whether Alt+F9 worked while the game was fullscreen.
- Your GPU model.

If anything fails, also send the text it printed. To get the full log, run the command again with ` 2> log.txt` at the end and send `log.txt`.
