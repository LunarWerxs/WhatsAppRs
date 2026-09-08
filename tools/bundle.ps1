<#
Assemble the shippable folder for one engine and report what it actually costs on disk.

The build directory is not the answer to "how big is this": it holds .pdb files, import
libraries, headers, every locale and a 20 MB CREDITS.html. This copies only what the app
needs to run, then you RUN IT from there, which is the only way to know the trim is honest
rather than a list of files someone guessed were unused.

  .\bundle.ps1 -Engine cef      -Out D:\wa-bundle\cef
  .\bundle.ps1 -Engine firefox  -Out D:\wa-bundle\firefox
#>
param(
    [Parameter(Mandatory)][ValidateSet('cef', 'firefox')][string]$Engine,
    [string]$Out,
    # Keep the software-rendering and DirectX-shader-compiler fallbacks. Dropping them is
    # ~33 MB and costs the app its ability to render on a machine with no working GPU
    # driver, which is a real machine, just not this one.
    [switch]$KeepFallbacks
)
$ErrorActionPreference = 'Stop'
if (-not $Out) { $Out = "D:\wa-bundle\$Engine" }
Remove-Item -Recurse -Force $Out -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $Out | Out-Null

function Report($path, $note) {
    $files = Get-ChildItem $path -Recurse -File
    "{0,-10} {1,8:N1} MB in {2,5} files   {3}" -f $Engine, (($files | Measure-Object Length -Sum).Sum / 1MB), $files.Count, $note
}

if ($Engine -eq 'cef') {
    $src = 'D:\ct\cefapp'
    # Everything CEF's own documentation lists as required for a Windows distribution,
    # and nothing else. locales/ is 51 MB for 220 languages; the app asks for en-US.
    $required = @(
        'whatsapp.exe', 'libcef.dll', 'chrome_elf.dll', 'libEGL.dll', 'libGLESv2.dll',
        'resources.pak', 'chrome_100_percent.pak', 'chrome_200_percent.pak',
        'icudtl.dat', 'v8_context_snapshot.bin'
    )
    $fallbacks = @('d3dcompiler_47.dll', 'dxcompiler.dll', 'dxil.dll', 'vk_swiftshader.dll', 'vk_swiftshader_icd.json', 'vulkan-1.dll')
    foreach ($f in $required) { Copy-Item (Join-Path $src $f) $Out -ErrorAction Stop }
    if ($KeepFallbacks) { foreach ($f in $fallbacks) { Copy-Item (Join-Path $src $f) $Out -ErrorAction SilentlyContinue } }
    New-Item -ItemType Directory -Force "$Out\locales" | Out-Null
    Copy-Item "$src\locales\en-US.pak" "$Out\locales\"
    Report $Out ("fallbacks " + $(if ($KeepFallbacks) { 'kept' } else { 'dropped' }))
    "  dropped: $((Get-ChildItem "$src\locales" -File).Count - 1) other locales, CREDITS.html, libcef.lib, include\, libcef_dll\, *.pdb, bootstrap*.exe"
} else {
    $src = 'D:\NEWProjects\WhatsAppRs\runtime\firefox'
    Copy-Item 'D:\NEWProjects\WhatsAppRs\target\release\whatsapp.exe' $Out
    New-Item -ItemType Directory -Force "$Out\firefox" | Out-Null
    Copy-Item "$src\*" "$Out\firefox" -Recurse
    # A Firefox we ship and drive is never going to update itself, report a crash to
    # Mozilla, install a maintenance service, or be set as the default browser.
    $drop = @(
        'maintenanceservice.exe', 'maintenanceservice_installer.exe', 'crashreporter.exe',
        'crashhelper.exe', 'default-browser-agent.exe', 'pingsender.exe', 'updater.exe',
        'updater.ini', 'update-settings.ini', 'uninstall', 'gmp-clearkey', 'minidump-analyzer.exe'
    )
    foreach ($d in $drop) { Remove-Item -Recurse -Force (Join-Path "$Out\firefox" $d) -ErrorAction SilentlyContinue }
    Report $Out "removed updater, crash reporter, maintenance service, default-browser agent"
}
"bundle at $Out"
