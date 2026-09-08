<#
Download the Firefox the app ships and unpack it into `runtime\firefox`.

The Firefox candidate bundles a real Firefox, so it has to come from somewhere reproducible.
It is NOT in git (345 MB of third-party binaries), so this is how a fresh clone gets one.

Two things worth knowing before changing this:

  * **Mozilla publishes no zip for Windows**, only `Firefox Setup X.exe` (NSIS) and an `.msi`.
    The msi is a wrapper: an administrative install (`msiexec /a`) extracts the wrapper and
    not Firefox. The NSIS installer is a 7-Zip self-extracting archive, so `7z x` on it
    yields `core\`, which is the whole browser, with no registry writes and nothing
    installed.
  * **This machine must not end up with Firefox installed.** The point of the exercise is an
    engine we ship, not a browser the owner has to have. Extraction only; never run setup.

  .\get-firefox.ps1                 # latest release
  .\get-firefox.ps1 -Version 155.0.1
#>
param(
    [string]$Version,
    [string]$Dest = (Join-Path (Split-Path $PSScriptRoot) 'runtime\firefox'),
    [string]$SevenZip = 'C:\Program Files\7-Zip\7z.exe'
)
$ErrorActionPreference = 'Stop'
if (-not (Test-Path $SevenZip)) { throw "7-Zip not found at $SevenZip (the installer is a 7z self-extracting archive)" }

if (-not $Version) {
    $Version = (Invoke-RestMethod 'https://product-details.mozilla.org/1.0/firefox_versions.json').LATEST_FIREFOX_VERSION
}
"firefox $Version"

$tmp = Join-Path $env:TEMP "wa-ff-$Version"
New-Item -ItemType Directory -Force $tmp | Out-Null
$setup = Join-Path $tmp 'setup.exe'
$url = "https://ftp.mozilla.org/pub/firefox/releases/$Version/win64/en-US/Firefox%20Setup%20$Version.exe"
if (-not (Test-Path $setup)) {
    "downloading $url"
    Invoke-WebRequest $url -OutFile $setup
}

& $SevenZip x -y -o"$tmp\x" $setup | Out-Null
$core = Join-Path $tmp 'x\core'
if (-not (Test-Path (Join-Path $core 'firefox.exe'))) { throw "no core\firefox.exe in the extracted installer" }

Remove-Item -Recurse -Force $Dest -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force (Split-Path $Dest) | Out-Null
Copy-Item $core $Dest -Recurse

$files = Get-ChildItem $Dest -Recurse -File
"unpacked to $Dest : {0:N1} MB in {1} files" -f (($files | Measure-Object Length -Sum).Sum / 1MB), $files.Count
(& (Join-Path $Dest 'firefox.exe') --version) 2>&1 | ForEach-Object { "  $_" }
