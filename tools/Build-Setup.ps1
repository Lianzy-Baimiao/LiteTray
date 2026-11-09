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
$metadataText = & cargo metadata --format-version 1 --no-deps --locked --manifest-path (Join-Path $projectRoot 'Cargo.toml')
if ($LASTEXITCODE -ne 0) { throw 'Cannot read the Cargo release version.' }
$metadata = ($metadataText -join "`n") | ConvertFrom-Json
$package = @($metadata.packages | Where-Object { $_.name -eq 'litetray' })
if ($package.Count -ne 1 -or $package[0].version -notmatch '^\d+\.\d+\.\d+$') {
    throw 'The LiteTray package must have one numeric major.minor.patch release version.'
}
$version = $package[0].version
$payload = Join-Path $projectRoot 'build\installer-payload'
& (Join-Path $PSScriptRoot 'Package.ps1') -BinaryPath $BinaryPath -OutputDirectory $payload | Out-Null
$executableVersion = [Diagnostics.FileVersionInfo]::GetVersionInfo((Join-Path $payload 'LiteTray.exe'))
if ($executableVersion.ProductVersion -ne $version -or $executableVersion.FileVersion -ne $version) {
    throw "The packaged executable version ($($executableVersion.ProductVersion)) does not match Cargo ($version). Rebuild before packaging."
}
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
for ($i = 1; $i -lt $names.Count; $i++) {
    $contentName=[IO.Path]::GetFileNameWithoutExtension($names[$i])+'.'+$hashes[$i]+'.dll'
    $manifest.binaries[$contentName] = $hashes[$i]
}
$manifest | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $output "$setupBase.manifest.json") -Encoding utf8
[pscustomobject]@{ Setup = $setup; Version = $version; BuildId = $buildId; SizeMiB = [math]::Round((Get-Item -LiteralPath $setup).Length / 1MB, 2); SHA256 = $setupHash }
