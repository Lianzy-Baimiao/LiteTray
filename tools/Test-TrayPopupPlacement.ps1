[CmdletBinding()]
param([Parameter(Mandatory=$true)][string]$ReportPath, [string]$ProcessName = 'QQ.exe')
$ErrorActionPreference = 'Stop'

# The recorder never moves the pointer or creates a popup. Only an actual
# visible popup while the pointer is on a secondary taskbar counts as evidence.
$observed = @(Get-Content -LiteralPath $ReportPath | ForEach-Object {
    if ([string]::IsNullOrWhiteSpace($_)) { return }
    $row = $_ | ConvertFrom-Json
    if ($row.hover_taskbar.class -eq 'Shell_SecondaryTrayWnd' -and
        $row.window.process -eq $ProcessName -and
        $row.window.popup -eq $true -and $row.window.visible -eq $true -and
        ($row.window.rect[2] - $row.window.rect[0]) -ge 24 -and
        ($row.window.rect[3] - $row.window.rect[1]) -ge 24 -and
        $row.verdict -in @('same_monitor', 'wrong_monitor')) { $row }
})
if ($observed.Count -eq 0) {
    Write-Output "INCONCLUSIVE: no visible $ProcessName popup recorded during secondary taskbar hover."
    exit 2
}
$wrong = @($observed | Where-Object verdict -eq 'wrong_monitor')
if ($wrong.Count -gt 0) {
    $wrong | ForEach-Object {
        Write-Output ('FAIL: {0} {1}, popup={2}, taskbar={3}, rect=[{4}]' -f
            $_.window.process, $_.window.class, $_.window.monitor,
            $_.hover_taskbar.monitor, ($_.window.rect -join ','))
    }
    exit 1
}
Write-Output ('PASS: {0} visible popup observations stayed on the hovered secondary monitor.' -f $observed.Count)
