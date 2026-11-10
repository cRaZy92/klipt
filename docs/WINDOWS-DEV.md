# Developing Klipt on Windows

Do **not** copy the Linux project folder. `node_modules`, `target/` and the git worktree metadata are Linux-specific. Clone from GitHub instead.

## 1. Install the tools (one time)

Run these in PowerShell. `winget` ships with Windows 11; on Windows 10, install "App Installer" from the Store first.

```powershell
winget install --id Git.Git -e
winget install --id OpenJS.NodeJS.22 -e           # Node 22 (the server image uses it; "OpenJS.NodeJS.LTS" is now Node 26, which has no corepack)
winget install --id Rustlang.Rustup -e
winget install --id Microsoft.VisualStudio.2022.BuildTools -e --override "--quiet --wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
winget install --id LLVM.LLVM -e                  # libclang for bindgen
winget install --id Gyan.FFmpeg -e                # ffmpeg + ffprobe on PATH
winget install --id GitHub.cli -e                 # gh, for PRs (then: gh auth login)
winget install --id Anthropic.ClaudeCode -e       # or: npm i -g @anthropic-ai/claude-code
```

Then **open a new terminal** so PATH updates take effect, and run:

```powershell
rustup default stable-x86_64-pc-windows-msvc
# Plain `corepack enable` fails with EPERM (it writes into Program Files); put the pnpm shim in the
# per-user npm folder instead, which the Node installer already adds to PATH.
corepack enable --install-directory "$env:APPDATA\npm"
[Environment]::SetEnvironmentVariable("LIBCLANG_PATH", "C:\Program Files\LLVM\bin", "User")
```

WebView2 (which Tauri needs) is built into Windows 11. On Windows 10 it usually arrives through Edge updates.

Check everything is in place: `git --version; node -v; pnpm -v; cargo -V; ffmpeg -version; Test-Path "C:\Program Files\LLVM\bin\libclang.dll"` (the LLVM installer doesn't put `clang` on PATH; bindgen only needs `LIBCLANG_PATH`).

A fresh Windows install has no git identity. Set it for this repo (`git config user.name …`, `git config user.email …`) before committing.

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

`klipt-recorder` loads OBS's `obs.dll` at runtime and needs OBS's plugin folders around it.

```powershell
powershell -ExecutionPolicy Bypass -File scripts\fetch-obs-windows.ps1 -Dest C:\klipt-deps
[Environment]::SetEnvironmentVariable("OBS_INCLUDE_DIR", "C:\klipt-deps\include\obs", "User")
[Environment]::SetEnvironmentVariable("SIMDE_INCLUDE_DIR", "C:\klipt-deps\include\simde", "User")
```

The script downloads the OBS 33.0.1 Windows x64 zip to `C:\klipt-deps\obs\` (`bin\64bit\obs.dll`, plugins in `core\<module>\`, `data\`), the libobs headers to `include\obs\` and SIMDe to `include\simde\`.

Build and run (from `desktop\`, in a new terminal so the variables are set):

```powershell
cargo build -p klipt-recorder --release
Copy-Item target\release\klipt-recorder.exe C:\klipt-deps\obs\bin\64bit\
C:\klipt-deps\obs\bin\64bit\klipt-recorder.exe --list
```

It must run from `bin\64bit`, next to `obs.dll`, `obs-ffmpeg-mux.exe` and `obs-nvenc-test.exe`. Tests that drive real OBS: `$env:KLIPT_TEST_OBS_ROOT="C:\klipt-deps\obs"; cargo test -p klipt-recorder`.

`scripts\package-recorder-windows.ps1` builds the trimmed (≈57 MB) folder that CI uploads and friends test with, including `TEST-ON-WINDOWS.md`.

## 4. libobs facts that matter here

- Load modules **explicitly** with `obs_open_module` and `obs_init_module`. Automatic discovery finds nothing outside OBS's own executable layout.
- OBS 33 on Windows keeps plugins in `core\<name>\<name>.dll` with data in `core\<name>\data\` (not `obs-plugins\64bit`).
- libobs loads the graphics module and plugins with `LOAD_LIBRARY_SEARCH_DEFAULT_DIRS`: only the exe's folder, System32 and `AddDllDirectory` folders are searched, never PATH or the working directory. libobs's own data is found at `..\..\data\libobs\` relative to the **working directory**.
- `obs-ffmpeg-mux.exe` **must** sit next to the recorder executable, or replay saving fails with "Failed to create process pipe". `obs-nvenc-test.exe` / `obs-amf-test.exe` must be there too, or that encoder is unavailable.
- libobs's default log handler prints to **stdout**; klipt-recorder installs its own that prints to stderr.
- Encoder settings: `rate_control=CBR, bitrate=20000, keyint_sec=1, preset=p4, tune=hq, multipass=disabled, lookahead=false, bf=0, profile=high`. OBS 30.2+'s NVENC reads `preset`; the old `preset2` key is silently ignored (you get p5). Check the encoder's startup log to confirm settings.

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
