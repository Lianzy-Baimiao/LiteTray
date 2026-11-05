[CmdletBinding()]
param(
    [string]$OutputPath = (Join-Path $PSScriptRoot '..\local\windhawk-profile.json'),
    [switch]$Force
)

$ErrorActionPreference = 'Stop'
$registryRoot = 'HKLM:\SOFTWARE\Windhawk\Engine\Mods'
$moduleIds = @('taskbar-multi-tray', 'taskbar-clock-customization')
$moduleProfiles = [ordered]@{}

foreach ($moduleId in $moduleIds) {
    $modulePath = Join-Path $registryRoot $moduleId
    if (-not (Test-Path -LiteralPath $modulePath)) {
        throw "Required Windhawk module was not found: $moduleId"
    }

    $moduleInfo = Get-ItemProperty -LiteralPath $modulePath
    $settingsPath = Join-Path $modulePath 'Settings'
    $settings = [ordered]@{}
    if (Test-Path -LiteralPath $settingsPath) {
        $settingsKey = Get-Item -LiteralPath $settingsPath
        foreach ($settingName in ($settingsKey.GetValueNames() | Sort-Object)) {
            $settings[$settingName] = $settingsKey.GetValue($settingName)
        }
    }

    $moduleProfiles[$moduleId] = [ordered]@{
        version = $moduleInfo.Version
        enabled = ($moduleInfo.Disabled -eq 0)
        settings = $settings
    }
}

$osInfo = Get-ItemProperty -LiteralPath 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion'
$profile = [ordered]@{
    schemaVersion = 1
    exportedAt = [DateTimeOffset]::Now.ToString('o')
    purpose = 'Read-only migration baseline; not a LiteTray runtime configuration.'
    windows = [ordered]@{
        displayVersion = $osInfo.DisplayVersion
        build = "$($osInfo.CurrentBuild).$($osInfo.UBR)"
    }
    modules = $moduleProfiles
}

$resolvedOutput = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($OutputPath)
$outputDirectory = Split-Path -Path $resolvedOutput -Parent
if (-not (Test-Path -LiteralPath $outputDirectory)) {
    [IO.Directory]::CreateDirectory($outputDirectory) | Out-Null
}

$json = $profile | ConvertTo-Json -Depth 12
$fileMode = if ($Force) { [IO.FileMode]::Create } else { [IO.FileMode]::CreateNew }
$outputStream = [IO.File]::Open($resolvedOutput, $fileMode, [IO.FileAccess]::Write, [IO.FileShare]::None)
try {
    $encoded = [Text.UTF8Encoding]::new($false).GetBytes($json + [Environment]::NewLine)
    $outputStream.Write($encoded, 0, $encoded.Length)
}
finally {
    $outputStream.Dispose()
}
Write-Output "Exported profile: $resolvedOutput"
