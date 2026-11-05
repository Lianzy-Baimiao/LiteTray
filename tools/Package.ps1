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
Copy-Item -LiteralPath (Join-Path $projectRoot 'README.md') -Destination (Join-Path $output 'README.md')
$hash=Get-FileHash -LiteralPath (Join-Path $output 'LiteTray.exe') -Algorithm SHA256
[IO.File]::WriteAllText((Join-Path $output 'SHA256SUMS.txt'),$hash.Hash.ToLowerInvariant()+'  LiteTray.exe'+[Environment]::NewLine)
Write-Output $output
