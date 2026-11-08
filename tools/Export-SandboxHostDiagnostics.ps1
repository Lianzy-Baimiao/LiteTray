[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][string]$OutputDirectory,
    [DateTimeOffset]$StartTime = [DateTimeOffset]::Now.AddMinutes(-30),
    [DateTimeOffset]$EndTime = [DateTimeOffset]::Now
)
$ErrorActionPreference = 'Stop'
$principal = [Security.Principal.WindowsPrincipal]::new([Security.Principal.WindowsIdentity]::GetCurrent())
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'Run this read-only log export from an administrator PowerShell. It does not enable features, repair Windows or restart anything.'
}
if ($EndTime -le $StartTime) { throw 'EndTime must be after StartTime.' }
$output = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($OutputDirectory)
if (Test-Path -LiteralPath $output) { throw 'OutputDirectory must be a new directory; existing results are never overwritten.' }
[IO.Directory]::CreateDirectory($output) | Out-Null
$channels = @(
    'Microsoft-Windows-Hyper-V-Compute-Admin',
    'Microsoft-Windows-Hyper-V-Compute-Operational',
    'Microsoft-Windows-Hyper-V-Worker-Admin',
    'Microsoft-Windows-Hyper-V-Worker-Operational',
    'Microsoft-Windows-Hyper-V-VID-Admin',
    'Microsoft-Windows-Hyper-V-Hypervisor-Admin',
    'Microsoft-Windows-Host-Network-Service-Admin'
)
$records = foreach ($channel in $channels) {
    try {
        # Verify access separately: an access error is not an empty event log.
        $metadata = Get-WinEvent -ListLog $channel -ErrorAction Stop
        $events = @()
        try {
            $events = @(Get-WinEvent -FilterHashtable @{
                LogName=$channel
                StartTime=$StartTime.LocalDateTime
                EndTime=$EndTime.LocalDateTime
            } -MaxEvents 300 -ErrorAction Stop | Select-Object TimeCreated,Id,ProviderName,LevelDisplayName,Message)
        } catch {
            if ($_.FullyQualifiedErrorId -notlike 'NoMatchingEventsFound*') { throw }
        }
        [pscustomobject]@{channel=$channel;enabled=$metadata.IsEnabled;status='read';events=$events;error=$null}
    } catch {
        [pscustomobject]@{channel=$channel;enabled=$null;status='unavailable';events=@();error=$_.Exception.Message}
    }
}
$report = [ordered]@{
    collected_at=[DateTimeOffset]::Now.ToString('o')
    start_time=$StartTime.ToString('o')
    end_time=$EndTime.ToString('o')
    maximum_events_per_channel=300
    changes_to_system=$false
    channels=@($records)
}
$path = Join-Path $output 'sandbox-host-events.json'
[IO.File]::WriteAllText($path,($report | ConvertTo-Json -Depth 8),[Text.UTF8Encoding]::new($false))
[pscustomobject]@{report=$path;read_channels=@($records | Where-Object status -eq 'read').Count;unavailable_channels=@($records | Where-Object status -eq 'unavailable').Count}
