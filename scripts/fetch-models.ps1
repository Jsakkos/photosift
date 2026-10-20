<#
.SYNOPSIS
  Downloads the two face models PhotoSift uses into <data root>\models.

.DESCRIPTION
  YuNet (face detection) and SFace (face identity) from the official
  opencv/opencv_zoo repository. Models are never committed to this repo.
  A download is accepted only if its size is in the expected range (a fast
  check that also rejects the tiny Git LFS pointer file a wrong URL would
  return) and its SHA-256 matches the pinned value below.

.PARAMETER DataRoot
  Data root to install into. Defaults to PHOTOSIFT_HOME, else ~\.photosift
  (or ~\.photosift-dev with -Dev).

.PARAMETER Dev
  Use the debug-build data root (~\.photosift-dev).

.PARAMETER Force
  Download again even if a correctly sized file is already there.
#>
[CmdletBinding()]
param(
    [string]$DataRoot,
    [switch]$Dev,
    [switch]$Force
)

$ErrorActionPreference = 'Stop'

if (-not $DataRoot) {
    if ($env:PHOTOSIFT_HOME) {
        $DataRoot = $env:PHOTOSIFT_HOME
    }
    else {
        $leaf = if ($Dev) { '.photosift-dev' } else { '.photosift' }
        $DataRoot = Join-Path $env:USERPROFILE $leaf
    }
}
$models = Join-Path $DataRoot 'models'
New-Item -ItemType Directory -Force -Path $models | Out-Null

$base = 'https://github.com/opencv/opencv_zoo/raw/main/models'
# Sizes are byte ranges: YuNet is about 232 KB, SFace about 38.7 MB. The SHA-256
# pins are the hashes of the files the app was developed and tested with.
$files = @(
    @{ Name = 'face_detection_yunet_2023mar.onnx';   Path = 'face_detection_yunet';   Min = 200000;   Max = 300000;
       Sha256 = '8f2383e4dd3cfbb4553ea8718107fc0423210dc964f9f4280604804ed2552fa4' },
    @{ Name = 'face_recognition_sface_2021dec.onnx'; Path = 'face_recognition_sface'; Min = 38000000; Max = 40000000;
       Sha256 = '0ba9fbfa01b5270c96627c4ef784da859931e02f04419c829e83484087c34e79' }
)

function Test-Sha256([string]$Path, [string]$Expected) {
    (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash -ieq $Expected
}

foreach ($f in $files) {
    $dest = Join-Path $models $f.Name
    if (-not $Force -and (Test-Path -LiteralPath $dest)) {
        $len = (Get-Item -LiteralPath $dest).Length
        if ($len -ge $f.Min -and $len -le $f.Max -and (Test-Sha256 $dest $f.Sha256)) {
            Write-Host "$($f.Name): already present ($len bytes, SHA-256 ok)"
            continue
        }
    }
    $url = "$base/$($f.Path)/$($f.Name)"
    $tmp = "$dest.download"
    Write-Host "$($f.Name): downloading $url"
    try {
        Invoke-WebRequest -Uri $url -OutFile $tmp -UseBasicParsing
        $len = (Get-Item -LiteralPath $tmp).Length
        if ($len -lt $f.Min -or $len -gt $f.Max) {
            throw "$($f.Name): downloaded $len bytes, expected between $($f.Min) and $($f.Max)"
        }
        if (-not (Test-Sha256 $tmp $f.Sha256)) {
            throw "$($f.Name): SHA-256 mismatch (expected $($f.Sha256)); the download is not the file PhotoSift was built against"
        }
        Move-Item -LiteralPath $tmp -Destination $dest -Force
        Write-Host "$($f.Name): ok ($len bytes)"
    }
    finally {
        if (Test-Path -LiteralPath $tmp) { Remove-Item -LiteralPath $tmp -Force }
    }
}

Write-Host "Models are in $models"
