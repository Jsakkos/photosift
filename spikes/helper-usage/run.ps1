# Usage-spike runner: curate a folder of camera JPGs with `claude -p` and log cost/usage.
# .\run.ps1 -Source <folder of JPGs> -Model sonnet|opus [-Crops] [-Probe] [-Limit N]
# Everything it writes goes under spikes/helper-usage/out/ (gitignored). Source is read-only.
param(
  [Parameter(Mandatory)][string]$Source,
  [Parameter(Mandatory)][string]$Model,
  [switch]$Crops,
  [switch]$Probe,       # 10 photos, stream-json, saves the system/init event
  [int]$Limit = 0,
  [double]$MaxBudget = 15,
  [int]$MaxTurns = 40
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
$root = Join-Path $PSScriptRoot 'out'
$thumbs = Join-Path $root 'thumbs'; $cropDir = Join-Path $root 'crops'; $cwd = Join-Path $root 'cwd'
foreach ($d in $root, $thumbs, $cropDir, $cwd) { New-Item -ItemType Directory -Force $d | Out-Null }
Get-ChildItem $cwd -Force | Remove-Item -Recurse -Force   # empty cwd: no CLAUDE.md / .mcp.json pickup

function Save-Scaled($src, $dst, $long, $q) {
  $img = [System.Drawing.Image]::FromFile($src)
  try {
    $s = $long / [Math]::Max($img.Width, $img.Height); if ($s -gt 1) { $s = 1 }
    $w = [int]($img.Width * $s); $h = [int]($img.Height * $s)
    $bmp = New-Object System.Drawing.Bitmap $w, $h
    $g = [System.Drawing.Graphics]::FromImage($bmp); $g.InterpolationMode = 'HighQualityBicubic'
    $g.DrawImage($img, 0, 0, $w, $h); $g.Dispose()
    $enc = [System.Drawing.Imaging.ImageCodecInfo]::GetImageEncoders() | Where-Object MimeType -eq 'image/jpeg'
    $p = New-Object System.Drawing.Imaging.EncoderParameters 1
    $p.Param[0] = New-Object System.Drawing.Imaging.EncoderParameter ([System.Drawing.Imaging.Encoder]::Quality), ([long]$q)
    $bmp.Save($dst, $enc, $p); $bmp.Dispose()
  } finally { $img.Dispose() }
}
function Get-Taken($path) {
  $img = [System.Drawing.Image]::FromFile($path)
  try {
    $pi = $img.PropertyItems | Where-Object Id -eq 0x9003 | Select-Object -First 1
    if ($pi) {
      $t = [Text.Encoding]::ASCII.GetString($pi.Value).Trim([char]0)
      return [datetime]::ParseExact($t, 'yyyy:MM:dd HH:mm:ss', $null)
    }
  } finally { $img.Dispose() }
  return (Get-Item $path).LastWriteTime
}

$files = Get-ChildItem $Source -File | Where-Object { $_.Extension -match '^\.jpe?g$' } | Sort-Object Name
if ($Probe) { $Limit = 10 }
if ($Limit -gt 0) { $files = $files | Select-Object -First $Limit }
Get-ChildItem $thumbs, $cropDir -File | Remove-Item -Force
$photos = @(); $i = 0
foreach ($f in $files) {
  $i++; $id = 'p{0:D4}' -f $i
  Save-Scaled $f.FullName (Join-Path $thumbs "$id.jpg") 400 80
  $photos += [pscustomobject]@{ id = $id; src = $f.FullName; taken = (Get-Taken $f.FullName) }
}
$photos = $photos | Sort-Object taken
$moments = @(); $cur = $null; $prev = $null
foreach ($p in $photos) {
  if (-not $cur -or ($p.taken - $prev).TotalSeconds -gt 90) { $cur = New-Object System.Collections.ArrayList; $moments += , $cur }
  [void]$cur.Add($p); $prev = $p.taken
}
$manifest = New-Object System.Text.StringBuilder
$mi = 0
foreach ($m in $moments) {
  $mi++; [void]$manifest.AppendLine("moment $mi ($($m.Count) photos):")
  foreach ($p in $m) { [void]$manifest.AppendLine("  $($p.id) = $($thumbs -replace '\\','/')/$($p.id).jpg") }
}

$schema = @{ type = 'object'; required = @('album_picks', 'your_call_pairs', 'shortlist')
  properties = @{
    album_picks = @{ type = 'array'; items = @{ type = 'object'; required = @('id', 'reason'); properties = @{ id = @{ type = 'string' }; reason = @{ type = 'string' } } } }
    your_call_pairs = @{ type = 'array'; items = @{ type = 'object'; required = @('a', 'b', 'question'); properties = @{ a = @{ type = 'string' }; b = @{ type = 'string' }; question = @{ type = 'string' } } } }
    shortlist = @{ type = 'array'; items = @{ type = 'string' } } } } | ConvertTo-Json -Depth 8 -Compress

$sys = 'You help a parent pick the best family-album photos from one shoot. You only annotate and suggest; the user decides. Look at the 400 px thumbnails with the Read tool (image files). Be concise.'
[IO.File]::WriteAllText((Join-Path $root 'system.txt'), $sys)
$prompt = @"
Thumbnails of one shoot, grouped into moments (90 s gap). Read the images as needed (a few per call is fine).
$($manifest.ToString())
Return structured output: album_picks (id + one-line reason; pick about one in ten, at most 2 per moment), your_call_pairs (near-duplicate pairs where you cannot decide, with a one-line question), and shortlist (exactly the 30 best candidate ids, used for a closer look).
"@

$verStr = ((& claude --version) -split ' ')[0]; $ver = [version]$verStr
$flags = @('-p', '--model', $Model, '--strict-mcp-config', '--tools', 'Read', '--add-dir', $root,
  '--setting-sources', 'local', '--disable-slash-commands', '--permission-mode', 'dontAsk',
  '--allowedTools', 'Read', '--system-prompt-file', (Join-Path $root 'system.txt'),
  '--max-turns', $MaxTurns, '--max-budget-usd', $MaxBudget, '--session-id', ([guid]::NewGuid().ToString()),
  '--json-schema', ($schema -replace '"', '\"'))   # Windows PowerShell 5.1 drops embedded quotes passed to native exes
if ($ver -ge [version]'2.1.259') { $flags += @('--permission-prompts', 'none') }   # unknown option before 2.1.259
$fmt = if ($Probe) { @('--output-format', 'stream-json', '--verbose') } else { @('--output-format', 'json') }

function Invoke-Claude($p, $extra) {
  $flags = $script:flags
  if ($extra -contains '--resume') {   # --session-id is for the first call only
    $k = [array]::IndexOf($flags, '--session-id'); $flags = @($flags[0..($k - 1)]) + @($flags[($k + 2)..($flags.Count - 1)])
  }
  Push-Location $cwd
  try { $sw = [Diagnostics.Stopwatch]::StartNew(); $out = $p | & claude @flags @fmt @extra 2>&1; $sw.Stop() } finally { Pop-Location }
  [pscustomobject]@{ out = $out; wall_ms = $sw.ElapsedMilliseconds }
}
function Get-Result($r) {
  $lines = @($r.out | ForEach-Object { "$_" } | Where-Object { $_.StartsWith('{') })
  if ($Probe) { $lines | Where-Object { $_ -match '"subtype":"init"' } | Select-Object -First 1 | Set-Content (Join-Path $root 'init.json') }
  $lines | ForEach-Object { try { $_ | ConvertFrom-Json } catch {} } | Where-Object { $_.type -eq 'result' } | Select-Object -Last 1
}

$r1 = Invoke-Claude $prompt @()
$res = Get-Result $r1
$turns = $res.num_turns; $cost = [double]$res.total_cost_usd; $ms = $r1.wall_ms
$usage = $res.usage
if ($Crops -and -not $res.is_error -and $res.structured_output.shortlist) {
  $byId = @{}; foreach ($p in $photos) { $byId[$p.id] = $p }
  foreach ($id in $res.structured_output.shortlist | Select-Object -First 30) {
    if ($byId.ContainsKey($id)) { Save-Scaled $byId[$id].src (Join-Path $cropDir "$id.jpg") 1024 85 }
  }
  $p2 = "Closer look: 1024 px versions of your 30 shortlisted photos are in $($cropDir -replace '\\','/')/<id>.jpg. Read them and return the final album_picks and your_call_pairs (shortlist may repeat the final picks)."
  $r2 = Invoke-Claude $p2 @('--resume', $res.session_id)
  $res2 = Get-Result $r2
  if ($res2) { $res = $res2; $turns = $res2.num_turns; $cost = [double]$res2.total_cost_usd; $usage = $res2.usage }  # --resume reports the conversation total
  $ms += $r2.wall_ms
}
$res | ConvertTo-Json -Depth 12 | Set-Content (Join-Path $root ("result-{0}-{1}.json" -f $Model, $(if ($Crops) { 'crops' } else { 'nocrops' })))
$row = [ordered]@{ model = $Model; crops = [bool]$Crops; cli = $verStr; photos = $photos.Count; moments = $moments.Count
  num_turns = $turns; duration_ms = $ms; total_cost_usd = $cost; is_error = [bool]$res.is_error; usage = $usage }
($row | ConvertTo-Json -Depth 8 -Compress) | Add-Content (Join-Path $root 'runs.jsonl')
$row | ConvertTo-Json -Depth 8
