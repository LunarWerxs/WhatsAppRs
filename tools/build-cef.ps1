<#
Build safe mode on a bundled Chromium (`--features cef`, `--profile cefapp`).

The `cef` crate downloads the 171 MB CEF binary distribution on first build and then
compiles `libcef_dll_wrapper` with cmake + ninja + MSVC, so this sets up that environment
and gets the target directory out of the way of two real traps:

  * **MAX_PATH.** cmake's compiler probe writes
    `.../out/build/CMakeFiles/CMakeScratch/TryCompile-xxxxxx/CMakeFiles/cmTC_xxxxx.dir/intermediate.manifest`
    under the target dir. Under a normal Windows temp path that is ~290 characters and
    `link.exe` fails with `LNK1104: cannot open file ... intermediate.manifest`, which
    reads like a broken toolchain and is not. `D:\ct` is short on purpose.
  * **The target lock.** A `cargo build` in the repo's own `target/` and this one would
    serialise against each other, and this one drags a 272 MB DLL around.

  .\build-cef.ps1            # build
  .\build-cef.ps1 -Check     # cargo check only, no link

Output: `D:\ct\cefapp\whatsapp.exe`, with the CEF runtime files copied beside it by the
crate's own build script. `tools\bundle.ps1 -Engine cef` turns that into a shippable folder.
#>
param(
    [switch]$Check,
    [string]$TargetDir = 'D:\ct',
    [string]$CefPath = "$env:USERPROFILE\.local\share\cef"
)
$ErrorActionPreference = 'Stop'

$vs = 'C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools'
if (-not (Test-Path $vs)) { $vs = 'C:\Program Files\Microsoft Visual Studio\2022\BuildTools' }
$cmake = Join-Path $vs 'Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin'
$ninja = Join-Path $vs 'Common7\IDE\CommonExtensions\Microsoft\CMake\Ninja'
foreach ($p in @($cmake, $ninja)) {
    if (-not (Test-Path $p)) { throw "missing $p - install the C++ CMake tools for Windows in the VS Build Tools installer" }
}

$env:CEF_PATH = $CefPath
$env:CARGO_TARGET_DIR = $TargetDir
$env:PATH = "$cmake;$ninja;$env:PATH"

Set-Location (Split-Path $PSScriptRoot)
if ($Check) { cargo check --features cef --profile cefapp } else { cargo build --features cef --profile cefapp }
$code = $LASTEXITCODE
if ($code -eq 0 -and -not $Check) {
    $exe = Join-Path $TargetDir 'cefapp\whatsapp.exe'
    "built $exe"
    "runtime beside it: " + ((Get-ChildItem (Split-Path $exe) -File -Filter '*.dll' | Measure-Object Length -Sum).Sum / 1MB).ToString('N1') + " MB of DLLs"
}
exit $code
