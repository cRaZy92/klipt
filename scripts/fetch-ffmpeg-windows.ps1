<#
.SYNOPSIS
  Downloads ffmpeg.exe and ffprobe.exe (BtbN win64 GPL build) into -Dest.

.DESCRIPTION
  Uses the BtbN FFmpeg-Builds release named by -Tag ("latest" by default) and picks the
  release-branch build (ffmpeg-nX.Y-latest-win64-gpl-X.Y.zip), not the master snapshot, so
  the bundled ffmpeg changes only with a new FFmpeg release.
#>
param(
  [Parameter(Mandatory)] [string]$Dest,
  [string]$Tag = 'latest'
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$headers = @{ 'User-Agent' = 'klipt-fetch-ffmpeg' }
if ($env:GITHUB_TOKEN) { $headers['Authorization'] = "Bearer $env:GITHUB_TOKEN" }
$pattern = '^ffmpeg-n[0-9].*-win64-gpl-[0-9.]+\.zip$' # release-branch builds (nX.Y…), not master or shared
# BtbN re-publishes its daily releases, so a release can briefly have no assets: take the newest
# release (or -Tag) that has the build, and retry a few times.
$asset = $null
for ($try = 1; $try -le 4 -and -not $asset; $try++) {
  try {
    $releases = if ($Tag -eq 'latest') {
      Invoke-RestMethod -Headers $headers -Uri 'https://api.github.com/repos/BtbN/FFmpeg-Builds/releases?per_page=10'
    } else {
      @(Invoke-RestMethod -Headers $headers -Uri "https://api.github.com/repos/BtbN/FFmpeg-Builds/releases/tags/$Tag")
    }
    foreach ($r in $releases) {
      $asset = $r.assets | Where-Object { $_.name -match $pattern } | Sort-Object name -Descending | Select-Object -First 1
      if ($asset) { $release = $r; break }
    }
  } catch { Write-Host "attempt $try failed: $_" }
  if (-not $asset) { Start-Sleep -Seconds (15 * $try) }
}
if (-not $asset) { throw "no win64 GPL release build found in BtbN's recent releases" }
Write-Host "ffmpeg: $($asset.name) from $($release.tag_name)"

$tmp = Join-Path ([IO.Path]::GetTempPath()) "klipt-ffmpeg"
if (Test-Path $tmp) { Remove-Item -Recurse -Force $tmp }
New-Item -ItemType Directory -Force -Path $tmp | Out-Null
$zip = Join-Path $tmp $asset.name
Invoke-WebRequest -Headers @{ 'User-Agent' = 'klipt-fetch-ffmpeg' } -Uri $asset.browser_download_url -OutFile $zip
Expand-Archive -Path $zip -DestinationPath $tmp

New-Item -ItemType Directory -Force -Path $Dest | Out-Null
foreach ($exe in 'ffmpeg.exe', 'ffprobe.exe') {
  $found = Get-ChildItem -Recurse -File -Path $tmp -Filter $exe | Select-Object -First 1
  if (-not $found) { throw "$exe not in $($asset.name)" }
  Copy-Item $found.FullName -Destination (Join-Path $Dest $exe)
}
