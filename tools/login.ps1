<#
Open the app on its persistent profile so a person can scan the QR, and leave it running.

Everything measured logged out is only half the story: WhatsApp Web's memory is mostly the
synced mailbox, the frame probe has no chat list to scroll until someone logs in, and a call
cannot be tested at all without an account.

  .\login.ps1

After linking, the measurements that were impossible before:

  .\bench.ps1 -Runs 3 -Real
  .\probe.ps1 -Script webrtc-probe.js -Real
  python .\bench-summary.py .\bench.jsonl

Quit from the tray icon, not Task Manager. Chromium flushes cookies on a clean shutdown;
killing the process is how the WhatsApp login gets lost.
#>
param(
    [string]$Exe = (Join-Path (Split-Path $PSScriptRoot) 'target\release\whatsapp.exe'),
    [int]$DebugPort = 0,
    [int]$InstancePort = 47980
)
$ErrorActionPreference = 'Stop'
if (-not (Test-Path $Exe)) { throw "no build at $Exe - run tools\build.ps1" }

$dataDir = "$env:LOCALAPPDATA\WhatsAppRs"
New-Item -ItemType Directory -Force $dataDir | Out-Null
$env:WHATSAPP_RS_DATA_DIR      = $dataDir
$env:WHATSAPP_RS_INSTANCE_PORT = "$InstancePort"
if ($DebugPort -gt 0) { $env:WHATSAPP_RS_DEBUG_PORT = "$DebugPort" }
else { Remove-Item Env:WHATSAPP_RS_DEBUG_PORT -ErrorAction SilentlyContinue }

$log = "$PSScriptRoot\login.err"
$p = Start-Process -FilePath $Exe -ArgumentList '--safe' -WorkingDirectory (Split-Path $Exe) -PassThru `
        -RedirectStandardOutput "$PSScriptRoot\login.out" -RedirectStandardError $log
"launched as pid $($p.Id)"
"profile  $dataDir"
"log      $log"
""
"Scan the QR with your phone. The linked-device list on the phone should say Chrome."
"When you are done, quit from the tray icon, not Task Manager."
