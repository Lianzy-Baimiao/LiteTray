[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][string]$PackageDirectory,
    [string]$OutputDirectory,
    [ValidateSet('Enable','Disable')][string]$VGpu = 'Disable',
    [ValidateRange(2048,16384)][int]$MemoryInMB = 4096,
    [switch]$InteractiveAfterChecks
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

# This separate generator explicitly authorizes a guest-only real taskbar
# transaction. New-SandboxCheck.ps1 keeps its isolated default unchanged.
# Generation never runs LiteTray, injects Explorer, or launches Sandbox.
$guestScript = Join-Path $PSScriptRoot 'Invoke-SandboxCompatibilityCheck.ps1'
if (-not (Test-Path -LiteralPath $guestScript -PathType Leaf)) {
    throw 'The live compatibility guest script is missing.'
}
$options = @{
    PackageDirectory=$PackageDirectory
    Mode='Isolated'
    VGpu=$VGpu
    MemoryInMB=$MemoryInMB
}
if ($OutputDirectory) { $options.OutputDirectory=$OutputDirectory }
$generated = & (Join-Path $PSScriptRoot 'New-SandboxCheck.ps1') @options | ConvertFrom-Json
$run = [string]$generated.run_directory
$inputDirectory = Join-Path $run 'input'
$settingsPath = Join-Path $inputDirectory 'run.json'
$settings = Get-Content -LiteralPath $settingsPath -Raw -Encoding UTF8 | ConvertFrom-Json
$settings | Add-Member -NotePropertyName live_taskbar_test_authorized -NotePropertyValue $true
$settings | Add-Member -NotePropertyName live_scope -NotePropertyValue 'primary_clock_and_tray_preparation'
$settings | Add-Member -NotePropertyName live_tests -NotePropertyValue @('--self-test-shell-source','--test-taskbar-compatibility')
$settings | Add-Member -NotePropertyName interactive_after_checks -NotePropertyValue ([bool]$InteractiveAfterChecks)
$settings.explorer_takeover_automated=$true
$utf8 = [Text.UTF8Encoding]::new($false)
[IO.File]::WriteAllText($settingsPath,($settings | ConvertTo-Json -Depth 8),$utf8)
Copy-Item -LiteralPath $guestScript -Destination (Join-Path $inputDirectory 'Invoke-SandboxCompatibilityCheck.ps1')

$configurationPath = [string]$generated.configuration
$configuration = [IO.File]::ReadAllText($configurationPath)
$oldCommand = '-File C:\LiteTrayInput\Invoke-SandboxGuestCheck.ps1 -EnvironmentType WindowsSandbox'
$newCommand = '-File C:\LiteTrayInput\Invoke-SandboxCompatibilityCheck.ps1 -EnvironmentType WindowsSandbox'
if (-not $configuration.Contains($oldCommand)) { throw 'The isolated generator logon command has changed; no live configuration was installed.' }
[IO.File]::WriteAllText($configurationPath,$configuration.Replace($oldCommand,$newCommand),$utf8)
$readme = @"
LiteTray explicit live taskbar compatibility run

This directory was generated only. Windows Sandbox has NOT been launched.
Review LiteTray-Check.wsb and input\run.json, then open the .wsb to run it.
The live_taskbar_test_authorized flag permits a temporary guest Explorer
primary clock takeover and restoration. This harness is for disposable guests.

Required order, stopping before live checks on any failed prerequisite:
1. Copy and SHA-256 verify the package in the guest, with a fresh disabled profile.
2. Complete disk diagnostics, app-icon and isolated XAML clock tests.
3. Require a complete built-in profile disk match and actual zero exit codes.
4. Complete --self-test-shell-source in the app's private host, without Explorer.
5. Run --test-taskbar-compatibility, recording the primary clock before, during,
   and after text/width updates, Stop acknowledgment, and unchanged Explorer PID.
   Prepare and stop the tray interfaces without creating a secondary replica.

vGPU: $VGpu; RAM: $MemoryInMB MiB
Networking, clipboard, camera, microphone and printer redirection: disabled
Read-only host mapping: input (verified snapshot and both guest scripts)
Only writable host mapping: results (this run's dedicated output directory)
No host profile, credentials, or Windows directory is mapped.

Open settings after all automatic checks pass: $([bool]$InteractiveAfterChecks)
Settings starts with takeover disabled; interactive calendar, scroll, hover,
tray menus and multiple monitors require separate observed acceptance records.
The automatic check restores its clock session before reporting success.
A timeout is failure and recovery remains unknown until independently checked.

Each invocation creates a live summary and a separate isolated prerequisite
summary. Follow baseline_results in the live summary for the prerequisite logs.
Both the actual child process exit code and its JSON report must pass.
Only primary clock takeover and tray preparation are covered by this harness.
Font/color/tooltip binding restoration is not independently inspected here.
Single-monitor Sandbox cannot validate tray model replication or multiple DPI.
The actual Windows build/profile is recorded; Sandbox cannot select an old OS.
"@
[IO.File]::WriteAllText((Join-Path $run 'README.txt'),$readme,$utf8)
[pscustomobject]@{
    run_directory=$run
    configuration=$configurationPath
    results_directory=$generated.results_directory
    mode='PrimaryTaskbarLive'
    live_taskbar_test_authorized=$true
    interactive_after_checks=[bool]$InteractiveAfterChecks
    verified_files=$generated.verified_files
    launched=$false
} | ConvertTo-Json
