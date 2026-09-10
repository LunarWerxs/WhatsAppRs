<#
Prove the single executable, the only way that counts: one file in an empty folder.

  .\single-test.ps1

Copies ONLY the packed exe into an empty directory, wipes the profile it will use so the
engine has to be unpacked from scratch, then starts it twice and checks:

  1. the folder really did contain one file and nothing else
  2. it starts at all - which is the delay-load working, because there is no libcef.dll here
  3. the engine folder appears under the profile with every file the bundle had
  4. WhatsApp Web reports itself loaded, through the debugging port
  5. the SECOND start does not unpack again, and is faster

It is deliberately separate from tray-test.ps1 / probe.ps1 / bench.ps1 - those all take -Exe
and should be pointed at the same copied file afterwards. This script only proves the thing
that is new in v0.2.0.
#>
param(
    [string]$Single,
    [string]$Folder = 'D:\wa-single-test',
    [int]$Wait = 90,
    [int]$DebugPort = 47966,
    [int]$InstancePort = 47967
)
$ErrorActionPreference = 'Continue'
. "$PSScriptRoot\pagescript.ps1"

if (-not $Single) {
    # `dist\` at the top of the repo, where single-exe.ps1 has written since 2026-09-09. This
    # used to point at D:\wa-bundle, and after the output moved it silently kept finding the
    # OLD exe there and testing that instead - a green run proving nothing about the build you
    # just made. A stale default that still passes is worse than one that fails.
    $dist = Join-Path (Split-Path $PSScriptRoot) 'dist'
    $Single = (Get-ChildItem $dist -Filter 'whatsapp-rs-*-windows-x64.exe' -File -ErrorAction SilentlyContinue |
               Sort-Object LastWriteTime -Descending | Select-Object -First 1).FullName
    if (-not $Single) { throw "no packed exe in $dist - run tools\single-exe.ps1 first" }
}
if (-not $Single -or -not (Test-Path $Single)) { throw "no packed exe - run tools\single-exe.ps1" }

$dataDir = "$env:LOCALAPPDATA\WhatsAppRs-single"
$env:WHATSAPP_RS_DATA_DIR      = $dataDir
$env:WHATSAPP_RS_INSTANCE_PORT = "$InstancePort"
$env:WHATSAPP_RS_DEBUG_PORT    = "$DebugPort"

Wait-ForExit | Out-Null
Remove-Item -Recurse -Force $Folder, $dataDir -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $Folder, $dataDir | Out-Null

$exe = Join-Path $Folder 'whatsapp.exe'
Copy-Item $Single $exe
"source   : $Single  ($([math]::Round((Get-Item $Single).Length/1MB,1)) MB)"

$contents = Get-ChildItem $Folder -Recurse -Force
"folder   : $($contents.Count) file(s) -> $(if ($contents.Count -eq 1) { 'PASS' } else { "FAIL: $($contents.Name -join ', ')" })"

# Write-Host, not bare strings: inside a PowerShell function every value written to the output
# stream becomes part of the RETURN value, so bare strings here would be captured into $p
# instead of printed - which is exactly what the first version of this script did.
function Start-Once([string]$Tag) {
    $sw = [Diagnostics.Stopwatch]::StartNew()
    $p = Start-Process -FilePath $exe -PassThru -WorkingDirectory $Folder `
            -RedirectStandardOutput "$PSScriptRoot\single-$Tag.out" `
            -RedirectStandardError  "$PSScriptRoot\single-$Tag.err"
    $ready = $null
    $deadline = (Get-Date).AddSeconds($Wait)
    while ((Get-Date) -lt $deadline) {
        Start-Sleep -Seconds 2
        $p.Refresh()
        if ($p.HasExited) {
            Write-Host "$Tag ready : FAIL - exited with $($p.ExitCode)"
            Get-Content "$PSScriptRoot\single-$Tag.err" -Tail 8 -ErrorAction SilentlyContinue | ForEach-Object { Write-Host "  $_" }
            return $null
        }
        $state = Invoke-Python @("$PSScriptRoot\cdp-eval.py", "$DebugPort", "$PSScriptRoot\page-state.js") -TimeoutSeconds 20
        # page-state.js reports "qr" on a logged-out profile and "chats"/"syncing" on a real
        # one. Anything else is still loading, or is the probe failing to reach the page.
        if ($state -match 'qr|chats|syncing') { $ready = $state; break }
    }
    $sw.Stop()
    Write-Host "$Tag start : $([math]::Round($sw.Elapsed.TotalSeconds,1))s to a loaded page -> $(if ($ready) { 'PASS' } else { 'FAIL' })"
    if ($ready) { Write-Host "$Tag page  : $ready" }
    $line = (Get-Content "$PSScriptRoot\single-$Tag.err" -ErrorAction SilentlyContinue |
             Where-Object { $_ -like '*] engine *' } | Select-Object -First 1)
    Write-Host "$Tag engine: $line"
    return $p
}

$p = Start-Once 'first'
if ($p) {
    $engine = Get-ChildItem (Join-Path $dataDir 'engine') -Directory -ErrorAction SilentlyContinue
    foreach ($e in $engine) {
        $files = Get-ChildItem $e.FullName -Recurse -File
        "engine   : $($e.Name)  $($files.Count) files  $([math]::Round((($files | Measure-Object Length -Sum).Sum)/1MB,1)) MB"
    }
    Request-Quit -Exe $exe
    Start-Sleep -Seconds 4
    Wait-ForExit | Out-Null
}

$p = Start-Once 'second'
if ($p) { Request-Quit -Exe $exe; Start-Sleep -Seconds 4 }
Wait-ForExit | Out-Null
