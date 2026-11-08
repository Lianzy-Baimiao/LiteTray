[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][int]$ProcessId,
    [string]$LogPath = (Join-Path $env:LOCALAPPDATA 'LiteTray\taskbar-events.log')
)
$ErrorActionPreference = 'Stop'
$hostProcess = Get-Process -Id $ProcessId
if ([IO.Path]::GetFileName($hostProcess.Path) -ne 'LiteTray.exe') {
    throw 'The target must be the running LiteTray host.'
}
if (-not (Test-Path -LiteralPath $LogPath)) { throw 'The running host event log is required.' }

Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class LiteTrayDeviceCheck {
    public delegate bool EnumProc(IntPtr window, IntPtr data);
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc callback, IntPtr data);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetClassNameW(IntPtr window, StringBuilder name, int capacity);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr window, out uint process);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindowW(string cls, string title);
    [DllImport("user32.dll", SetLastError=true)] public static extern IntPtr SendMessageTimeoutW(IntPtr window, uint message, UIntPtr w, IntPtr l, uint flags, uint timeout, out UIntPtr result);
    public static IntPtr FindHost(uint pid) {
        IntPtr result = IntPtr.Zero;
        EnumWindows((window, data) => {
            uint owner; GetWindowThreadProcessId(window, out owner);
            if (owner != pid) return true;
            var name = new StringBuilder(128);
            GetClassNameW(window, name, name.Capacity);
            if (name.ToString() != "LiteTray.Settings.1") return true;
            result = window; return false;
        }, IntPtr.Zero);
        return result;
    }
}
'@
$window = [LiteTrayDeviceCheck]::FindHost($ProcessId)
if ($window -eq [IntPtr]::Zero) { throw 'No LiteTray host window belongs to the requested process.' }
$taskbar = [LiteTrayDeviceCheck]::FindWindowW('Shell_TrayWnd', $null)
[uint32]$shellProcessId = 0
[void][LiteTrayDeviceCheck]::GetWindowThreadProcessId($taskbar, [ref]$shellProcessId)
if ($shellProcessId -eq 0) { throw 'No desktop taskbar was found.' }
$shellProcess = Get-Process -Id $shellProcessId
$hostStarted = $hostProcess.StartTime
$shellStarted = $shellProcess.StartTime
$before = [IO.File]::ReadAllText($LogPath)
if ($before -notmatch "pid=$ProcessId connected: taskbar session active") {
    throw 'The target host has not recorded an active taskbar session.'
}

# Send only to this host, never broadcast to Explorer or other applications.
# DBT_DEVNODES_CHANGED has no cross-process device payload to marshal.
foreach ($event in @(7) * 16) {
    [UIntPtr]$messageResult = [UIntPtr]::Zero
    if ([LiteTrayDeviceCheck]::SendMessageTimeoutW($window, 0x0219, [UIntPtr]$event, [IntPtr]::Zero, 2, 1000, [ref]$messageResult) -eq [IntPtr]::Zero) {
        throw "Sending the host device notification failed: $([Runtime.InteropServices.Marshal]::GetLastWin32Error())"
    }
}
$deadline = [DateTime]::UtcNow.AddSeconds(8)
$observed = ''
do {
    Start-Sleep -Milliseconds 200
    $current = [IO.File]::ReadAllText($LogPath)
    if (-not $current.StartsWith($before)) { throw 'The event log rotated; rerun the check.' }
    $observed = $current.Substring($before.Length)
    if ($observed -match "pid=$ProcessId (restart|restored|session fault|restore failed):") {
        throw "A device notification interrupted the active taskbar session:`n$observed"
    }
    if ($hostProcess.HasExited -or $shellProcess.HasExited) { throw 'The host or Explorer exited during the check.' }
} while ([DateTime]::UtcNow -lt $deadline)
if ($observed -notmatch "pid=$ProcessId devices unchanged: taskbar session retained") {
    throw "The host did not confirm unchanged topology within 8 seconds:`n$observed"
}
if ((Get-Process -Id $ProcessId).StartTime -ne $hostStarted -or
    (Get-Process -Id $shellProcessId).StartTime -ne $shellStarted) {
    throw 'The host or Explorer process identity changed.'
}
[pscustomobject]@{
    passed = $true
    notifications = 16
    host_pid = $ProcessId
    explorer_pid = $shellProcessId
    native_restore_or_reconnect = $false
    events = @($observed.Trim() -split '\r?\n')
} | ConvertTo-Json -Depth 4
