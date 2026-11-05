[CmdletBinding()]
param([Parameter(ValueFromRemainingArguments=$true)][string[]]$CargoArguments = @('build'))

$ErrorActionPreference = 'Stop'
$names = @('PATH','RUSTFLAGS','CARGO_TARGET_DIR','CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER','WINDRES','AR')
$previous = @{}
foreach ($name in $names) { $previous[$name] = [Environment]::GetEnvironmentVariable($name,'Process') }
try {
    $hostInfo = rustc -vV
    if ($hostInfo -match 'host: x86_64-pc-windows-gnu') {
        $mingw = if ($env:LITETRAY_MINGW_BIN) { $env:LITETRAY_MINGW_BIN } else { Join-Path $env:USERPROFILE 'tools\mingw64\bin' }
        if (-not (Test-Path -LiteralPath (Join-Path $mingw 'dlltool.exe'))) {
            throw 'Set LITETRAY_MINGW_BIN to a MinGW bin directory, or use Rust MSVC with Visual Studio C++ Build Tools.'
        }
        $env:PATH = $mingw + [IO.Path]::PathSeparator + $env:PATH
        $env:RUSTFLAGS = '-C dlltool=' + (Join-Path $mingw 'dlltool.exe')
        $env:CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER = Join-Path $mingw 'gcc.exe'
        $env:WINDRES = Join-Path $mingw 'windres.exe'
        $env:AR = Join-Path $mingw 'ar.exe'
        # GNU resource/linker tools need an ASCII artifact path on this machine.
        $env:CARGO_TARGET_DIR = Join-Path $env:USERPROFILE '.cargo-target\litetray'
        # Recent MinGW injects its own manifest. The app embeds a complete DPI/v6
        # manifest, so exclude only the compiler's default resource at link time.
        $gccSpecs = @(& (Join-Path $mingw 'gcc.exe') -dumpspecs)
        $endfileIndex = [Array]::IndexOf($gccSpecs, '*endfile:')
        if ($endfileIndex -ge 0) {
            $endfile = $gccSpecs[$endfileIndex + 1]
            $defaultManifest = '%{!shared:%:if-exists(default-manifest.o%s)}'
            if ($endfile.Contains($defaultManifest)) {
                [IO.Directory]::CreateDirectory($env:CARGO_TARGET_DIR) | Out-Null
                $specPath = Join-Path $env:CARGO_TARGET_DIR 'litetray-manifest.specs'
                [IO.File]::WriteAllText($specPath, "*endfile:`n" + $endfile.Replace($defaultManifest, '') + "`n", [Text.UTF8Encoding]::new($false))
                $env:RUSTFLAGS += ' -C link-arg=-specs=' + $specPath
            }
        }
    }
    & cargo @CargoArguments
    if ($LASTEXITCODE -ne 0) { throw "cargo exited with code $LASTEXITCODE" }
}
finally {
    foreach ($name in $names) {
        if ($null -eq $previous[$name]) { Remove-Item -LiteralPath ('Env:' + $name) -ErrorAction SilentlyContinue }
        else { [Environment]::SetEnvironmentVariable($name,$previous[$name],'Process') }
    }
}
