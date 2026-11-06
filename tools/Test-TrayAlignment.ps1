[CmdletBinding()]
param([Parameter(Mandatory)][string]$ReportPath)
$ErrorActionPreference = 'Stop'
$report = Get-Content -LiteralPath $ReportPath -Raw | ConvertFrom-Json
if ($report.error) { throw $report.error }
$primary = @($report.taskbars | Where-Object { -not $_.secondary })
if ($primary.Count -ne 1) { throw 'Expected one primary taskbar.' }
function Find-Surface($taskbar, [string]$name) {
    $matches = @($taskbar.geometry | Where-Object { $taskbar.elements[$_.index].name -eq $name })
    if ($matches.Count -eq 1 -and $taskbar.elements[$matches[0].index].visible) { return $matches[0] }
    return $null
}
$failures = @()
if (@($report.taskbars | Where-Object secondary).Count -eq 0) { throw 'No secondary taskbar was inspected.' }
foreach ($secondary in @($report.taskbars | Where-Object secondary)) {
    if ($secondary.geometry_error) { throw $secondary.geometry_error }
    $aframe = Find-Surface $primary[0] 'SystemTrayFrameGrid'
    $bframe = Find-Surface $secondary 'SystemTrayFrameGrid'
    if (-not $aframe -or -not $bframe) { throw 'Tray frame geometry is missing.' }
    foreach ($name in @('NotifyIconStack', 'NotificationAreaIcons', 'MainStack', 'NonActivatableStack', 'ControlCenterButton', 'NotificationCenterButton', 'ShowDesktopStack')) {
        $a = Find-Surface $primary[0] $name
        $b = Find-Surface $secondary $name
        if ($a -and -not $b) { $failures += "Secondary is missing visible surface: $name"; continue }
        if ($a -and $b) {
            $aw = $a.bounds.right - $a.bounds.left
            $bw = $b.bounds.right - $b.bounds.left
            # Compare DIPs so mixed monitor DPI does not cause false failures.
            $ascale = ($a.bounds.bottom - $a.bounds.top) / $primary[0].elements[$a.index].height
            $bscale = ($b.bounds.bottom - $b.bounds.top) / $secondary.elements[$b.index].height
            if ([Math]::Abs($aw / $ascale - $bw / $bscale) -gt 1) {
                $failures += "Secondary surface width differs: $name"
            }
            if ([Math]::Abs(($aframe.bounds.right - $a.bounds.left) / $ascale - ($bframe.bounds.right - $b.bounds.left) / $bscale) -gt 1 -or
                [Math]::Abs(($a.bounds.top - $aframe.bounds.top) / $ascale - ($b.bounds.top - $bframe.bounds.top) / $bscale) -gt 1) {
                $failures += "Secondary surface alignment differs: $name"
            }
        }
    }
    $aicons = @($primary[0].geometry | Where-Object { $primary[0].elements[$_.index].class_name -eq 'SystemTray.NotifyIconView' })
    $bicons = @($secondary.geometry | Where-Object { $secondary.elements[$_.index].class_name -eq 'SystemTray.NotifyIconView' })
    if (($aicons.model_identity -join ',') -ne ($bicons.model_identity -join ',')) {
        $failures += 'Secondary notification icon identities/order differ from the primary.'
    }
}
if ($failures.Count) { throw ($failures -join [Environment]::NewLine) }
'PASS: tray widths, right-edge alignment, vertical alignment and application icon order match the primary.'
