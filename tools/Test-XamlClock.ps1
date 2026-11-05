[CmdletBinding()]
param([Parameter(Mandatory=$true)][string]$BinaryPath)
$ErrorActionPreference='Stop'
$executable=(Resolve-Path -LiteralPath $BinaryPath).Path
$output=Join-Path ([IO.Path]::GetTempPath()) ('litetray-xaml-check-'+[Guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($output)|Out-Null
$previous=[Environment]::GetEnvironmentVariable('LITETRAY_DATA_DIR','Process')
try {
    $env:LITETRAY_DATA_DIR=$output
    $stdout=Join-Path $output 'result.json'
    $stderr=Join-Path $output 'error.txt'
    $process=Start-Process -FilePath $executable -ArgumentList '--self-test-clock' -WindowStyle Hidden -PassThru -RedirectStandardOutput $stdout -RedirectStandardError $stderr
    if(-not $process.WaitForExit(30000)) {
        $process.Kill()
        throw "Isolated XAML check timed out. Logs: $output"
    }
    # Flush asynchronous redirected output before inspecting the result.
    $process.WaitForExit()
    if($process.ExitCode -ne 0){throw "Isolated XAML check failed: $(Get-Content -LiteralPath $stderr -Raw)"}
    $result=Get-Content -LiteralPath $stdout -Raw|ConvertFrom-Json
    if($result.passed -ne $true -or $result.explorer_accessed -ne $false -or $result.environment -ne 'isolated_xaml_in_host_process'){throw 'Invalid isolated check result.'}
    $result|ConvertTo-Json -Depth 5
    Write-Output "Logs: $output"
}
finally {
    [Environment]::SetEnvironmentVariable('LITETRAY_DATA_DIR',$previous,'Process')
}
