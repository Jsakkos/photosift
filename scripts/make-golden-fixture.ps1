<#
.SYNOPSIS
  Builds a fake camera card from real photos listed in a local manifest.

.DESCRIPTION
  The manifest (tests\golden\manifest.txt, gitignored) lists one absolute RAW
  path per line; blank lines and lines starting with # are ignored. Each RAW is
  copied into the fake card, together with any same-stem .JPG beside it:
    <Out>\DCIM\100_FUJI\*.RAF, *.JPG
    <Out>\DCIM\100NIKON\*.NEF
  Also writes <Out>\expected.toml with the counts the golden test asserts.
  Safe to re-run: the DCIM folder and expected.toml are rebuilt from scratch.
  Refuses (deleting nothing) unless <Out> is new, empty, or carries the
  .photosift-golden-fixture marker this script writes; never a drive root.
  The source files are only read, never modified.

.PARAMETER Out
  Card directory. Defaults to $env:PHOTOSIFT_GOLDEN_DIR, else
  <repo>\.photosift-golden\card.

.PARAMETER Manifest
  Defaults to <repo>\tests\golden\manifest.txt.
#>
[CmdletBinding()]
param(
    [string]$Out,
    [string]$Manifest
)

$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot

if (-not $Manifest) { $Manifest = Join-Path $repo 'tests\golden\manifest.txt' }
if (-not $Out) {
    if ($env:PHOTOSIFT_GOLDEN_DIR) { $Out = $env:PHOTOSIFT_GOLDEN_DIR }
    else { $Out = Join-Path $repo '.photosift-golden\card' }
}
if (-not [System.IO.File]::Exists($Manifest)) {
    throw "Manifest not found: $Manifest. Copy tests\golden\manifest.example.txt to tests\golden\manifest.txt and list your own files."
}

$Out = [System.IO.Path]::GetFullPath($Out)
$root = [System.IO.Path]::GetPathRoot($Out)
if ($Out.TrimEnd([char]92, [char]47) -eq $root.TrimEnd([char]92, [char]47)) {
    throw "Refusing to use a drive root as the fixture dir: $Out"
}

# Data-safety guard: this script deletes <Out>\DCIM, so it only does so in a
# directory it created itself (marker file) or one that is new/empty. A real
# card or library passed by mistake is left untouched.
$marker = Join-Path $Out '.photosift-golden-fixture'
$outExists = [System.IO.Directory]::Exists($Out)
$outEmpty = (-not $outExists) -or (-not (@([System.IO.Directory]::EnumerateFileSystemEntries($Out)).Count))
if (-not ($outEmpty -or [System.IO.File]::Exists($marker))) {
    throw "Refusing to touch $Out : it is not empty and has no .photosift-golden-fixture marker, so it was not created by this script. Nothing was deleted. Point -Out / PHOTOSIFT_GOLDEN_DIR at a new or empty folder."
}
[System.IO.Directory]::CreateDirectory($Out) | Out-Null
[System.IO.File]::WriteAllText($marker, "Created by scripts/make-golden-fixture.ps1; safe to rebuild.`r`n", (New-Object System.Text.UTF8Encoding $false))

$dcim = Join-Path $Out 'DCIM'
if ([System.IO.Directory]::Exists($dcim)) { [System.IO.Directory]::Delete($dcim, $true) }
$expectedPath = Join-Path $Out 'expected.toml'
if ([System.IO.File]::Exists($expectedPath)) { [System.IO.File]::Delete($expectedPath) }

$dirFor = @{ '.raf' = '100_FUJI'; '.jpg' = '100_FUJI'; '.nef' = '100NIKON' }
$rawCount = 0
$copied = 0
$seen = New-Object 'System.Collections.Generic.HashSet[string]' ([System.StringComparer]::OrdinalIgnoreCase)

function Copy-IntoCard([string]$src) {
    $ext = [System.IO.Path]::GetExtension($src).ToLowerInvariant()
    $sub = $dirFor[$ext]
    if (-not $sub) { throw "Unsupported extension in manifest: $ext" }
    if (-not $seen.Add($src)) { return $false }
    $destDir = Join-Path $dcim $sub
    [System.IO.Directory]::CreateDirectory($destDir) | Out-Null
    [System.IO.File]::Copy($src, (Join-Path $destDir ([System.IO.Path]::GetFileName($src))), $true)
    $script:copied++
    return $true
}

foreach ($line in [System.IO.File]::ReadAllLines($Manifest, [System.Text.Encoding]::UTF8)) {
    $path = $line.Trim()
    if ($path -eq '' -or $path.StartsWith('#')) { continue }
    if (-not [System.IO.File]::Exists($path)) { throw "Manifest entry not found on disk (line: $path)" }
    $ext = [System.IO.Path]::GetExtension($path).ToLowerInvariant()
    if ($ext -eq '.raf' -or $ext -eq '.nef') {
        if (Copy-IntoCard $path) { $rawCount++ }
        $stem = [System.IO.Path]::GetFileNameWithoutExtension($path)
        $dir = [System.IO.Path]::GetDirectoryName($path)
        foreach ($jx in '.JPG', '.jpg') {
            $sibling = Join-Path $dir ($stem + $jx)
            if ([System.IO.File]::Exists($sibling)) { [void](Copy-IntoCard $sibling); break }
        }
    } else {
        [void](Copy-IntoCard $path)
    }
}

# cameras and events follow from the manifest design (not computed): one
# Fujifilm body plus one Nikon body, and with zero camera offsets the scan
# proposes three events - the other day, an early Nikon frame that sits more
# than event_gap_minutes before the main shoot, and the main shoot itself.
$toml = "photos = $rawCount`r`ncameras = 2`r`nevents = 3`r`n"
[System.IO.File]::WriteAllText($expectedPath, $toml, (New-Object System.Text.UTF8Encoding $false))

Write-Host "Golden card ready: $Out ($rawCount photos, $copied files copied)"
Write-Host "Run: `$env:PHOTOSIFT_GOLDEN_DIR = '$Out'; cargo test --manifest-path server/Cargo.toml --test golden --test media -- --ignored"
