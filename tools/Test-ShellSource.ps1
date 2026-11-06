[CmdletBinding()]
param([Parameter(Mandatory=$true)][string]$BinaryPath)
$ErrorActionPreference='Stop'
$executable=(Resolve-Path -LiteralPath $BinaryPath).Path
$output=Join-Path ([IO.Path]::GetTempPath()) ('litetray-shell-source-check-'+[Guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($output)|Out-Null
$stdout=Join-Path $output 'result.json'
$stderr=Join-Path $output 'error.txt'
# A distinct process is essential: this private host initializes XAML itself.
# The command performs no profile access, taskbar enumeration or hook setup.
$process=Start-Process -FilePath $executable -ArgumentList '--self-test-shell-source' -WindowStyle Hidden -PassThru -RedirectStandardOutput $stdout -RedirectStandardError $stderr
if(-not $process.WaitForExit(20000)) {
    $process.Kill()
    throw "Isolated private host check timed out. Logs: $output"
}
$process.WaitForExit()
if($process.ExitCode -ne 0){
    $exitDescription='{0} (0x{1:X8})' -f $process.ExitCode, ([long]$process.ExitCode -band 0xffffffffL)
    throw "Isolated private host check failed with exit $exitDescription. Logs: $output. $(Get-Content -LiteralPath $stderr -Raw)"
}
$result=Get-Content -LiteralPath $stdout -Raw|ConvertFrom-Json
if($result.passed -ne $true -or $result.explorer_accessed -ne $false -or $result.production_taskbar_verified -ne $false -or $result.environment -ne 'isolated_private_shell_host_in_app_process' -or $result.checks.Count -lt 6){throw 'Invalid private host check result.'}
$result|ConvertTo-Json -Depth 5
Write-Output "Logs: $output"
