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
$uri = if ($Tag -eq 'latest') { 'https://api.github.com/repos/BtbN/FFmpeg-Builds/releases/latest' }
       else { "https://api.github.com/repos/BtbN/FFmpeg-Builds/releases/tags/$Tag" }
$release = Invoke-RestMethod -Headers $headers -Uri $uri
$asset = $release.assets |
  Where-Object { $_.name -match '^ffmpeg-n[0-9.]+-latest-win64-gpl-[0-9.]+\.zip$' } |
  Sort-Object name -Descending | Select-Object -First 1
if (-not $asset) { throw "no win64 GPL release build in BtbN release $($release.tag_name)" }
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
