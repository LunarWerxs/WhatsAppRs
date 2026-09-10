<#
Produce the one file this repo ships: whatsapp.exe with the whole engine inside it.

  .\build.ps1
  .\single-exe.ps1                      # -> D:\wa-bundle\whatsapp-rs-<version>-windows-x64.exe
  .\single-exe.ps1 -Codec xz:9          # if the download/first-run trade ever changes

Three steps, and `bundle.ps1` is still the one that decides what the engine consists of:

  1. bundle.ps1                  assembles the 17-file folder, exactly as v0.1.0 shipped it,
                                 software-rendering fallback included - since 2026-09-09 that
                                 is not optional, because it is what Chromium falls back TO
  2. pack-payload.py             compresses everything except whatsapp.exe into one stream
  3.                             writes whatsapp.exe, then the stream, then a 64-byte footer

Windows ignores bytes after the end of a PE image, so the result is a normal executable that
happens to be 129 MB. `src/engine.rs` reads the footer out of its own file on startup.

`& python` is deliberately not used: these scripts also run detached with no console, where
PowerShell's native-command path fails intermittently with "No process is on the other end of
the pipe". Start-Process with redirected files never touches a console.
#>
param(
    [string]$Bundle = (Join-Path (Split-Path $PSScriptRoot) 'dist\bundle'),
    [string]$Out,
    [string]$Codec = 'zstd:22',
    [switch]$SkipBundle,
    [switch]$SkipTests
)
$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot

$version = (Select-String -Path (Join-Path $repo 'Cargo.toml') -Pattern '^version = "(.+)"' |
            Select-Object -First 1).Matches[0].Groups[1].Value
# THE PRODUCT lands in `dist\` at the top of the repo, beside the bundle it was made from, so
# that "where is the file a person downloads" has an obvious answer inside the project itself.
if (-not $Out) { $Out = Join-Path (Split-Path $Bundle) "whatsapp-rs-$version-windows-x64.exe" }

# The unit tests gate the RELEASE, because this repository has no CI and a guard nobody runs is
# not a guard. `in_process_gpu_is_never_shipped` is the one that matters: it fails if
# `--in-process-gpu` or `--single-process` is back in the shipped switch list, which is the
# defect that cost 244 GB of log, 10.9 GB of RAM and 8.3 CPU-hours on 2026-09-09. It costs a
# couple of seconds, and this is the last moment anybody can catch it before strangers get it.
if (-not $SkipTests) {
    & (Join-Path $PSScriptRoot 'build.ps1') -Test
    if ($LASTEXITCODE -ne 0) { throw 'unit tests failed - refusing to pack a release' }
}

if (-not $SkipBundle) {
    & (Join-Path $PSScriptRoot 'bundle.ps1') -Out $Bundle
}
$stub = Join-Path $Bundle 'whatsapp.exe'
if (-not (Test-Path $stub)) { throw "no bundle at $Bundle" }

# Refuse to pack a stale build. `build.ps1` exits non-zero when cargo fails, but `exit` inside a
# called script only ends that script - so a failed build followed by this one would happily ship
# the previous version's exe with the new version's engine. It happened once: cargo could not
# overwrite whatsapp.exe because five of its processes were still running, and the pack carried on.
$stubVersion = (Get-Item $stub).VersionInfo.FileVersion
if ($stubVersion -and -not $stubVersion.StartsWith($version)) {
    throw "the built exe reports version $stubVersion but Cargo.toml says $version - run tools\build.ps1 and check it succeeded"
}

Remove-Item $Out -ErrorAction SilentlyContinue
$o = Join-Path $env:TEMP "wa-pack-$PID.out"
$e = Join-Path $env:TEMP "wa-pack-$PID.err"
$proc = Start-Process -FilePath 'python' -WindowStyle Hidden -PassThru `
    -ArgumentList @((Join-Path $PSScriptRoot 'pack-payload.py'), $Bundle, '--out', $Out, '--codec', $Codec) `
    -RedirectStandardOutput $o -RedirectStandardError $e
$proc.WaitForExit()
Get-Content $o -ErrorAction SilentlyContinue
if ($proc.ExitCode -ne 0) {
    Get-Content $e -Tail 20 -ErrorAction SilentlyContinue
    Remove-Item $o, $e -ErrorAction SilentlyContinue
    throw "pack-payload.py failed with $($proc.ExitCode)"
}
Remove-Item $o, $e -ErrorAction SilentlyContinue

# Read the artifact back before it is allowed to be a release. Same file, opposite direction:
# parse the footer, decompress, walk the index and check every CRC - so a file that would not
# unpack cannot leave this script looking finished.
$o = Join-Path $env:TEMP "wa-verify-$PID.out"
$e = Join-Path $env:TEMP "wa-verify-$PID.err"
$proc = Start-Process -FilePath 'python' -WindowStyle Hidden -PassThru `
    -ArgumentList @((Join-Path $PSScriptRoot 'pack-payload.py'), '--verify', $Out) `
    -RedirectStandardOutput $o -RedirectStandardError $e
$proc.WaitForExit()
Get-Content $o -ErrorAction SilentlyContinue | Select-Object -Last 1
if ($proc.ExitCode -ne 0) {
    Get-Content $o, $e -Tail 20 -ErrorAction SilentlyContinue
    Remove-Item $o, $e -ErrorAction SilentlyContinue
    throw "the packed exe did not verify - do not ship it"
}
Remove-Item $o, $e -ErrorAction SilentlyContinue

$folder = (Get-ChildItem $Bundle -Recurse -File | Measure-Object Length -Sum).Sum
"single : {0:N1} MB, from a {1:N0} MB folder of {2} files" -f ((Get-Item $Out).Length / 1MB),
    ($folder / 1MB), (Get-ChildItem $Bundle -Recurse -File).Count
"sha256 : $((Get-FileHash $Out -Algorithm SHA256).Hash.ToLower())"
"        prove it: tools\single-test.ps1 - copies ONLY this file into an empty folder"
