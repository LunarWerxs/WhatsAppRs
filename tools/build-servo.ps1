param(
    [string]$Repo = 'D:\NEWProjects\WhatsAppRs',
    [string]$ServoRepo = 'D:\NEWProjects\servo',
    [switch]$Check
)
# Builds the app with safe mode on Servo (`--features servo`). Servo's crates need
# the same environment its own `mach` sets up on Windows (SERVO_BUILD.md): LLVM's
# clang-cl and lld-link on PATH, libclang for bindgen, python for mozjs. Then the
# ANGLE DLLs Servo renders through are copied beside the exe, as mach does for
# servoshell. Run it under the fair-CPU wrapper on the shared box.
$ErrorActionPreference = 'Stop'
$llvm = 'C:\Program Files\LLVM\bin'
if (-not (Test-Path "$llvm\clang-cl.exe")) { throw "LLVM not found at $llvm (see SERVO_BUILD.md)" }
$env:PATH = "$llvm;" + $env:PATH
$env:LIBCLANG_PATH = $llvm
$env:CC = 'clang-cl.exe'
$env:CXX = 'clang-cl.exe'
$env:PYTHON3 = 'python'
$env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER = 'lld-link.exe'
if (-not $env:SERVO_STYLE_THREAD_STACK_SIZE_KB) { $env:SERVO_STYLE_THREAD_STACK_SIZE_KB = '8192' }

Set-Location $Repo
$sw = [Diagnostics.Stopwatch]::StartNew()
if ($Check) {
    cargo check --profile servo --features servo --bin whatsapp
} else {
    cargo build --profile servo --features servo --bin whatsapp
}
if ($LASTEXITCODE -ne 0) { throw "cargo failed ($LASTEXITCODE) after $([int]$sw.Elapsed.TotalMinutes) min" }

$out = Join-Path $Repo 'target\servo'
foreach ($dll in 'libEGL.dll', 'libGLESv2.dll') {
    $src = Get-ChildItem -Path (Join-Path $Repo 'target\servo\build') -Recurse -Filter $dll -ErrorAction SilentlyContinue | Select-Object -First 1
    if (-not $src) { $src = Get-Item (Join-Path $ServoRepo "target\release\$dll") -ErrorAction SilentlyContinue }
    if ($src) { Copy-Item $src.FullName (Join-Path $out $dll) -Force; "copied $dll from $($src.FullName)" } else { "WARNING: $dll not found" }
}
"built in $([int]$sw.Elapsed.TotalMinutes) min: $(Join-Path $out 'whatsapp.exe') ($([math]::Round((Get-Item (Join-Path $out 'whatsapp.exe')).Length/1MB)) MB)"
