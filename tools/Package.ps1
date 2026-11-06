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
Copy-Item -LiteralPath (Join-Path $projectRoot 'README.md') -Destination (Join-Path $output 'README.md')
Copy-Item -LiteralPath (Join-Path $projectRoot 'THIRD-PARTY-NOTICES.md') -Destination (Join-Path $output 'THIRD-PARTY-NOTICES.md')
$documents=@('docs/implementation-plan.md','docs/progress.md','docs/taskbar-source-binding.md','docs/live-clock-acceptance.md','docs/images/panel-light.png','docs/images/panel-dark.png','docs/images/agenda-dark.png','docs/images/sounds-dark.png','crates/taskbar-adapter/ABI.md','crates/popup-hook/README.md')
$documents += @('docs/images/settings-light.png','docs/images/settings-style-dark.png')
$documents += @('data/holidays/README.md','data/holidays/LICENSE')
$documents += @('docs/images/panel-editor-dark.png')
$documents += @('docs/ui-audit-5.5.md','docs/ui-audit-5.6.md','docs/images/focus-light.png')
foreach($name in $documents){
    $destination=Join-Path $output $name
    [IO.Directory]::CreateDirectory((Split-Path $destination -Parent))|Out-Null
    Copy-Item -LiteralPath (Join-Path $projectRoot $name) -Destination $destination
}
$hashes=foreach($name in (@('LiteTray.exe','litetray_taskbar_adapter.dll','litetray_popup_hook.dll','README.md','THIRD-PARTY-NOTICES.md')+$documents)){
    $hash=Get-FileHash -LiteralPath (Join-Path $output $name) -Algorithm SHA256
    $hash.Hash.ToLowerInvariant()+'  '+$name
}
[IO.File]::WriteAllText((Join-Path $output 'SHA256SUMS.txt'),($hashes -join [Environment]::NewLine)+[Environment]::NewLine)
Write-Output $output
