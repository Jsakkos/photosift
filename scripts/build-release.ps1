<#
.SYNOPSIS
  Builds the release PhotoSift: web UI + server exe + runtime DLLs into one folder.

.DESCRIPTION
  Output layout (default <repo>\dist\photosift):
    photosift.exe                      the server (GUI subsystem: no console window)
    web\                               built React UI; the exe serves it from next to itself
    onnxruntime_providers_*.dll        ORT provider DLLs, copied from server\target\release
    fetch-models.ps1                   downloads the face models into the data root
    cudart64_12 / cublas / cufft / cudnn DLLs   only if -CudaDllDir is given or found
    README.txt                         how to run it

  CUDA: ORT ships its CUDA provider DLL but NOT cuBLAS/cuDNN. Either those DLLs
  must be on PATH when photosift.exe starts (the scheduled task inherits the
  user PATH), or they are copied next to the exe by this script. Without them
  the server logs a CUDA init failure and falls back to CPU (never fatal).

  Output goes to a log file, not a console: <data root>\logs\photosift.log.

.PARAMETER OutDir
  Folder that receives the photosift\ release folder. Default <repo>\dist.

.PARAMETER CudaDllDir
  Folder holding the CUDA 12 / cuDNN 9 runtime DLLs to bundle. Defaults to
  C:\Github\photosift\src-tauri\target\debug when that exists (the legacy Tauri
  setup; it is deleted in Task 18, so point this at a permanent folder then).
#>
[CmdletBinding()]
param(
    [string]$OutDir,
    [string]$CudaDllDir
)

$ErrorActionPreference = 'Stop'

$repo = Split-Path -Parent $PSScriptRoot
if (-not $OutDir) { $OutDir = Join-Path $repo 'dist' }
if (-not $CudaDllDir) {
    $legacy = 'C:\Github\photosift\src-tauri\target\debug'
    if (Test-Path -LiteralPath $legacy) { $CudaDllDir = $legacy }
}

function Invoke-Native([string]$What, [scriptblock]$Command) {
    & $Command
    if ($LASTEXITCODE -ne 0) { throw "$What failed (exit $LASTEXITCODE)" }
}

$web = Join-Path $repo 'web'
if (-not (Test-Path -LiteralPath (Join-Path $web 'node_modules'))) {
    Write-Host '== npm ci (web)'
    Invoke-Native 'npm ci' { npm --prefix $web ci }
}
Write-Host '== building web UI'
Invoke-Native 'web build' { npm --prefix $web run build }

Write-Host '== building server (release)'
$manifest = Join-Path $repo 'server\Cargo.toml'
Invoke-Native 'cargo build' { cargo build --release --manifest-path $manifest }

$release = Join-Path $repo 'server\target\release'
$dest = Join-Path $OutDir 'photosift'
if (Test-Path -LiteralPath $dest) { Remove-Item -LiteralPath $dest -Recurse -Force }
New-Item -ItemType Directory -Force -Path $dest | Out-Null

Copy-Item -LiteralPath (Join-Path $release 'photosift.exe') -Destination $dest
Copy-Item -LiteralPath (Join-Path $web 'dist') -Destination (Join-Path $dest 'web') -Recurse
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'fetch-models.ps1') -Destination $dest

foreach ($name in 'onnxruntime_providers_shared.dll', 'onnxruntime_providers_cuda.dll') {
    $src = Join-Path $release $name
    if (Test-Path -LiteralPath $src) {
        Copy-Item -LiteralPath $src -Destination $dest
    }
    else {
        Write-Warning "$name not found in $release; GPU inference will be unavailable (CPU fallback)."
    }
}

$cudaPatterns = 'cudart64_12*.dll', 'cublas64_12*.dll', 'cublasLt64_12*.dll', 'cufft64_11*.dll', 'cudnn*64_9*.dll'
if ($CudaDllDir -and (Test-Path -LiteralPath $CudaDllDir)) {
    $copied = 0
    foreach ($pattern in $cudaPatterns) {
        foreach ($f in Get-ChildItem -LiteralPath $CudaDllDir -Filter $pattern -File) {
            Copy-Item -LiteralPath $f.FullName -Destination $dest -Force
            $copied++
        }
    }
    Write-Host "== bundled $copied CUDA/cuDNN DLLs from $CudaDllDir"
}
else {
    Write-Warning ('No -CudaDllDir (and the legacy folder is absent): CUDA/cuDNN DLLs are not bundled. ' +
        'They must be on PATH for photosift.exe, or inference falls back to CPU.')
}

$readme = @'
PhotoSift release folder

  photosift.exe    Start it (or let the logon task do so); then open http://127.0.0.1:7420/
                   Running it again with --open opens the browser and does not start a second server.
  fetch-models.ps1 powershell -File fetch-models.ps1   (downloads the face models into %USERPROFILE%\.photosift\models)
  Logs             %USERPROFILE%\.photosift\logs\photosift.log (there is no console window)
  CUDA             cuBLAS/cuDNN DLLs must sit next to photosift.exe or be on PATH; otherwise CPU is used.
'@
[System.IO.File]::WriteAllText((Join-Path $dest 'README.txt'), $readme, (New-Object System.Text.UTF8Encoding($false)))

Write-Host "== done: $dest"
