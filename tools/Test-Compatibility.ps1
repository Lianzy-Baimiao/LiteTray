[CmdletBinding()]
param([Parameter(Mandatory=$true)][string]$BinaryPath)
$ErrorActionPreference='Stop'
$executable=(Resolve-Path -LiteralPath $BinaryPath).Path
$directory=Join-Path ([IO.Path]::GetTempPath()) ('litetray-compatibility-'+[Guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($directory)|Out-Null
# Copy only the EXE: disk export must work without either adapter DLL.
$isolated=Join-Path $directory 'LiteTray.exe'
Copy-Item -LiteralPath $executable -Destination $isolated
$data=Join-Path $directory 'profile'
[IO.Directory]::CreateDirectory($data)|Out-Null
$config=Join-Path $data 'config.json'
[IO.File]::WriteAllText($config,'deliberately invalid config')
$before=(Get-FileHash -LiteralPath $config).Hash
$previous=$env:LITETRAY_DATA_DIR
function Run-Export([string]$Name,[string[]]$Arguments,[bool]$ShouldSucceed=$true) {
    $stdout=Join-Path $directory "$Name.stdout.txt"
    $stderr=Join-Path $directory "$Name.stderr.txt"
    $process=Start-Process -FilePath $isolated -ArgumentList $Arguments -WindowStyle Hidden -PassThru -RedirectStandardOutput $stdout -RedirectStandardError $stderr
    if(-not $process.WaitForExit(15000)) { $process.Kill(); throw "Compatibility export timed out: $directory" }
    $process.WaitForExit()
    if(($process.ExitCode -eq 0) -ne $ShouldSucceed) { throw "Unexpected exit $($process.ExitCode): $(Get-Content -LiteralPath $stderr -Raw)" }
}
try {
    $env:LITETRAY_DATA_DIR=$data
    $live=Join-Path $directory 'current.json'
    Run-Export 'current' @('--compatibility-report',('"'+$live+'"'))
    $report=Get-Content -LiteralPath $live -Raw|ConvertFrom-Json
    if($report.scope -ne 'disk_only' -or $report.explorer_accessed -ne $false -or $report.network_accessed -ne $false -or $report.loaded_modules_checked -ne $false -or $report.runtime_topology_checked -ne $false -or $report.modules.Count -ne 3 -or $report.features.Count -ne 4) {throw 'Invalid compatibility report'}
    $hash=(Get-FileHash -LiteralPath $live).Hash
    Run-Export 'existing-output' @('--compatibility-report',('"'+$live+'"')) $false
    if((Get-FileHash -LiteralPath $live).Hash -ne $hash) {throw 'Existing report was overwritten'}
    $offline=Join-Path $directory 'offline Windows'
    [IO.Directory]::CreateDirectory((Join-Path $offline 'System32'))|Out-Null
    [IO.File]::WriteAllText((Join-Path $offline 'System32\taskbar.dll'),'not a PE')
    $missing=Join-Path $directory 'offline.json'
    Run-Export 'offline' @('--compatibility-report',('"'+$missing+'"'),'--system-root',('"'+$offline+'"'))
    $result=Get-Content -LiteralPath $missing -Raw|ConvertFrom-Json
    if($null -ne $result.os_build -or $result.all_profile_images_matched -ne $false -or $result.modules[0].status -ne 'invalid_image' -or $result.modules[1].status -ne 'missing' -or $result.features[0].status -ne 'inspection_failed') {throw 'Offline failures misreported as support'}
    if((Get-FileHash -LiteralPath $config).Hash -ne $before -or (Test-Path -LiteralPath (Join-Path $data 'symbols')) -or (Test-Path -LiteralPath (Join-Path $data 'adapters'))) {throw 'Disk export changed config or created symbol caches'}
    [pscustomobject]@{Passed=$true; Checks=5; CurrentImagesMatched=$report.all_profile_images_matched; Logs=$directory}
} finally { [Environment]::SetEnvironmentVariable('LITETRAY_DATA_DIR',$previous,'Process') }
