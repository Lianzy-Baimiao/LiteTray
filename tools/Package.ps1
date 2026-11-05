[CmdletBinding()]
param([string]$BinaryPath)
$ErrorActionPreference='Stop'
$projectRoot=Split-Path $PSScriptRoot -Parent
if (-not $BinaryPath) {
    $standard=Join-Path $projectRoot 'target\release\LiteTray.exe'
    $local=Join-Path $env:USERPROFILE '.cargo-target\litetray\release\LiteTray.exe'
    $BinaryPath=if(Test-Path -LiteralPath $standard){$standard}else{$local}
}
if(-not (Test-Path -LiteralPath $BinaryPath)){throw 'Build the release executable first.'}
$output=Join-Path $projectRoot 'dist\LiteTray-dev-windows-x64'
[IO.Directory]::CreateDirectory($output)|Out-Null
Copy-Item -LiteralPath $BinaryPath -Destination (Join-Path $output 'LiteTray.exe')
$adapter=Join-Path (Split-Path $BinaryPath -Parent) 'litetray_taskbar_adapter.dll'
if(-not(Test-Path -LiteralPath $adapter)){throw 'Build the complete release workspace; the adapter DLL is required.'}
Copy-Item -LiteralPath $adapter -Destination (Join-Path $output 'litetray_taskbar_adapter.dll')
Copy-Item -LiteralPath (Join-Path $projectRoot 'README.md') -Destination (Join-Path $output 'README.md')
Copy-Item -LiteralPath (Join-Path $projectRoot 'THIRD-PARTY-NOTICES.md') -Destination (Join-Path $output 'THIRD-PARTY-NOTICES.md')
$hashes=foreach($name in @('LiteTray.exe','litetray_taskbar_adapter.dll','README.md','THIRD-PARTY-NOTICES.md')){
    $hash=Get-FileHash -LiteralPath (Join-Path $output $name) -Algorithm SHA256
    $hash.Hash.ToLowerInvariant()+'  '+$name
}
[IO.File]::WriteAllText((Join-Path $output 'SHA256SUMS.txt'),($hashes -join [Environment]::NewLine)+[Environment]::NewLine)
Write-Output $output
