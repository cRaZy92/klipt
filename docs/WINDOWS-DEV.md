# Developing Klipt on Windows

Do **not** copy the Linux project folder. `node_modules`, `target/` and the git worktree metadata are Linux-specific. Clone from GitHub instead.

## 1. Install the tools (one time)

Run these in PowerShell. `winget` ships with Windows 11; on Windows 10, install "App Installer" from the Store first.

```powershell
winget install --id Git.Git -e
winget install --id OpenJS.NodeJS.LTS -e          # Node 22 LTS
winget install --id Rustlang.Rustup -e
winget install --id Microsoft.VisualStudio.2022.BuildTools -e --override "--quiet --wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
winget install --id LLVM.LLVM -e                  # libclang for bindgen
winget install --id Gyan.FFmpeg -e                # ffmpeg + ffprobe on PATH
winget install --id Anthropic.ClaudeCode -e       # or: npm i -g @anthropic-ai/claude-code
```

Then **open a new terminal** so PATH updates take effect, and run:

```powershell
rustup default stable-x86_64-pc-windows-msvc
corepack enable                                   # gives you pnpm
[Environment]::SetEnvironmentVariable("LIBCLANG_PATH", "C:\Program Files\LLVM\bin", "User")
```

WebView2 (which Tauri needs) is built into Windows 11. On Windows 10 it usually arrives through Edge updates.

Check everything is in place: `git --version; node -v; pnpm -v; cargo -V; clang --version; ffmpeg -version`

## 2. Get the code

```powershell
cd $HOME\Documents
git clone git@github.com:cRaZy92/klipt.git    # or https://github.com/cRaZy92/klipt.git
cd klipt
git fetch --all
git switch feat/desktop                       # desktop work continues here
```

Branches:
- `main`: design docs and plans.
- `feat/server`: the finished server (PR pending).
- `spike/capture`: the recording spike and its decision record.
- `feat/desktop`: the desktop app plan, these docs, and the work still to do.

## 3. OBS runtime for `klipt-recorder` (desktop plan, Tasks 1–2)

`klipt-recorder` loads OBS's `obs.dll` at runtime and needs OBS's plugin folders next to it. Desktop plan Task 2 adds `scripts/fetch-obs-windows.ps1`, which automates the steps below. Until then, do it by hand:

1. Download the **OBS Studio 33.0.1 Windows x64 zip** (not the installer) from https://github.com/obsproject/obs-studio/releases/tag/33.0.1 and extract it to `C:\klipt-deps\obs\`. It contains `bin\64bit\obs.dll`, `obs-plugins\64bit\` and `data\`.
2. Headers for bindgen:
   ```powershell
   git clone --depth 1 --branch 33.0.1 https://github.com/obsproject/obs-studio C:\klipt-deps\obs-src
   git clone --depth 1 --branch v0.8.2 https://github.com/simd-everywhere/simde C:\klipt-deps\simde
   [Environment]::SetEnvironmentVariable("OBS_INCLUDE_DIR", "C:\klipt-deps\obs-src\libobs", "User")
   [Environment]::SetEnvironmentVariable("SIMDE_INCLUDE_DIR", "C:\klipt-deps\simde", "User")
   ```
3. Build `klipt-recorder` and copy `klipt-recorder.exe` into `C:\klipt-deps\obs\bin\64bit\`, next to `obs.dll`, `obs-ffmpeg-mux.exe` and `obs-nvenc-test.exe`. It must run from there.

## 4. Things learned on Linux that apply to Windows

- With OBS 33, load modules **explicitly** with `obs_open_module` and `obs_init_module`. Automatic discovery found nothing outside OBS's own executable layout.
- `obs-ffmpeg-mux(.exe)` **must** sit next to the recorder executable, or replay saving fails with "Failed to create process pipe".
- `obs-nvenc-test(.exe)` **must** sit next to it too, or the NVENC module refuses to load.
- Encoder settings that worked: `rate_control=CBR, bitrate=20000, keyint_sec=1, preset2=p4, tune=hq, multipass=disabled, lookahead=false, bf=0, profile=high`.

## 5. Running the server locally (optional, for upload testing)

You need Docker Desktop. Then:

```powershell
git worktree add ..\klipt-server feat/server
cd ..\klipt-server
$env:BASE_URL="http://localhost:3000"; $env:ADMIN_USERNAME="admin"; $env:ADMIN_PASSWORD="changeme123"; $env:POSTGRES_PASSWORD="dev"
docker compose up --build
```

The compose file only `expose`s port 3000. For local use, add a `docker-compose.override.yml` with `ports: ["3000:3000"]` under `server`, and don't commit it.

## Safety rules for the agent (the owner's preference)

- Never kill processes by name or pattern (`taskkill /IM …`, `Stop-Process -Name …`, `pkill -f …`). Stop only processes you started, by their exact PID.
- Prefer test programs that exit on their own, such as `--exit-after N` or a timeout.
