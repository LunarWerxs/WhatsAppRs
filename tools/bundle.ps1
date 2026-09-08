<#
Assemble the shippable folder and report what it actually costs on disk.

The build directory is not the answer to "how big is this": it holds .pdb files, import
libraries, headers, every locale CEF ships and a 20 MB CREDITS.html. This copies only what the
app needs to run - then RUN IT from there, which is the only way to know the trim is honest
rather than a list of files someone guessed were unused.

  .\bundle.ps1
  .\bundle.ps1 -KeepFallbacks     # +37 MB, keeps software rendering for a machine with no GPU

Verified 2026-09-07: the trimmed bundle launches and renders WhatsApp Web.
#>
param(
    [string]$Out = 'D:\wa-bundle\whatsapp',
    [string]$From = (Join-Path (Split-Path $PSScriptRoot) 'target\release'),
    # Keep the software-rendering and DirectX-shader-compiler fallbacks. Dropping them is
    # ~37 MB and costs the app its ability to render on a machine with no working GPU driver,
    # which is a real machine, just not this one.
    [switch]$KeepFallbacks
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
$fallbacks = @('d3dcompiler_47.dll', 'dxcompiler.dll', 'dxil.dll',
               'vk_swiftshader.dll', 'vk_swiftshader_icd.json', 'vulkan-1.dll')

foreach ($f in $required) { Copy-Item (Join-Path $From $f) $Out -ErrorAction Stop }
if ($KeepFallbacks) { foreach ($f in $fallbacks) { Copy-Item (Join-Path $From $f) $Out -ErrorAction SilentlyContinue } }
Copy-Item (Join-Path $From 'locales\en-US.pak') "$Out\locales\"

$files = Get-ChildItem $Out -Recurse -File
"{0,8:N1} MB in {1} files   fallbacks {2}" -f (($files | Measure-Object Length -Sum).Sum / 1MB),
    $files.Count, $(if ($KeepFallbacks) { 'kept' } else { 'dropped' })
$dropped = (Get-ChildItem (Join-Path $From 'locales') -File -ErrorAction SilentlyContinue).Count - 1
"  dropped: $dropped other locales, CREDITS.html, libcef.lib, include\, libcef_dll\, *.pdb, bootstrap*.exe"
"bundle at $Out"
