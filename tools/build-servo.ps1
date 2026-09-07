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
# A running instance IS the output file, and Windows will not let cargo replace it.
# Renaming a running exe is allowed, so move it aside and let the build write fresh.
# Stale copies from earlier builds are swept once nothing holds them.
# Both copies: cargo links into deps\ and then places the final one alongside.
foreach ($exe in @((Join-Path $Repo 'target\servo\whatsapp.exe'), (Join-Path $Repo 'target\servo\deps\whatsapp.exe'))) {
    if (-not (Test-Path $exe)) { continue }
    try { [IO.File]::OpenWrite($exe).Close() } catch {
        $aside = [IO.Path]::ChangeExtension($exe, $null) + "inuse-" + (Get-Random) + ".exe"
        Rename-Item $exe $aside
        "moved a running binary aside: $(Split-Path $aside -Leaf)"
    }
}
Get-ChildItem (Join-Path $Repo 'target\servo') -Recurse -Filter 'whatsapp*inuse-*.exe' -ErrorAction SilentlyContinue | ForEach-Object {
    try { Remove-Item $_.FullName -Force -ErrorAction Stop } catch {}
}
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
    $dest = Join-Path $out $dll
    if (-not $src) { "WARNING: $dll not found"; continue }
    # A running instance holds these open. They never change between builds, so an
    # existing copy is as good as a fresh one; only a MISSING one is a failure.
    try { Copy-Item $src.FullName $dest -Force; "copied $dll from $($src.FullName)" }
    catch {
        if (Test-Path $dest) { "kept existing $dll (locked by a running instance)" }
        else { throw }
    }
}
"built in $([int]$sw.Elapsed.TotalMinutes) min: $(Join-Path $out 'whatsapp.exe') ($([math]::Round((Get-Item (Join-Path $out 'whatsapp.exe')).Length/1MB)) MB)"
