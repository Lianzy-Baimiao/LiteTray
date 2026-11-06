[CmdletBinding()]
param([string]$OutputDirectory = 'local\ui-renders')
$ErrorActionPreference = 'Stop'
$output = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($OutputDirectory)
$previous = $env:LITETRAY_RENDER_DIR
try {
    $env:LITETRAY_RENDER_DIR = $output
    & (Join-Path $PSScriptRoot 'Invoke-Cargo.ps1') -CargoArguments @('test','--bin','LiteTray','render_settings_artifacts','--','--ignored','--nocapture')
    Add-Type -AssemblyName System.Drawing
    Get-ChildItem -LiteralPath $output -Filter 'settings-*.bmp' | ForEach-Object {
        $bitmap = [Drawing.Bitmap]::FromFile($_.FullName)
        try { $bitmap.Save([IO.Path]::ChangeExtension($_.FullName, '.png'), [Drawing.Imaging.ImageFormat]::Png) }
        finally { $bitmap.Dispose() }
    }
    Write-Output "Native UI renders: $output"
}
finally { [Environment]::SetEnvironmentVariable('LITETRAY_RENDER_DIR', $previous, 'Process') }
