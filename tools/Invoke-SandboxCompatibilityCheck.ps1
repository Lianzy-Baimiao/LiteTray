[CmdletBinding()]
param(
    [string]$InputDirectory = 'C:\LiteTrayInput',
    [string]$ResultsDirectory = 'C:\LiteTrayResults',
    [ValidateSet('WindowsSandbox','VirtualMachine')][string]$EnvironmentType = 'WindowsSandbox'
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

# VM mode must be explicitly selected inside a disposable VM. This operator
# guard is deliberately also checked before any report directories are created.
if ($EnvironmentType -eq 'WindowsSandbox' -and $env:USERNAME -ne 'WDAGUtilityAccount') {
    throw 'Run this live harness inside Windows Sandbox, or explicitly choose -EnvironmentType VirtualMachine inside a disposable VM.'
}
if (-not [Environment]::Is64BitProcess) { throw 'The x64 package requires 64-bit PowerShell.' }
$inputRoot = (Resolve-Path -LiteralPath $InputDirectory).Path
$outputRoot = (Resolve-Path -LiteralPath $ResultsDirectory).Path
$settings = Get-Content -LiteralPath (Join-Path $inputRoot 'run.json') -Raw -Encoding UTF8 | ConvertFrom-Json
if ($settings.schema -ne 1 -or $settings.mode -ne 'Isolated' -or
    $settings.run_id -notmatch '^sandbox-[0-9]{8}-[0-9]{6}-[0-9a-f]{8}$' -or
    $settings.live_taskbar_test_authorized -ne $true -or
    $settings.live_scope -ne 'primary_clock_and_tray_preparation') {
    throw 'A separately generated, explicitly authorized live run configuration is required.'
}
$instance = $settings.run_id + '-live-' + [Guid]::NewGuid().ToString('N').Substring(0,8)
$output = Join-Path $outputRoot $instance
[IO.Directory]::CreateDirectory($output) | Out-Null
$utf8 = [Text.UTF8Encoding]::new($false)
$profile = $null
$app = $null
$binary = $null
$previousProfile = $env:LITETRAY_DATA_DIR
$summary = [ordered]@{
    schema=1
    run_id=$settings.run_id
    guest_instance=$instance
    started_at=(Get-Date).ToUniversalTime().ToString('o')
    completed_at=$null
    environment=$EnvironmentType
    mode='PrimaryTaskbarLive'
    status='running'
    baseline_results=$null
    os=$null
    profile_id=$null
    package_manifest_sha256=$settings.package_manifest_sha256
    prerequisite_checks_passed=$false
    explorer_takeover_automated=$true
    explorer_live_check_started=$false
    live_taskbar_validation='not_run'
    clock_restoration='not_run'
    tray_preparation='not_run'
    tray_model_replication='not_run'
    calendar_scroll_hover_and_tray_menus='not_run'
    multi_monitor_validation='not_run'
    interactive_settings_opened=$false
    tests=@()
    collected_logs=@()
    error=$null
}
function Write-Summary {
    [IO.File]::WriteAllText((Join-Path $output 'summary.json'),($summary | ConvertTo-Json -Depth 20),$utf8)
}
function Read-Report([string]$Path) {
    Get-Content -LiteralPath $Path -Raw -Encoding UTF8 | ConvertFrom-Json
}
function ConvertTo-ProcessArgument([string]$Argument) {
    # CommandLineToArgvW quoting, including literal quotes and trailing slashes.
    $escaped = [regex]::Replace($Argument, '(\\*)"', '$1$1\"')
    $escaped = [regex]::Replace($escaped, '(\\+)$', '$1$1')
    '"' + $escaped + '"'
}
function Invoke-RecordedCheck([string]$Name,[string[]]$Arguments,[string]$ReportName,[int]$TimeoutMilliseconds,[scriptblock]$Validate) {
    $stdout = Join-Path $output ($Name+'.stdout.json')
    $stderr = Join-Path $output ($Name+'.stderr.txt')
    $record = [ordered]@{
        name=$Name
        exit_code=$null
        timed_out=$false
        passed=$false
        process_id=$null
        duration_ms=0
        stdout=[IO.Path]::GetFileName($stdout)
        stderr=[IO.Path]::GetFileName($stderr)
        report=$ReportName
        error=$null
    }
    $summary.tests+=,$record
    Write-Summary
    $clock = [Diagnostics.Stopwatch]::StartNew()
    $process = New-Object System.Diagnostics.Process
    $stdoutTask = $null
    $stderrTask = $null
    try {
        $process.StartInfo.FileName=$binary
        $process.StartInfo.WorkingDirectory=$app
        $process.StartInfo.Arguments=($Arguments | ForEach-Object { ConvertTo-ProcessArgument $_ }) -join ' '
        $process.StartInfo.UseShellExecute=$false
        $process.StartInfo.CreateNoWindow=$true
        $process.StartInfo.RedirectStandardOutput=$true
        $process.StartInfo.RedirectStandardError=$true
        $process.StartInfo.StandardOutputEncoding=$utf8
        $process.StartInfo.StandardErrorEncoding=$utf8
        # Process.Start retains the process handle at creation on PS 5.1 too;
        # a later Start-Process handle lookup can lose a fast child's ExitCode.
        if (-not $process.Start()) { throw 'The test process did not start.' }
        $record.process_id=$process.Id
        $stdoutTask=$process.StandardOutput.ReadToEndAsync()
        $stderrTask=$process.StandardError.ReadToEndAsync()
        if (-not $process.WaitForExit($TimeoutMilliseconds)) {
            $record.timed_out=$true
            # Never kill or restart Explorer to manufacture a restored result.
            $process.Kill()
            if (-not $process.WaitForExit(10000)) { throw 'The timed-out LiteTray child could not be reaped; restoration is unknown.' }
        }
        $process.WaitForExit()
        $record.exit_code=$process.ExitCode
        [IO.File]::WriteAllText($stdout,$stdoutTask.GetAwaiter().GetResult(),$utf8)
        [IO.File]::WriteAllText($stderr,$stderrTask.GetAwaiter().GetResult(),$utf8)
        if ($record.timed_out) { throw 'The test exceeded its deadline; recovery is not assumed.' }
        if ($null -eq $record.exit_code -or $record.exit_code -ne 0) {
            throw ('The test did not return an actual zero exit code: '+$record.exit_code)
        }
        $result = Read-Report (Join-Path $output $ReportName)
        if ($result.passed -ne $true) { throw 'The test report did not declare success.' }
        & $Validate $result
        $record.passed=$true
        return $result
    } catch {
        $record.error=$_.Exception.Message
        throw
    } finally {
        # Preserve completed streams even if starting, waiting or validation
        # failed. Do not wait indefinitely for streams of an unreaped process.
        if ($null -ne $stdoutTask -and $stdoutTask.IsCompleted -and -not (Test-Path -LiteralPath $stdout)) {
            [IO.File]::WriteAllText($stdout,$stdoutTask.GetAwaiter().GetResult(),$utf8)
        }
        if ($null -ne $stderrTask -and $stderrTask.IsCompleted -and -not (Test-Path -LiteralPath $stderr)) {
            [IO.File]::WriteAllText($stderr,$stderrTask.GetAwaiter().GetResult(),$utf8)
        }
        $process.Dispose()
        $clock.Stop()
        $record.duration_ms=$clock.ElapsedMilliseconds
        Write-Summary
    }
}
Write-Summary
try {
    # Reuse the unchanged isolated checks for package verification and a new
    # guest profile. Clearing this variable prevents reusing a stale run if the
    # baseline harness fails before establishing its own work directory.
    Remove-Item -LiteralPath 'Env:LITETRAY_DATA_DIR' -ErrorAction SilentlyContinue
    & (Join-Path $inputRoot 'Invoke-SandboxGuestCheck.ps1') -InputDirectory $inputRoot -ResultsDirectory $outputRoot -EnvironmentType $EnvironmentType
    if (-not $env:LITETRAY_DATA_DIR) { throw 'The isolated harness did not establish a guest profile.' }
    $profile = $env:LITETRAY_DATA_DIR
    $work = Split-Path $profile -Parent
    $baselineInstance = Split-Path $work -Leaf
    if ($baselineInstance -notmatch ('^'+[regex]::Escape($settings.run_id)+'-guest-[0-9a-f]{8}$')) {
        throw 'The isolated result does not belong to this generated run.'
    }
    $baselineOutput = Join-Path $outputRoot $baselineInstance
    $summary.baseline_results=$baselineInstance
    $baseline = Read-Report (Join-Path $baselineOutput 'summary.json')
    $summary.os=$baseline.os
    if ($baseline.run_id -ne $settings.run_id -or $baseline.status -ne 'checks_passed' -or
        $baseline.explorer_takeover_automated -ne $false -or $baseline.interactive_settings_opened -ne $false) {
        throw 'The isolated prerequisite harness did not complete within the required boundary.'
    }
    foreach ($name in @('compatibility','diagnostics','app-icon','xaml-clock')) {
        $checks = @($baseline.tests | Where-Object { $_.name -eq $name })
        if ($checks.Count -ne 1 -or $checks[0].passed -ne $true -or $checks[0].timed_out -ne $false -or
            $null -eq $checks[0].exit_code -or $checks[0].exit_code -ne 0) {
            throw ('Prerequisite lacks both a passed result and an actual zero exit code: '+$name)
        }
    }
    $disk = Read-Report (Join-Path $baselineOutput 'compatibility.json')
    if ($disk.scope -ne 'disk_only' -or $disk.explorer_accessed -ne $false -or
        $disk.network_accessed -ne $false -or $disk.all_profile_images_matched -ne $true -or
        [string]::IsNullOrWhiteSpace([string]$disk.profile_id)) {
        throw 'All profile images must match on disk before any Explorer check is permitted.'
    }
    $summary.profile_id=$disk.profile_id
    $xaml = Read-Report (Join-Path $baselineOutput 'xaml-clock.stdout.json')
    if ($xaml.passed -ne $true -or $xaml.explorer_accessed -ne $false -or
        $xaml.environment -ne 'isolated_xaml_in_host_process') {
        throw 'The XAML clock prerequisite did not confirm isolated success.'
    }
    $app = Join-Path $work 'app'
    $binary = Join-Path $app 'LiteTray.exe'
    $null = Invoke-RecordedCheck 'shell-source' @('--self-test-shell-source') 'shell-source.stdout.json' 90000 {
        param($result)
        if ($result.explorer_accessed -ne $false -or
            $result.environment -ne 'isolated_private_shell_host_in_app_process' -or
            $result.profile_id -ne $summary.profile_id) {
            throw 'Private Shell host self-test did not confirm the selected profile and isolated boundary.'
        }
    }
    $summary.prerequisite_checks_passed=$true
    $summary.explorer_live_check_started=$true
    $summary.live_taskbar_validation='running'
    $summary.clock_restoration='pending'
    Write-Summary
    $live = Invoke-RecordedCheck 'taskbar-live' @('--test-taskbar-compatibility',(Join-Path $output 'taskbar-live.json')) 'taskbar-live.json' 120000 {
        param($result)
        if ($result.scope -ne 'primary_taskbar_live' -or $result.explorer_accessed -ne $true -or
            $result.writes_requested -ne $true -or $result.input_injected -ne $false -or
            $result.explorer_identity_preserved -ne $true -or $result.cleanup_acknowledged -ne $true -or
            $result.clock.passed -ne $true -or $result.clock.stop_acknowledged -ne $true -or
            $result.tray.preparation_passed -ne $true -or $result.tray.stop_acknowledged -ne $true -or
            $result.tray.activated -ne $false) {
            throw 'Live report lacks required primary clock restoration, Explorer identity, or tray preparation evidence.'
        }
    }
    $summary.live_taskbar_validation='primary_clock_passed'
    $summary.clock_restoration='observed_original_text_layout_and_visibility_with_stop_acknowledgment'
    $summary.tray_preparation='passed'
    $summary.tray_model_replication=$live.tray.replication
    $summary.status='checks_passed'
    if ($settings.interactive_after_checks -eq $true) {
        # Explicit opt-in opens visible settings for the operator. The isolated
        # profile still has clock takeover, tray mirroring and QQ hooks disabled.
        Start-Process -FilePath $binary -WorkingDirectory $app -ArgumentList '--settings' -WindowStyle Normal | Out-Null
        $summary.interactive_settings_opened=$true
    }
} catch {
    $summary.status='checks_failed'
    $summary.error=$_.Exception.Message
    if ($summary.live_taskbar_validation -eq 'running') {
        $summary.live_taskbar_validation='failed'
        $summary.clock_restoration='not_confirmed; inspect taskbar-live.json and the guest before proceeding'
    }
} finally {
    foreach ($log in @(
        @{source=(Join-Path $env:LOCALAPPDATA 'LiteTray\adapter-events.log');name='adapter-events.log'},
        @{source=(Join-Path $env:LOCALAPPDATA 'LiteTray\adapter-events.old.log');name='adapter-events.old.log'},
        @{source=$(if ($profile) { Join-Path $profile 'taskbar-events.log' } else { $null });name='taskbar-events.log'}
    )) {
        if ($log.source -and (Test-Path -LiteralPath $log.source -PathType Leaf)) {
            try {
                Copy-Item -LiteralPath $log.source -Destination (Join-Path $output $log.name)
                $summary.collected_logs+=,$log.name
            } catch { $summary.collected_logs+=,($log.name+': '+$_.Exception.Message) }
        }
    }
    $summary.completed_at=(Get-Date).ToUniversalTime().ToString('o')
    Write-Summary
    $text = @(
        '# LiteTray primary taskbar live results','',
        ('Status: '+$summary.status),
        ('Selected profile: '+$summary.profile_id),
        ('Isolated prerequisite results: '+$summary.baseline_results),'',
        ('Live clock: '+$summary.live_taskbar_validation),
        ('Restoration: '+$summary.clock_restoration),
        ('Tray interfaces: '+$summary.tray_preparation),
        ('Tray model replication: '+$summary.tray_model_replication),'',
        '| Check | Passed | Actual exit | Timeout |',
        '| --- | --- | --- | --- |'
    )
    foreach ($check in $summary.tests) { $text+=('| '+$check.name+' | '+$check.passed+' | '+$check.exit_code+' | '+$check.timed_out+' |') }
    $text+=@('','Calendar click, scroll, hover, tray menus and multiple monitors are not validated automatically.','Font/color/tooltip bindings are not independently exported by the live inspection.','A child timeout is a failure; automatic recovery is never assumed from a killed process.')
    if ($summary.error) { $text+=@('',('Error: '+$summary.error)) }
    [IO.File]::WriteAllText((Join-Path $output 'summary.md'),($text -join "`r`n"),$utf8)
    [IO.File]::WriteAllText((Join-Path ([Environment]::GetFolderPath('Desktop')) 'LiteTray-Test-Results.txt'),('Status: '+$summary.status+"`r`nLive results: "+$output+"`r`nBaseline results: "+$summary.baseline_results+"`r`n"),$utf8)
    if ($null -eq $previousProfile) {
        Remove-Item -LiteralPath 'Env:LITETRAY_DATA_DIR' -ErrorAction SilentlyContinue
    } else {
        $env:LITETRAY_DATA_DIR=$previousProfile
    }
}
