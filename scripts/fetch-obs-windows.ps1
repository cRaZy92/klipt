<#
.SYNOPSIS
  Downloads the OBS Studio Windows runtime and the headers that klipt-recorder is built against.

.DESCRIPTION
  Produces this layout under -Dest:
    obs\              OBS release zip, extracted (bin\64bit\obs.dll, obs-plugins\64bit\, data\)
    include\obs\      libobs headers from the obs-studio source at the same tag (obs.h is at the top)
    include\simde\    SIMDe, needed by bindgen to parse obs.h

  Safe to re-run: steps whose output already exists are skipped.
  Works on Windows PowerShell 5.1 and PowerShell 7.

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File scripts\fetch-obs-windows.ps1 -Dest C:\klipt-deps
#>
param(
  [string]$Version = '33.0.1',
  [string]$Dest = 'C:\klipt-deps',
  [string]$SimdeVersion = 'v0.8.2'
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue' # the progress bar makes Invoke-WebRequest very slow on PS 5.1
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

function Invoke-Git {
  & git -c advice.detachedHead=false @args
  if ($LASTEXITCODE -ne 0) { throw "git $($args -join ' ') failed with exit code $LASTEXITCODE" }
}

New-Item -ItemType Directory -Force -Path $Dest | Out-Null
$Dest = (Resolve-Path $Dest).Path

# --- 1. OBS runtime ---------------------------------------------------------------------------
$obsDir = Join-Path $Dest 'obs'
if (Test-Path (Join-Path $obsDir 'bin\64bit\obs.dll')) {
  Write-Host "OBS runtime already present in $obsDir"
} else {
  $headers = @{ 'User-Agent' = 'klipt-fetch-obs' }
  if ($env:GITHUB_TOKEN) { $headers['Authorization'] = "Bearer $env:GITHUB_TOKEN" }
  $release = Invoke-RestMethod -Headers $headers -Uri "https://api.github.com/repos/obsproject/obs-studio/releases/tags/$Version"
  $asset = $release.assets |
    Where-Object { $_.name -match 'Windows.*x64.*\.zip$' -and $_.name -notmatch 'PDB' } |
    Select-Object -First 1
  if (-not $asset) { throw "OBS $Version has no Windows x64 zip asset" }

  $zip = Join-Path $Dest $asset.name
  Write-Host "Downloading $($asset.name) ($([math]::Round($asset.size / 1MB)) MB)"
  Invoke-WebRequest -Headers @{ 'User-Agent' = 'klipt-fetch-obs' } -Uri $asset.browser_download_url -OutFile $zip

  $staging = Join-Path $Dest 'obs-staging'
  if (Test-Path $staging) { Remove-Item -Recurse -Force $staging }
  Expand-Archive -Path $zip -DestinationPath $staging
  # Some releases wrap everything in one top-level folder; find the folder that holds bin\64bit\obs.dll.
  $dll = Get-ChildItem -Path $staging -Recurse -Filter obs.dll | Where-Object { $_.Directory.Name -eq '64bit' } | Select-Object -First 1
  if (-not $dll) { throw "obs.dll not found in $($asset.name)" }
  $root = $dll.Directory.Parent.Parent.FullName
  if (Test-Path $obsDir) { Remove-Item -Recurse -Force $obsDir }
  Move-Item -Path $root -Destination $obsDir
  if (Test-Path $staging) { Remove-Item -Recurse -Force $staging }
  Remove-Item $zip
  Write-Host "OBS runtime extracted to $obsDir"
}

# --- 2. libobs headers ------------------------------------------------------------------------
$includeDir = Join-Path $Dest 'include'
$obsInclude = Join-Path $includeDir 'obs'
if (Test-Path (Join-Path $obsInclude 'obs.h')) {
  Write-Host "libobs headers already present in $obsInclude"
} else {
  $src = Join-Path $Dest 'obs-src-tmp'
  if (Test-Path $src) { Remove-Item -Recurse -Force $src }
  Invoke-Git clone --quiet --depth 1 --branch $Version --filter=blob:none --sparse https://github.com/obsproject/obs-studio $src
  Invoke-Git -C $src sparse-checkout set libobs
  New-Item -ItemType Directory -Force -Path $obsInclude | Out-Null
  # robocopy keeps the folder structure (util\, graphics\, media-io\, ...) and copies headers only.
  robocopy (Join-Path $src 'libobs') $obsInclude *.h *.hpp /S /NFL /NDL /NJH /NJS /NP | Out-Null
  if ($LASTEXITCODE -ge 8) { throw "robocopy of libobs headers failed with exit code $LASTEXITCODE" }
  $global:LASTEXITCODE = 0
  Remove-Item -Recurse -Force $src
  Write-Host "libobs headers copied to $obsInclude"
}

# --- 3. SIMDe ---------------------------------------------------------------------------------
$simdeDir = Join-Path $includeDir 'simde'
if (Test-Path (Join-Path $simdeDir 'simde\x86\sse2.h')) {
  Write-Host "SIMDe already present in $simdeDir"
} else {
  if (Test-Path $simdeDir) { Remove-Item -Recurse -Force $simdeDir }
  Invoke-Git clone --quiet --depth 1 --branch $SimdeVersion https://github.com/simd-everywhere/simde $simdeDir
  Write-Host "SIMDe cloned to $simdeDir"
}

Write-Host ''
Write-Host 'Set these before building klipt-recorder:'
Write-Host "  OBS_INCLUDE_DIR   = $obsInclude"
Write-Host "  SIMDE_INCLUDE_DIR = $simdeDir"
Write-Host "  OBS runtime       = $(Join-Path $obsDir 'bin\64bit')  (copy klipt-recorder.exe here)"
