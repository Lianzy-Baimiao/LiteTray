[CmdletBinding()]
param([string]$BinaryPath,[string]$OutputDirectory)
$ErrorActionPreference='Stop'
$projectRoot=Split-Path $PSScriptRoot -Parent
if (-not $BinaryPath) {
    $standard=Join-Path $projectRoot 'target\release\LiteTray.exe'
    $local=Join-Path $env:USERPROFILE '.cargo-target\litetray\release\LiteTray.exe'
    $BinaryPath=if(Test-Path -LiteralPath $standard){$standard}else{$local}
}
if(-not (Test-Path -LiteralPath $BinaryPath)){throw 'Build the release executable first.'}
$adapter=Join-Path (Split-Path $BinaryPath -Parent) 'litetray_taskbar_adapter.dll'
$popupHook=Join-Path (Split-Path $BinaryPath -Parent) 'litetray_popup_hook.dll'
if(-not(Test-Path -LiteralPath $popupHook)){throw 'Build the complete release workspace; the popup hook DLL is required.'}
if(-not(Test-Path -LiteralPath $adapter)){throw 'Build the complete release workspace; the adapter DLL is required.'}
$output=if($OutputDirectory){$ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($OutputDirectory)}else{Join-Path $projectRoot 'dist\LiteTray-dev-windows-x64'}
[IO.Directory]::CreateDirectory($output)|Out-Null
function Copy-BinaryIfChanged([string]$Source, [string]$Destination) {
    # Explorer may retain an identical adapter DLL after a test. Do not try to
    # overwrite a loaded file when its content already matches this build.
    if ((Test-Path -LiteralPath $Destination) -and
        (Get-FileHash -LiteralPath $Source -Algorithm SHA256).Hash -eq
        (Get-FileHash -LiteralPath $Destination -Algorithm SHA256).Hash) { return }
    Copy-Item -LiteralPath $Source -Destination $Destination
}
Copy-BinaryIfChanged $BinaryPath (Join-Path $output 'LiteTray.exe')
Copy-BinaryIfChanged $adapter (Join-Path $output 'litetray_taskbar_adapter.dll')
Copy-BinaryIfChanged $popupHook (Join-Path $output 'litetray_popup_hook.dll')
function Add-ContentNamedDll([string]$Name) {
    # Derive both the name and contents from one read of the packaged source.
    $bytes=[IO.File]::ReadAllBytes((Join-Path $output $Name))
    $sha=[Security.Cryptography.SHA256]::Create()
    try { $digest=[BitConverter]::ToString($sha.ComputeHash($bytes)).Replace('-','').ToLowerInvariant() }
    finally { $sha.Dispose() }
    $contentName=[IO.Path]::GetFileNameWithoutExtension($Name)+'.'+$digest+'.dll'
    $destination=Join-Path $output $contentName
    if (-not (Test-Path -LiteralPath $destination)) {
        $pending=Join-Path $output ('.'+[Guid]::NewGuid().ToString('N')+'.tmp')
        try {
            $stream=[IO.File]::Open($pending,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
            try { $stream.Write($bytes,0,$bytes.Length); $stream.Flush($true) }
            finally { $stream.Dispose() }
            # File.Move without an overwrite argument never replaces a target.
            try { [IO.File]::Move($pending,$destination) }
            catch { if (-not (Test-Path -LiteralPath $destination)) { throw } }
        } finally { if (Test-Path -LiteralPath $pending) { Remove-Item -LiteralPath $pending } }
    }
    if (-not (Test-Path -LiteralPath $destination -PathType Leaf) -or
        (Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash.ToLowerInvariant() -ne $digest) {
        throw "Content-named DLL is invalid; it will not be overwritten: $destination"
    }
    return $contentName
}
$contentDlls=@(Add-ContentNamedDll 'litetray_taskbar_adapter.dll'; Add-ContentNamedDll 'litetray_popup_hook.dll')
Copy-Item -LiteralPath (Join-Path $projectRoot 'README.md') -Destination (Join-Path $output 'README.md')
Copy-Item -LiteralPath (Join-Path $projectRoot 'THIRD-PARTY-NOTICES.md') -Destination (Join-Path $output 'THIRD-PARTY-NOTICES.md')
$documents=@('docs/implementation-plan.md','docs/progress.md','docs/taskbar-source-binding.md','docs/live-clock-acceptance.md','docs/images/panel-light.png','docs/images/panel-dark.png','docs/images/agenda-dark.png','docs/images/sounds-dark.png','crates/taskbar-adapter/ABI.md','crates/popup-hook/README.md')
$documents += @('docs/images/settings-light.png','docs/images/settings-style-dark.png')
$documents += @('data/holidays/README.md','data/holidays/LICENSE')
$documents += @('docs/images/panel-editor-dark.png')
$documents += @('docs/ui-audit-5.5.md','docs/ui-audit-5.6.md','docs/images/focus-light.png','docs/weather-location.md','docs/images/weather-location-light.png')
$documents += @('docs/almanac-hover.md','docs/images/almanac-light.png','docs/images/almanac-dark.png','docs/images/solar-terms-light.png')
$documents += @('docs/hourly-weather.md','docs/images/hourly-weather-light.png')
$documents += @('docs/text-rendering.md')
$documents += @('docs/installer.md')
$documents += @('docs/festivals.md')
$documents += @('docs/weather-format.md')
$documents += @('docs/weather-icons.md','assets/icons/weather/README.md')
$documents += @('docs/clock-input-5.11.md')
$documents += @('docs/taskbar-integration-and-store.md')
$documents += @('README.en.md','CHANGELOG.md','docs/usage.md','docs/licensing.md','docs/images/taskbar-secondary.png')
$documents += @('docs/languages.md')
$documents += @('LICENSE','NOTICE')
$documents += @('docs/images/taskbar-5.12.0.png','docs/images/README.md')
$documents += @('docs/images/calendar-5.12.0-zh-light.png','docs/images/calendar-5.12.0-zh-dark.png')
$documents += @('docs/images/calendar-5.12.0-en-light.png','docs/images/calendar-5.12.0-en-dark.png')
$documents += @('docs/compatibility.md','docs/windows11-compatibility-plan.md','docs/windows11-compatibility-research.md')
$documents += @('docs/stability-5.13.1.md')
$documents += @('docs/sandbox-compatibility.md','docs/compatibility-matrix.template.json')
$documents += @('docs/sandbox-host-check-2026-11-08.md')
$documents += @('crates/adapter-protocol/profiles/README.md','crates/adapter-protocol/profiles/windows11-26300.9711-x64.json')
$documents += @('crates/adapter-protocol/profiles/windows11-26300.9818-x64.json','docs/compatibility-9818.md')
$documents += @('docs/compatibility-8457.md')
$documents += @('crates/adapter-protocol/profiles/windows11-26200.8457-x64.json')
$documents += @('crates/adapter-protocol/profiles/windows11-26200.8037-x64.json','crates/adapter-protocol/profiles/windows11-26100.1742-x64.json')
$documents += @('docs/compatibility-next-versions.md','docs/windows11-next-versions-research.md')
$documents += @('crates/adapter-protocol/profiles/windows11-22621.1702-x64.json','crates/adapter-protocol/profiles/windows11-22631.2861-x64.json','docs/compatibility-22h2-23h2.md')
$documents += @('docs/windows11-commonality.md','docs/images/compatibility-5.14.0-zh-light.png','docs/images/compatibility-5.14.0-en-light.png')
$documents += @('docs/module-recognition.md','crates/adapter-protocol/profiles/execution-fingerprints.json')
$documents += @('docs/compatibility-traits-5.15.0.md')
$documents += @('docs/hotplug-recovery-5.15.0.md')
$documents += @('docs/images/compatibility-5.15.0-zh-light.png','docs/images/compatibility-5.15.0-en-light.png')
$documents += @('docs/images/compatibility-5.15.1-zh-light.png','docs/images/compatibility-5.15.1-en-light.png')
$documents += @('docs/compatibility-completion-5.15.1.md')
$documents += @('docs/compatibility-21h2.md','docs/windows11-coverage-next-research.md')
$documents += @('crates/adapter-protocol/profiles/windows11-22000.318-x64-clock.json')
$documents += @('docs/compatibility-child-contracts.md','docs/compatibility-release-5.16.0.md','docs/windows11-arm64-test-environment.md')
$documents += @('docs/compatibility-release-5.16.1.md')
$documents += @('crates/adapter-protocol/profiles/windows11-28000.1-x64-notification.json','docs/compatibility-26h1.md')
$documents += @('docs/startup-weather-5.16.3.md')
$documents += @('docs/proxy-settings-5.16.4.md')
$documents += @('docs/diagnostic-responsiveness-5.16.4.md')
foreach($name in $documents){
    $destination=Join-Path $output $name
    [IO.Directory]::CreateDirectory((Split-Path $destination -Parent))|Out-Null
    Copy-Item -LiteralPath (Join-Path $projectRoot $name) -Destination $destination
}
$hashes=foreach($name in (@('LiteTray.exe','litetray_taskbar_adapter.dll','litetray_popup_hook.dll','README.md','THIRD-PARTY-NOTICES.md')+$contentDlls+$documents)){
    $hash=Get-FileHash -LiteralPath (Join-Path $output $name) -Algorithm SHA256
    $hash.Hash.ToLowerInvariant()+'  '+$name
}
[IO.File]::WriteAllText((Join-Path $output 'SHA256SUMS.txt'),($hashes -join [Environment]::NewLine)+[Environment]::NewLine)
Write-Output $output
