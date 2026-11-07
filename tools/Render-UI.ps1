[CmdletBinding()]
param([string]$OutputDirectory = 'local\ui-renders', [ValidateSet('zh-cn','en')][string]$Language = 'zh-cn')
$ErrorActionPreference = 'Stop'
$output = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($OutputDirectory)
$previous = $env:LITETRAY_RENDER_DIR
$previousLanguage = $env:LITETRAY_RENDER_LANGUAGE
try {
    $env:LITETRAY_RENDER_DIR = $output
    $env:LITETRAY_RENDER_LANGUAGE = $Language
    & (Join-Path $PSScriptRoot 'Invoke-Cargo.ps1') -CargoArguments @('test','--bin','LiteTray','render_settings_artifacts','--','--ignored','--nocapture')
    Add-Type -AssemblyName System.Drawing
    Get-ChildItem -LiteralPath $output -Filter 'settings-*.bmp' | ForEach-Object {
        $bitmap = [Drawing.Bitmap]::FromFile($_.FullName)
        try { $bitmap.Save([IO.Path]::ChangeExtension($_.FullName, '.png'), [Drawing.Imaging.ImageFormat]::Png) }
        finally { $bitmap.Dispose() }
    }
    Write-Output "Native UI renders: $output"
}
finally {
    [Environment]::SetEnvironmentVariable('LITETRAY_RENDER_DIR', $previous, 'Process')
    [Environment]::SetEnvironmentVariable('LITETRAY_RENDER_LANGUAGE', $previousLanguage, 'Process')
}
