[CmdletBinding()]
param([string]$OutputDirectory)
$ErrorActionPreference='Stop'
$projectRoot=Split-Path $PSScriptRoot -Parent
$output=if($OutputDirectory){$ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($OutputDirectory)}else{Join-Path $projectRoot 'local\panel-renders'}
$previous=[Environment]::GetEnvironmentVariable('LITETRAY_RENDER_DIR','Process')
Push-Location -LiteralPath $projectRoot
try {
    $env:LITETRAY_RENDER_DIR=$output
    & (Join-Path $PSScriptRoot 'Invoke-Cargo.ps1') -CargoArguments @('test','--bin','LiteTray','render_panel_artifacts','--','--ignored','--nocapture')
    Add-Type -AssemblyName System.Drawing
    Get-ChildItem -LiteralPath $output -Filter 'panel-*.bmp' | ForEach-Object {
        $bitmap=[Drawing.Bitmap]::FromFile($_.FullName)
        try { $bitmap.Save([IO.Path]::ChangeExtension($_.FullName,'.png'),[Drawing.Imaging.ImageFormat]::Png) }
        finally { $bitmap.Dispose() }
    }
    Write-Output "Offscreen UI renders (sample weather, no desktop windows): $output"
}
finally {
    [Environment]::SetEnvironmentVariable('LITETRAY_RENDER_DIR',$previous,'Process')
    Pop-Location
}
