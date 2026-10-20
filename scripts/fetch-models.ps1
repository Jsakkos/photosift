<#
.SYNOPSIS
  Downloads the two face models PhotoSift uses into <data root>\models.

.DESCRIPTION
  YuNet (face detection) and SFace (face identity) from the official
  opencv/opencv_zoo repository. Models are never committed to this repo.
  A download is accepted only if its size is in the expected range, which
  also rejects the tiny Git LFS pointer file a wrong URL would return.

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
# Sizes are byte ranges: YuNet is about 232 KB, SFace about 38.7 MB.
$files = @(
    @{ Name = 'face_detection_yunet_2023mar.onnx';   Path = 'face_detection_yunet';   Min = 200000;   Max = 300000 },
    @{ Name = 'face_recognition_sface_2021dec.onnx'; Path = 'face_recognition_sface'; Min = 38000000; Max = 40000000 }
)

foreach ($f in $files) {
    $dest = Join-Path $models $f.Name
    if (-not $Force -and (Test-Path -LiteralPath $dest)) {
        $len = (Get-Item -LiteralPath $dest).Length
        if ($len -ge $f.Min -and $len -le $f.Max) {
            Write-Host "$($f.Name): already present ($len bytes)"
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
        Move-Item -LiteralPath $tmp -Destination $dest -Force
        Write-Host "$($f.Name): ok ($len bytes)"
    }
    finally {
        if (Test-Path -LiteralPath $tmp) { Remove-Item -LiteralPath $tmp -Force }
    }
}

Write-Host "Models are in $models"
