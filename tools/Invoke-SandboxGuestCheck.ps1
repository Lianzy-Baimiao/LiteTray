[CmdletBinding()]
param(
    [string]$InputDirectory = 'C:\LiteTrayInput',
    [string]$ResultsDirectory = 'C:\LiteTrayResults',
    [ValidateSet('WindowsSandbox','VirtualMachine')][string]$EnvironmentType = 'WindowsSandbox'
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

# A guard against accidentally running the guest harness on the host. This is
# an operator check, not a security proof. VM use requires the explicit flag.
if ($EnvironmentType -eq 'WindowsSandbox' -and $env:USERNAME -ne 'WDAGUtilityAccount') {
    throw 'Run this script inside Windows Sandbox, or explicitly choose -EnvironmentType VirtualMachine inside a disposable VM.'
}
if (-not [Environment]::Is64BitProcess) { throw 'The x64 package requires 64-bit PowerShell.' }
$inputRoot = (Resolve-Path -LiteralPath $InputDirectory).Path
$outputRoot = (Resolve-Path -LiteralPath $ResultsDirectory).Path
$settings = Get-Content -LiteralPath (Join-Path $inputRoot 'run.json') -Raw -Encoding UTF8 | ConvertFrom-Json
if ($settings.schema -ne 1 -or $settings.mode -notin @('Isolated','Interactive') -or
    $settings.run_id -notmatch '^sandbox-[0-9]{8}-[0-9]{6}-[0-9a-f]{8}$') {
    throw 'Unsupported or invalid generated run configuration.'
}
$instance = $settings.run_id + '-guest-' + [Guid]::NewGuid().ToString('N').Substring(0,8)
$output = Join-Path $outputRoot $instance
$work = Join-Path $env:LOCALAPPDATA ('LiteTraySandboxChecks\' + $instance)
$app = Join-Path $work 'app'
$profile = Join-Path $work 'profile'
[IO.Directory]::CreateDirectory($output) | Out-Null
[IO.Directory]::CreateDirectory($app) | Out-Null
[IO.Directory]::CreateDirectory($profile) | Out-Null
$utf8 = [Text.UTF8Encoding]::new($false)
function Write-Json([string]$Name,$Value) {
    [IO.File]::WriteAllText((Join-Path $output $Name),($Value | ConvertTo-Json -Depth 16),$utf8)
}
$summary = [ordered]@{
    schema=1
    run_id=$settings.run_id
    guest_instance=$instance
    started_at=(Get-Date).ToUniversalTime().ToString('o')
    completed_at=$null
    environment=$EnvironmentType
    mode=$settings.mode
    vgpu=$settings.vgpu
    status='running'
    os=$null
    package_manifest_sha256=$settings.package_manifest_sha256
    tests=@()
    feature_status=@()
    explorer_takeover_automated=$false
    live_taskbar_validation='not_run'
    multi_monitor_validation='not_run'
    interactive_settings_opened=$false
    error=$null
}
Write-Json 'summary.json' $summary
try {
    $version = Get-ItemProperty -LiteralPath 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion'
    $os = Get-CimInstance -ClassName Win32_OperatingSystem
    $system = Get-CimInstance -ClassName Win32_ComputerSystem
    $guestOs = [ordered]@{
        caption=$os.Caption
        version=$os.Version
        build=$os.BuildNumber
        ubr=$version.UBR
        build_lab=$version.BuildLabEx
        display_version=$version.DisplayVersion
        edition_id=$version.EditionID
        architecture=$os.OSArchitecture
        system_manufacturer=$system.Manufacturer
        system_model=$system.Model
        powershell=$PSVersionTable.PSVersion.ToString()
        environment=$EnvironmentType
    }
    $summary.os=$guestOs
    Write-Json 'os.json' $guestOs

    $source = Join-Path $inputRoot 'package'
    $manifest = Join-Path $source 'SHA256SUMS.txt'
    if ((Get-FileHash -LiteralPath $manifest -Algorithm SHA256).Hash.ToLowerInvariant() -ne $settings.package_manifest_sha256) {
        throw 'Generated package manifest changed before the guest run.'
    }
    $copied = [Collections.Generic.List[object]]::new()
    foreach ($file in $settings.package_files) {
        $relative = [string]$file.relative_path
        if ([IO.Path]::IsPathRooted($relative) -or $relative -match '[\x00-\x1f:*?"<>|]' -or
            ($relative.Split('\') | Where-Object { -not $_ -or $_ -in @('.','..') -or $_.EndsWith('.') -or $_.EndsWith(' ') })) {
            throw "Unsafe package file in run configuration: $relative"
        }
        $from = [IO.Path]::GetFullPath((Join-Path $source $relative))
        $destination = [IO.Path]::GetFullPath((Join-Path $app $relative))
        if (-not $from.StartsWith($source.TrimEnd('\')+'\',[StringComparison]::OrdinalIgnoreCase) -or
            -not $destination.StartsWith($app.TrimEnd('\')+'\',[StringComparison]::OrdinalIgnoreCase)) {
            throw 'Package file escaped its directory.'
        }
        [IO.Directory]::CreateDirectory((Split-Path $destination -Parent)) | Out-Null
        Copy-Item -LiteralPath $from -Destination $destination
        $hash = (Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($hash -ne $file.sha256) { throw "Guest package hash mismatch: $relative" }
        $copied.Add([pscustomobject]@{relative_path=$relative;sha256=$hash})
    }
    Write-Json 'package-verified.json' @($copied.ToArray())
    # Never copy the host's user profile. All guest UI features start disabled.
    [IO.File]::WriteAllText((Join-Path $profile 'config.json'),@'
{"schema_version":1,"tray":{"enabled":false,"clock_enabled":false,"qq_popup_compatibility":false},"weather":{"enabled":false},"updates":{"check_on_startup":false},"start_with_windows":false}
'@,$utf8)
    $env:LITETRAY_DATA_DIR=$profile
    $binary = Join-Path $app 'LiteTray.exe'
    $checks = @(
        @{name='compatibility';arguments=@('--compatibility-report',('"'+(Join-Path $output 'compatibility.json')+'"'));json='compatibility.json'},
        @{name='diagnostics';arguments=@('--diagnose',('"'+(Join-Path $output 'diagnostics.json')+'"'));json='diagnostics.json'},
        @{name='app-icon';arguments=@('--self-test-app-icon');json='app-icon.stdout.json'},
        @{name='xaml-clock';arguments=@('--self-test-clock');json='xaml-clock.stdout.json'}
    )
    foreach ($check in $checks) {
        $stdout = Join-Path $output ($check.name+'.stdout.json')
        $stderr = Join-Path $output ($check.name+'.stderr.txt')
        $clock = [Diagnostics.Stopwatch]::StartNew()
        $record = [ordered]@{name=$check.name;exit_code=$null;timed_out=$false;passed=$false;duration_ms=0;stdout=[IO.Path]::GetFileName($stdout);stderr=[IO.Path]::GetFileName($stderr);error=$null}
        try {
            $process = Start-Process -FilePath $binary -WorkingDirectory $app -ArgumentList $check.arguments -WindowStyle Hidden -PassThru -RedirectStandardOutput $stdout -RedirectStandardError $stderr
            # Windows PowerShell 5.1 needs the handle retained before waiting;
            # otherwise ExitCode can remain null after the process exits.
            $null = $process.Handle
            if (-not $process.WaitForExit(60000)) {
                $record.timed_out=$true
                $process.Kill()
                $process.WaitForExit()
            } else {
                $process.WaitForExit()
            }
            $record.exit_code=$process.ExitCode
            if ($null -eq $record.exit_code) { throw 'The process exited without an available exit code.' }
            if (-not $record.timed_out -and $record.exit_code -eq 0) {
                $value = Get-Content -LiteralPath (Join-Path $output $check.json) -Raw -Encoding UTF8 | ConvertFrom-Json
                if ($check.name -eq 'compatibility') {
                    if ($value.scope -ne 'disk_only' -or $value.explorer_accessed -ne $false -or $value.network_accessed -ne $false) {
                        throw 'Compatibility report did not confirm its disk-only boundary.'
                    }
                    $summary.feature_status=@($value.features | Select-Object feature,status)
                } elseif ($check.name -eq 'xaml-clock') {
                    if ($value.passed -ne $true -or $value.explorer_accessed -ne $false -or $value.environment -ne 'isolated_xaml_in_host_process') {
                        throw 'XAML self-test did not confirm isolated success.'
                    }
                } elseif ($check.name -eq 'app-icon' -and $value.passed -ne $true) {
                    throw 'App-icon self-test did not confirm success.'
                }
                $record.passed=$true
            }
        } catch {
            $record.error=$_.Exception.Message
        }
        $clock.Stop()
        $record.duration_ms=$clock.ElapsedMilliseconds
        $summary.tests+=,[pscustomobject]$record
        Write-Json 'summary.json' $summary
    }
    $summary.status=if (@($summary.tests | Where-Object { -not $_.passed }).Count -eq 0) { 'checks_passed' } else { 'checks_failed' }
    # A successful disk export may report unsupported modules. That is an
    # expected compatibility finding, never evidence of successful takeover.
    if ($settings.mode -eq 'Interactive') {
        Start-Process -FilePath $binary -WorkingDirectory $app -ArgumentList '--settings' | Out-Null
        $summary.interactive_settings_opened=$true
    }
} catch {
    $summary.status='harness_failed'
    $summary.error=$_.Exception.Message
} finally {
    $summary.completed_at=(Get-Date).ToUniversalTime().ToString('o')
    Write-Json 'summary.json' $summary
    $table = @('# LiteTray guest results','',('Status: '+$summary.status),'','This result covers disk inventory and isolated checks. Live taskbar takeover and multiple monitors remain untested.','','| Check | Passed | Exit | Timeout |','| --- | --- | --- | --- |')
    foreach ($check in $summary.tests) { $table+=('| '+$check.name+' | '+$check.passed+' | '+$check.exit_code+' | '+$check.timed_out+' |') }
    $table+=@('','| Feature | Disk status |','| --- | --- |')
    foreach ($feature in $summary.feature_status) { $table+=('| '+$feature.feature+' | '+$feature.status+' |') }
    $table+=@('','Read os.json for the actual guest build and compatibility.json for module hashes/PE/PDB identities.','Interactive checks must be recorded separately; opening settings is not a passed taskbar test.')
    if ($summary.error) { $table+=@('',('Harness error: '+$summary.error)) }
    [IO.File]::WriteAllText((Join-Path $output 'summary.md'),($table -join "`r`n"),$utf8)
    $desktop = [Environment]::GetFolderPath('Desktop')
    [IO.File]::WriteAllText((Join-Path $desktop 'LiteTray-Test-Results.txt'),("Status: "+$summary.status+"`r`nResults: "+$output+"`r`nGuest work directory: "+$work+"`r`n"),$utf8)
}
