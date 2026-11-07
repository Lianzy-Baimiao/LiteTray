[CmdletBinding()]
param(
    [string]$BinaryPath,
    [string]$CompilerPath,
    [string]$OutputDirectory,
    [switch]$SkipBuild
)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path $PSScriptRoot -Parent
if (-not $CompilerPath) {
    $candidates = @(
        (Join-Path $env:USERPROFILE 'tools\InnoSetup7\ISCC.exe'),
        (Join-Path ${env:ProgramFiles(x86)} 'Inno Setup 7\ISCC.exe'),
        (Join-Path ${env:ProgramFiles(x86)} 'Inno Setup 6\ISCC.exe')
    )
    $command = Get-Command ISCC.exe -ErrorAction SilentlyContinue
    if ($command) { $candidates = @($command.Source) + $candidates }
    $CompilerPath = $candidates | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
}
if (-not $CompilerPath -or -not (Test-Path -LiteralPath $CompilerPath)) {
    throw 'Install Inno Setup 7 (or 6.5+) and pass -CompilerPath <ISCC.exe>.'
}
if (-not $SkipBuild) {
    & (Join-Path $PSScriptRoot 'Invoke-Cargo.ps1') -CargoArguments @('build','--workspace','--release','--locked')
}
$source = [IO.File]::ReadAllText((Join-Path $projectRoot 'src\ui.rs'))
$match = [regex]::Match($source, 'const UI_REVISION: &str = "开发版 ([0-9]+\.[0-9]+(?:\.[0-9]+)?)";')
if (-not $match.Success) { throw 'Cannot determine the UI release version.' }
$version = $match.Groups[1].Value
if ($version.Split('.').Count -eq 2) { $version += '.0' }
$payload = Join-Path $projectRoot 'build\installer-payload'
& (Join-Path $PSScriptRoot 'Package.ps1') -BinaryPath $BinaryPath -OutputDirectory $payload | Out-Null
$names = @('LiteTray.exe','litetray_taskbar_adapter.dll','litetray_popup_hook.dll')
$hashes = @($names | ForEach-Object { (Get-FileHash -LiteralPath (Join-Path $payload $_) -Algorithm SHA256).Hash.ToLowerInvariant() })
$sha = [Security.Cryptography.SHA256]::Create()
try {
    $combined = $sha.ComputeHash([Text.Encoding]::ASCII.GetBytes($hashes -join ':'))
    $buildId = $version + '-' + ([BitConverter]::ToString($combined).Replace('-', '').ToLowerInvariant().Substring(0, 12))
} finally { $sha.Dispose() }
$output = if ($OutputDirectory) { $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($OutputDirectory) } else { Join-Path $projectRoot 'dist' }
[IO.Directory]::CreateDirectory($output) | Out-Null
$arguments = @(
    '/Q', "/DPayloadDir=$payload", "/DAppVersion=$version", "/DBuildId=$buildId",
    "/DExeHash=$($hashes[0])", "/DAdapterHash=$($hashes[1])", "/DHookHash=$($hashes[2])",
    "/O$output", (Join-Path $projectRoot 'installer\LiteTray.iss')
)
& $CompilerPath @arguments
if ($LASTEXITCODE -ne 0) { throw "Inno Setup failed with exit code $LASTEXITCODE" }
$setupBase = "LiteTray-$version-Setup-x64"
$setupName = "$setupBase.exe"
$setup = Join-Path $output $setupName
$setupHash = (Get-FileHash -LiteralPath $setup -Algorithm SHA256).Hash.ToLowerInvariant()
[IO.File]::WriteAllText((Join-Path $output "$setupName.sha256"), "$setupHash  $setupName`r`n")
$manifest = [ordered]@{
    version = $version
    buildId = $buildId
    setup = $setupName
    sha256 = $setupHash
    executable = "versions\$buildId\LiteTray.exe"
    binaries = [ordered]@{}
}
for ($i = 0; $i -lt $names.Count; $i++) { $manifest.binaries[$names[$i]] = $hashes[$i] }
$manifest | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $output "$setupBase.manifest.json") -Encoding utf8
[pscustomobject]@{ Setup = $setup; Version = $version; BuildId = $buildId; SizeMiB = [math]::Round((Get-Item -LiteralPath $setup).Length / 1MB, 2); SHA256 = $setupHash }
