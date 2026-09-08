<#
Build the app.

There is one engine now, so this is `cargo build --release` plus the environment the `cef`
crate's build script needs: cmake, ninja and MSVC, because it compiles CEF's C++ wrapper.
The first build also downloads the 171 MB CEF binary distribution into CEF_PATH; later builds
reuse it.

  .\build.ps1              # release build -> target\release\whatsapp.exe
  .\build.ps1 -Check       # cargo check only, no link

**The MAX_PATH trap, kept written down because it reads like a broken toolchain.** cmake's
compiler probe writes
`<target>\build\cef-dll-sys-<hash>\out\build\CMakeFiles\CMakeScratch\TryCompile-xxxxxx\CMakeFiles\cmTC_xxxxx.dir\intermediate.manifest`.
From this repo's own `target\` that is about 175 characters and fine; from a deep temp
directory it passed 290 and `link.exe` failed with `LNK1104: cannot open file ...
intermediate.manifest`. If you ever move the build somewhere deeper, set CARGO_TARGET_DIR to
something short instead of debugging the compiler.
#>
param(
    [switch]$Check,
    [string]$TargetDir,
    [string]$CefPath = "$env:USERPROFILE\.local\share\cef"
)
$ErrorActionPreference = 'Stop'

$vs = 'C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools'
if (-not (Test-Path $vs)) { $vs = 'C:\Program Files\Microsoft Visual Studio\2022\BuildTools' }
$cmake = Join-Path $vs 'Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin'
$ninja = Join-Path $vs 'Common7\IDE\CommonExtensions\Microsoft\CMake\Ninja'
foreach ($p in @($cmake, $ninja)) {
    if (-not (Test-Path $p)) {
        throw "missing $p - install 'C++ CMake tools for Windows' in the Visual Studio Build Tools installer"
    }
}

$env:CEF_PATH = $CefPath
$env:PATH = "$cmake;$ninja;$env:PATH"
if ($TargetDir) { $env:CARGO_TARGET_DIR = $TargetDir }

Set-Location (Split-Path $PSScriptRoot)
if ($Check) { cargo check --release } else { cargo build --release }
$code = $LASTEXITCODE
if ($code -ne 0) { exit $code }

if (-not $Check) {
    $out = if ($TargetDir) { Join-Path $TargetDir 'release' } else { Join-Path (Split-Path $PSScriptRoot) 'target\release' }
    $exe = Join-Path $out 'whatsapp.exe'
    "built  : $exe  ($([math]::Round((Get-Item $exe).Length/1MB,1)) MB)"
    $dlls = Get-ChildItem $out -File -Filter '*.dll' -ErrorAction SilentlyContinue
    "engine : $([math]::Round((($dlls | Measure-Object Length -Sum).Sum)/1MB,1)) MB of DLLs beside it, copied there by the cef crate"
    "        run tools\bundle.ps1 to assemble the trimmed shippable folder"
}
