<#
Run one page-side probe against a running engine build and print what it returns.

  .\probe-engine.ps1 -Engine cef     -Script webrtc-probe.js
  .\probe-engine.ps1 -Engine firefox -Script notify-probe.js

Launches the app, waits for the page, runs the script, prints the answer, stops the app.
The two engines need different clients - Chromium speaks CDP, Firefox speaks WebDriver
BiDi since it dropped CDP in 141 - and picking the wrong one fails in a way that reads
like a broken browser, so this chooses it.
#>
param(
    [Parameter(Mandatory)][ValidateSet('cef', 'firefox', 'webview2')][string]$Engine,
    [Parameter(Mandatory)][string]$Script,
    [int]$Wait = 45,
    [string]$Exe,
    [switch]$Real,
    [switch]$Keep,
    [int]$DebugPort = 47962,
    [int]$InstancePort = 47963
)
$ErrorActionPreference = 'Continue'
. "$PSScriptRoot\pagescript.ps1"
if (-not $Exe) {
    $Exe = if ($Engine -eq 'cef') { 'D:\ct\cefapp\whatsapp.exe' } else { 'D:\NEWProjects\WhatsAppRs\target\release\whatsapp.exe' }
}
$dataDir = if ($Real) { "$env:LOCALAPPDATA\WhatsAppRs" } else { "$env:LOCALAPPDATA\WhatsAppRs-probe-$Engine" }
New-Item -ItemType Directory -Force $dataDir | Out-Null
Set-Content -Path (Join-Path $dataDir 'mode.txt') -Value 'safe' -NoNewline
$env:WHATSAPP_RS_DATA_DIR      = $dataDir
$env:WHATSAPP_RS_INSTANCE_PORT = "$InstancePort"
$env:WHATSAPP_RS_DEBUG_PORT    = "$DebugPort"
$env:WHATSAPP_RS_ENGINE        = $Engine

# A previous probe's instance may still hold the single-instance port; launching into that
# makes the new process exit 0 immediately, which reads as a crash that succeeded.
Wait-ForExit -Exe $Exe | Out-Null
$p = Start-Process -FilePath $Exe -ArgumentList '--safe' -WorkingDirectory (Split-Path $Exe) -PassThru `
        -RedirectStandardOutput "$PSScriptRoot\probe-$Engine.out" -RedirectStandardError "$PSScriptRoot\probe-$Engine.err"
Start-Sleep -Seconds $Wait
if ($p.HasExited) { "EXITED $($p.ExitCode)"; Get-Content "$PSScriptRoot\probe-$Engine.err" -Tail 20; exit 2 }

$client = if ($Engine -eq 'firefox') { "$PSScriptRoot\bidi-eval.py" } else { "$PSScriptRoot\cdp-eval.py" }
"engine : $Engine"
"page   : " + (Invoke-Python @($client, "$DebugPort", "$PSScriptRoot\page-state.js"))
"result : " + (Invoke-Python @($client, "$DebugPort", "$PSScriptRoot\$Script"))
"--- app stderr, last 12 ---"
Get-Content "$PSScriptRoot\probe-$Engine.err" -Tail 12 -ErrorAction SilentlyContinue |
    ForEach-Object { $_.Substring(0, [Math]::Min(200, $_.Length)) }

if (-not $Keep) {
    Request-Quit -Exe $Exe
    Start-Sleep -Seconds 3
    # Only our own process. The job object in firefox_view.rs takes the browser with it,
    # so there is no need to hunt Firefox processes here - and hunting them by image path
    # is how an earlier version killed a benchmark that another window was running.
    Get-Process whatsapp -ErrorAction SilentlyContinue |
        Where-Object { $_.Path -eq $Exe } | Stop-Process -Force -ErrorAction SilentlyContinue
}
