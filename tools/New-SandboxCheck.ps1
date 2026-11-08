[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][string]$PackageDirectory,
    [string]$OutputDirectory,
    [ValidateSet('Isolated','Interactive')][string]$Mode = 'Isolated',
    [ValidateSet('Enable','Disable')][string]$VGpu = 'Enable',
    [ValidateRange(2048,16384)][int]$MemoryInMB = 4096
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

# This generator never starts a program from the package or launches Sandbox.
# SHA256SUMS verifies consistency with the chosen package, not its publisher.
$package = (Resolve-Path -LiteralPath $PackageDirectory).Path
if (-not (Test-Path -LiteralPath $package -PathType Container)) {
    throw 'PackageDirectory must be an unpacked package directory.'
}
$manifest = Join-Path $package 'SHA256SUMS.txt'
if (-not (Test-Path -LiteralPath $manifest -PathType Leaf)) {
    throw 'The package must include SHA256SUMS.txt from tools/Package.ps1.'
}
function Assert-NoReparsePath([string]$Path) {
    $current = Get-Item -LiteralPath $Path -Force
    while ($null -ne $current) {
        if (($current.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw "Reparse points are not accepted in sandbox inputs/output: $($current.FullName)"
        }
        $current = if ($current -is [IO.DirectoryInfo]) { $current.Parent } else { $current.Directory }
    }
}
Assert-NoReparsePath $manifest
$prefix = $package.TrimEnd('\') + '\'
$files = [Collections.Generic.List[object]]::new()
$names = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
foreach ($line in [IO.File]::ReadAllLines($manifest)) {
    if ([string]::IsNullOrWhiteSpace($line)) { continue }
    if ($line -notmatch '^([0-9a-fA-F]{64})  (.+)$') { throw "Malformed package hash entry: $line" }
    $digest = $Matches[1].ToLowerInvariant()
    $relative = $Matches[2].Replace('/','\')
    # Reject roots, traversal, ADS, shell metacharacters and ambiguous Win32 names.
    if ($relative -match '[\x00-\x1f:*?"<>|]' -or [IO.Path]::IsPathRooted($relative)) {
        throw "Unsafe package path: $relative"
    }
    foreach ($part in $relative.Split('\')) {
        if (-not $part -or $part -in @('.','..') -or $part.EndsWith('.') -or $part.EndsWith(' ') -or
            $part -match '^(CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])(?:\.|$)') {
            throw "Unsafe package path component: $relative"
        }
    }
    if (-not $names.Add($relative)) { throw "Duplicate package path: $relative" }
    $source = [IO.Path]::GetFullPath((Join-Path $package $relative))
    if (-not $source.StartsWith($prefix,[StringComparison]::OrdinalIgnoreCase) -or
        -not (Test-Path -LiteralPath $source -PathType Leaf)) {
        throw "Missing or out-of-package file: $relative"
    }
    Assert-NoReparsePath $source
    if ((Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash.ToLowerInvariant() -ne $digest) {
        throw "Package hash mismatch: $relative"
    }
    $files.Add([pscustomobject]@{ relative_path=$relative; sha256=$digest; source=$source })
}
foreach ($required in @('LiteTray.exe','litetray_taskbar_adapter.dll','litetray_popup_hook.dll')) {
    if (-not $names.Contains($required)) { throw "Required binary absent from package manifest: $required" }
}
$runId = 'sandbox-' + (Get-Date -Format 'yyyyMMdd-HHmmss') + '-' + [Guid]::NewGuid().ToString('N').Substring(0,8)
if ($OutputDirectory) {
    $run = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($OutputDirectory)
} else {
    $run = Join-Path (Split-Path $PSScriptRoot -Parent) ('local\sandbox-checks\' + $runId)
}
$run = [IO.Path]::GetFullPath($run)
if (Test-Path -LiteralPath $run) { throw "OutputDirectory must be a new directory; nothing will be overwritten: $run" }
if ($run.StartsWith($prefix,[StringComparison]::OrdinalIgnoreCase)) {
    throw 'OutputDirectory must be outside the source package.'
}
$parent = Split-Path $run -Parent
$existingParent = $parent
while (-not (Test-Path -LiteralPath $existingParent)) {
    $existingParent = Split-Path $existingParent -Parent
    if (-not $existingParent) { throw 'OutputDirectory has no existing filesystem parent.' }
}
Assert-NoReparsePath $existingParent
$guestScript = Join-Path $PSScriptRoot 'Invoke-SandboxGuestCheck.ps1'
if (-not (Test-Path -LiteralPath $guestScript -PathType Leaf)) { throw 'Guest check script is missing.' }

$inputDirectory = Join-Path $run 'input'
$snapshot = Join-Path $inputDirectory 'package'
$results = Join-Path $run 'results'
[IO.Directory]::CreateDirectory($snapshot) | Out-Null
[IO.Directory]::CreateDirectory($results) | Out-Null
foreach ($file in $files) {
    $destination = Join-Path $snapshot $file.relative_path
    [IO.Directory]::CreateDirectory((Split-Path $destination -Parent)) | Out-Null
    Copy-Item -LiteralPath $file.source -Destination $destination
    if ((Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash.ToLowerInvariant() -ne $file.sha256) {
        throw "Package changed during copy; discard this incomplete run: $($file.relative_path)"
    }
}
$utf8 = [Text.UTF8Encoding]::new($false)
$manifestText = ($files | ForEach-Object { $_.sha256 + '  ' + $_.relative_path.Replace('\','/') }) -join "`r`n"
[IO.File]::WriteAllText((Join-Path $snapshot 'SHA256SUMS.txt'),$manifestText + "`r`n",$utf8)
Copy-Item -LiteralPath $guestScript -Destination (Join-Path $inputDirectory 'Invoke-SandboxGuestCheck.ps1')
$settings = [ordered]@{
    schema=1
    run_id=$runId
    created_at=(Get-Date).ToUniversalTime().ToString('o')
    mode=$Mode
    vgpu=$VGpu
    memory_mb=$MemoryInMB
    package_manifest_sha256=(Get-FileHash -LiteralPath (Join-Path $snapshot 'SHA256SUMS.txt') -Algorithm SHA256).Hash.ToLowerInvariant()
    package_files=@($files | Select-Object relative_path,sha256)
    tests=@('--compatibility-report','--diagnose','--self-test-app-icon','--self-test-clock')
    explorer_takeover_automated=$false
}
[IO.File]::WriteAllText((Join-Path $inputDirectory 'run.json'),($settings | ConvertTo-Json -Depth 8),$utf8)
$escape = { param([string]$Value) [Security.SecurityElement]::Escape($Value) }
$inputXml = & $escape $inputDirectory
$resultsXml = & $escape $results
$configuration = @"
<Configuration>
  <VGpu>$VGpu</VGpu>
  <Networking>Disable</Networking>
  <ClipboardRedirection>Disable</ClipboardRedirection>
  <AudioInput>Disable</AudioInput>
  <VideoInput>Disable</VideoInput>
  <PrinterRedirection>Disable</PrinterRedirection>
  <MemoryInMB>$MemoryInMB</MemoryInMB>
  <MappedFolders>
    <MappedFolder>
      <HostFolder>$inputXml</HostFolder>
      <SandboxFolder>C:\LiteTrayInput</SandboxFolder>
      <ReadOnly>true</ReadOnly>
    </MappedFolder>
    <MappedFolder>
      <HostFolder>$resultsXml</HostFolder>
      <SandboxFolder>C:\LiteTrayResults</SandboxFolder>
      <ReadOnly>false</ReadOnly>
    </MappedFolder>
  </MappedFolders>
  <LogonCommand>
    <Command>powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File C:\LiteTrayInput\Invoke-SandboxGuestCheck.ps1 -EnvironmentType WindowsSandbox</Command>
  </LogonCommand>
</Configuration>
"@
$wsb = Join-Path $run 'LiteTray-Check.wsb'
[IO.File]::WriteAllText($wsb,$configuration,$utf8)
$readme = @"
LiteTray isolated compatibility run

This directory was generated only. Windows Sandbox has NOT been launched.
Review LiteTray-Check.wsb and input\run.json, then open the .wsb yourself.

Mode: $Mode
vGPU: $VGpu; RAM: $MemoryInMB MiB
Networking, clipboard, camera, microphone and printer redirection: disabled
Read-only mapping: input (verified package snapshot and guest scripts)
Only writable host mapping: results (a dedicated directory for this run)

The guest copies the package into its own local directory, verifies hashes,
and runs only disk diagnostics and isolated self-tests. It does not enable
taskbar clock takeover, tray mirroring or QQ hooks automatically.
Interactive mode additionally opens settings with those features disabled.
Enable a taskbar feature manually inside the guest only when testing it.

Each guest invocation writes a fresh results subdirectory. Read summary.json,
summary.md, os.json, compatibility.json and the individual stdout/stderr logs.
No host user configuration, credentials or Windows directory is mapped.
Package SHA-256 verification checks consistency, not publisher authenticity.

Sandbox cannot select an older Windows build. Its OS follows the host's
Sandbox image; os.json and compatibility.json record the actual guest.
Use separately installed VMs and snapshots for real cross-version coverage.
Do not infer multiple-monitor or physical-GPU compatibility from this run.
"@
[IO.File]::WriteAllText((Join-Path $run 'README.txt'),$readme,$utf8)
[pscustomobject]@{
    run_directory=$run
    configuration=$wsb
    results_directory=$results
    mode=$Mode
    verified_files=$files.Count
    launched=$false
} | ConvertTo-Json
