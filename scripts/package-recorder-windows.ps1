<#
.SYNOPSIS
  Assembles klipt-recorder.exe with the subset of the OBS runtime it needs.

.DESCRIPTION
  Mirrors OBS's own layout, because libobs resolves paths relative to it:
    bin\64bit\              obs.dll + runtime DLLs, klipt-recorder.exe, obs-ffmpeg-mux.exe, encoder test exes
    core\<module>\          only the modules klipt-recorder loads (no .pdb files)
    data\libobs\            libobs effects
    TEST-ON-WINDOWS.md

  Left out: the OBS UI (obs64.exe, Qt), scripting, OpenGL, debug symbols, and win-capture's
  game-capture injection helpers (Klipt captures the display only).

.EXAMPLE
  scripts\package-recorder-windows.ps1 -ObsDir C:\klipt-deps\obs -RecorderExe desktop\target\release\klipt-recorder.exe -Out dist\klipt-recorder-windows
#>
param(
  [Parameter(Mandatory)] [string]$ObsDir,
  [Parameter(Mandatory)] [string]$RecorderExe,
  [Parameter(Mandatory)] [string]$Out
)

$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$modules = 'win-capture', 'win-wasapi', 'obs-ffmpeg', 'obs-outputs', 'obs-x264', 'obs-nvenc', 'obs-qsv11'

if (-not (Test-Path (Join-Path $ObsDir 'bin\64bit\obs.dll'))) { throw "$ObsDir does not contain bin\64bit\obs.dll" }
if (-not (Test-Path $RecorderExe)) { throw "$RecorderExe not found" }
if (Test-Path $Out) { Remove-Item -Recurse -Force $Out }

$bin = Join-Path $Out 'bin\64bit'
New-Item -ItemType Directory -Force -Path $bin | Out-Null
$skipBin = '^(Qt6.*|obs64\.exe|obs-frontend-api\.dll|obs-scripting\.dll|lua51\.dll|libobs-opengl\.dll|.*\.pdb)$'
Get-ChildItem -File (Join-Path $ObsDir 'bin\64bit') |
  Where-Object { $_.Name -notmatch $skipBin } |
  Copy-Item -Destination $bin
Copy-Item $RecorderExe -Destination $bin

$skipModuleFile = '^(.*\.pdb|graphics-hook(32|64)\.dll|inject-helper(32|64)\.exe|get-graphics-offsets(32|64)\.exe|obs-vulkan(32|64)\.json)$'
foreach ($m in $modules) {
  $src = Join-Path $ObsDir "core\$m"
  if (-not (Test-Path $src)) { Write-Warning "module $m not in this OBS build, skipped"; continue }
  $dst = Join-Path $Out "core\$m"
  Get-ChildItem -Recurse -File $src | Where-Object { $_.Name -notmatch $skipModuleFile } | ForEach-Object {
    $target = Join-Path $dst $_.FullName.Substring($src.Length + 1)
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $target) | Out-Null
    Copy-Item $_.FullName -Destination $target
  }
}

New-Item -ItemType Directory -Force -Path (Join-Path $Out 'data') | Out-Null
Copy-Item -Recurse (Join-Path $ObsDir 'data\libobs') -Destination (Join-Path $Out 'data\libobs')
Copy-Item (Join-Path $repo 'desktop\recorder-obs\TEST-ON-WINDOWS.md') -Destination $Out

foreach ($required in 'bin\64bit\klipt-recorder.exe', 'bin\64bit\obs.dll', 'bin\64bit\obs-ffmpeg-mux.exe', 'bin\64bit\libobs-d3d11.dll', 'core\obs-outputs\obs-outputs.dll', 'data\libobs\default.effect') {
  if (-not (Test-Path (Join-Path $Out $required))) { throw "package is missing $required" }
}
$size = (Get-ChildItem -Recurse -File $Out | Measure-Object -Sum Length).Sum
Write-Host ("Packaged klipt-recorder in {0} ({1:N0} MB)" -f $Out, ($size / 1MB))
