Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing

$outPath = $args[0]
if (-not $outPath) { $outPath = "screenshot.png" }

$screen = [System.Windows.Forms.Screen]::PrimaryScreen
$bitmap = New-Object System.Drawing.Bitmap($screen.Bounds.Width, $screen.Bounds.Height)
$graphics = [System.Drawing.Graphics]::FromImage($bitmap)
$graphics.CopyFromScreen($screen.Bounds.Location, [System.Drawing.Point]::Empty, $screen.Bounds.Size)
$bitmap.Save($outPath)
$graphics.Dispose()
$bitmap.Dispose()
Write-Host "Screenshot saved to $outPath"
