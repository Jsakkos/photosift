<#
.SYNOPSIS
  Registers (or removes) the Task Scheduler task that starts PhotoSift at logon.

.DESCRIPTION
  Task "PhotoSift": runs photosift.exe when the current user logs on, hidden
  (the exe is a GUI-subsystem build, so no console window; output goes to
  <data root>\logs\photosift.log), restarts up to 3 times, 1 minute apart, if
  it fails, and never runs two copies at once.

.PARAMETER ExePath
  The release exe. Default <repo>\dist\photosift\photosift.exe (see build-release.ps1).

.PARAMETER WhatIf
  Print the exact Register-ScheduledTask definition and register nothing.

.PARAMETER Uninstall
  Remove the task instead of creating it (combine with -WhatIf to preview).

.PARAMETER TaskName
  Default PhotoSift.
#>
[CmdletBinding()]
param(
    [string]$ExePath,
    [string]$TaskName = 'PhotoSift',
    [switch]$WhatIf,
    [switch]$Uninstall
)

$ErrorActionPreference = 'Stop'

if ($Uninstall) {
    if ($WhatIf) {
        Write-Host "[WhatIf] Unregister-ScheduledTask -TaskPath '\' -TaskName '$TaskName' -Confirm:`$false"
        return
    }
    if (Get-ScheduledTask -TaskPath '\' -TaskName $TaskName -ErrorAction SilentlyContinue) {
        Unregister-ScheduledTask -TaskPath '\' -TaskName $TaskName -Confirm:$false
        Write-Host "Removed scheduled task '$TaskName'."
    }
    else {
        Write-Host "Scheduled task '$TaskName' does not exist; nothing to remove."
    }
    return
}

if (-not $ExePath) {
    $ExePath = Join-Path (Split-Path -Parent $PSScriptRoot) 'dist\photosift\photosift.exe'
}
$ExePath = [System.IO.Path]::GetFullPath($ExePath)
if (-not (Test-Path -LiteralPath $ExePath)) {
    if ($WhatIf) { Write-Warning "ExePath does not exist yet: $ExePath (run build-release.ps1 first)" }
    else { throw "photosift.exe not found at $ExePath. Run scripts\build-release.ps1 first." }
}
$workDir = Split-Path -Parent $ExePath
$user = "$env:USERDOMAIN\$env:USERNAME"

$action = New-ScheduledTaskAction -Execute $ExePath -WorkingDirectory $workDir
$trigger = New-ScheduledTaskTrigger -AtLogOn -User $user
$principal = New-ScheduledTaskPrincipal -UserId $user -LogonType Interactive -RunLevel Limited
$settings = New-ScheduledTaskSettingsSet -Hidden -RestartCount 3 -RestartInterval (New-TimeSpan -Minutes 1) `
    -ExecutionTimeLimit ([TimeSpan]::Zero) -MultipleInstances IgnoreNew `
    -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -StartWhenAvailable

if ($WhatIf) {
    Write-Host '[WhatIf] Nothing is registered. The task would be created with:'
    Write-Host ''
    Write-Host "`$action    = New-ScheduledTaskAction -Execute '$ExePath' -WorkingDirectory '$workDir'"
    Write-Host "`$trigger   = New-ScheduledTaskTrigger -AtLogOn -User '$user'"
    Write-Host "`$principal = New-ScheduledTaskPrincipal -UserId '$user' -LogonType Interactive -RunLevel Limited"
    Write-Host '$settings  = New-ScheduledTaskSettingsSet -Hidden -RestartCount 3 -RestartInterval (New-TimeSpan -Minutes 1) `'
    Write-Host '    -ExecutionTimeLimit ([TimeSpan]::Zero) -MultipleInstances IgnoreNew `'
    Write-Host '    -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -StartWhenAvailable'
    Write-Host "Register-ScheduledTask -TaskPath '\' -TaskName '$TaskName' -Action `$action -Trigger `$trigger -Principal `$principal -Settings `$settings -Force"
    Write-Host ''
    Write-Host 'Resolved settings:'
    $settings | Format-List Hidden, RestartCount, RestartInterval, ExecutionTimeLimit, MultipleInstances, DisallowStartIfOnBatteries, StopIfGoingOnBatteries, StartWhenAvailable | Out-String | Write-Host
    return
}

Register-ScheduledTask -TaskPath '\' -TaskName $TaskName -Action $action -Trigger $trigger -Principal $principal -Settings $settings -Force | Out-Null
Write-Host "Registered scheduled task '$TaskName' (at logon of $user): $ExePath"
Write-Host "Start it now with: Start-ScheduledTask -TaskPath '\' -TaskName $TaskName"
