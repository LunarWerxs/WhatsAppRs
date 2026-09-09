<#
Assemble the shippable folder and report what it actually costs on disk.

The build directory is not the answer to "how big is this": it holds .pdb files, import
libraries, headers, every locale CEF ships and a 20 MB CREDITS.html. This copies only what the
app needs to run - then RUN IT from there, which is the only way to know the trim is honest
rather than a list of files someone guessed were unused.

  .\bundle.ps1

Verified 2026-09-07: the trimmed bundle launches and renders WhatsApp Web.

⛔ **The software-rendering fallback is no longer optional, and `-KeepFallbacks` is gone**
(2026-09-09). It used to be opt-in, worth ~37 MB, on the reasoning that this machine has a
GPU. That reasoning died with the runaway: the app now ships an out-of-process GPU precisely
so Chromium can count context-loss failures and FALL BACK TO SOFTWARE after about three - and
what it falls back to is `vk_swiftshader.dll`. Drop those files and the fallback stack is
empty, at which point Chromium stops the browser process rather than loop. Shipping the
recovery path without the thing it recovers ONTO is not a saving. See DECISIONS.md #24.

This script's default `-Out` is also the folder the owner's own install runs from, so a run
without the fallbacks would have quietly disarmed a live app.
#>
param(
    [string]$Out = 'D:\wa-bundle\whatsapp',
    [string]$From = (Join-Path (Split-Path $PSScriptRoot) 'target\release')
)
$ErrorActionPreference = 'Stop'
if (-not (Test-Path (Join-Path $From 'whatsapp.exe'))) { throw "no build at $From - run tools\build.ps1 first" }

Remove-Item -Recurse -Force $Out -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $Out, "$Out\locales" | Out-Null

# Everything CEF's own documentation lists as required for a Windows distribution, and nothing
# else. locales\ is 51 MB for 220 languages; the app asks for en-US.
$required = @(
    'whatsapp.exe', 'libcef.dll', 'chrome_elf.dll', 'libEGL.dll', 'libGLESv2.dll',
    'resources.pak', 'chrome_100_percent.pak', 'chrome_200_percent.pak',
    'icudtl.dat', 'v8_context_snapshot.bin'
)
# Software rendering and the DirectX shader compiler. NOT optional: this is where Chromium
# goes when the GPU process has died three times, which is the whole recovery the 2026-09-09
# fix restored. `-ErrorAction Stop` like the rest, because a missing one is now a broken
# build rather than a smaller one.
$fallbacks = @('d3dcompiler_47.dll', 'dxcompiler.dll', 'dxil.dll',
               'vk_swiftshader.dll', 'vk_swiftshader_icd.json', 'vulkan-1.dll')

foreach ($f in $required + $fallbacks) { Copy-Item (Join-Path $From $f) $Out -ErrorAction Stop }
Copy-Item (Join-Path $From 'locales\en-US.pak') "$Out\locales\"

$files = Get-ChildItem $Out -Recurse -File
"{0,8:N1} MB in {1} files   (software-rendering fallback included, and required)" -f
    (($files | Measure-Object Length -Sum).Sum / 1MB), $files.Count
$dropped = (Get-ChildItem (Join-Path $From 'locales') -File -ErrorAction SilentlyContinue).Count - 1
"  dropped: $dropped other locales, CREDITS.html, libcef.lib, include\, libcef_dll\, *.pdb, bootstrap*.exe"
"bundle at $Out"
