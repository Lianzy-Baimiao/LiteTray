[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][string]$AdapterPath,
    [switch]$IsolatedChild
)
$ErrorActionPreference = 'Stop'
$adapter = (Resolve-Path -LiteralPath $AdapterPath).Path
if ($IsolatedChild) {
    # This child only loads the adapter and invokes its isolated self-test.
    # It never enumerates taskbars, installs shell hooks or raises an exception.
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class FaultBeaconCheck {
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    static extern IntPtr LoadLibraryExW(string path, IntPtr file, uint flags);
    [DllImport("kernel32.dll", CharSet = CharSet.Ansi, ExactSpelling = true)]
    static extern IntPtr GetProcAddress(IntPtr module, string name);
    [DllImport("kernel32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    static extern bool FreeLibrary(IntPtr module);
    [DllImport("kernel32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    static extern bool GetModuleHandleExW(uint flags, IntPtr address, out IntPtr module);
    [UnmanagedFunctionPointer(CallingConvention.Winapi)]
    delegate int InstallBeacon();
    public static bool Run(string path) {
        var module = LoadLibraryExW(path, IntPtr.Zero, 0x00000100 | 0x00001000);
        if (module == IntPtr.Zero) throw new InvalidOperationException("Cannot load adapter: " + Marshal.GetLastWin32Error());
        var address = GetProcAddress(module, "LiteTraySelfTestFaultBeacon");
        if (address == IntPtr.Zero) throw new InvalidOperationException("Missing fault-beacon self-test export");
        var install = (InstallBeacon)Marshal.GetDelegateForFunctionPointer(address, typeof(InstallBeacon));
        if (install() != 0) throw new InvalidOperationException("Fault-beacon installation failed");
        if (install() != 0) throw new InvalidOperationException("Repeated installation failed");
        if (!FreeLibrary(module)) throw new InvalidOperationException("Cannot release caller module reference");
        // Do not invoke a function or throw an exception through a potentially
        // unloaded image. Query the loader for the exact former callback image.
        IntPtr retained;
        return GetModuleHandleExW(0x00000004 | 0x00000002, address, out retained) && retained == module;
    }
}
'@
    $retained = [FaultBeaconCheck]::Run($adapter)
    [pscustomobject]@{
        passed = $retained
        environment = 'isolated_fault_beacon_process'
        explorer_accessed = $false
        exception_raised = $false
        callback_module_retained_after_free_library = $retained
    } | ConvertTo-Json -Compress
    # No managed throw on the failing path: the regression is specifically a
    # dangling exception callback, so end this disposable process immediately.
    [Environment]::Exit($(if ($retained) { 0 } else { 1 }))
}
$output = Join-Path ([IO.Path]::GetTempPath()) ('litetray-fault-beacon-' + [Guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($output) | Out-Null
$stdout = Join-Path $output 'result.json'
$stderr = Join-Path $output 'error.txt'
$shell = (Get-Process -Id $PID).Path
$arguments = @('-NoProfile','-File',('"' + $PSCommandPath + '"'),'-AdapterPath',('"' + $adapter + '"'),'-IsolatedChild')
$process = Start-Process -FilePath $shell -ArgumentList $arguments -WindowStyle Hidden -PassThru -RedirectStandardOutput $stdout -RedirectStandardError $stderr
if (-not $process.WaitForExit(20000)) {
    $process.Kill()
    throw "Isolated fault-beacon check timed out. Logs: $output"
}
$process.WaitForExit()
if ($process.ExitCode -ne 0) {
    throw "Fault-beacon callback outlived its module or setup failed (exit $($process.ExitCode)). Logs: $output. $(Get-Content -LiteralPath $stdout -Raw) $(Get-Content -LiteralPath $stderr -Raw)"
}
$result = Get-Content -LiteralPath $stdout -Raw | ConvertFrom-Json
if ($result.passed -ne $true -or $result.explorer_accessed -ne $false -or $result.exception_raised -ne $false -or $result.callback_module_retained_after_free_library -ne $true) {
    throw "Invalid fault-beacon check result. Logs: $output"
}
$result | ConvertTo-Json
Write-Output "Logs: $output"
