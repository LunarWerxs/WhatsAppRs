<#
Open one engine build on its own PERSISTENT profile so a person can scan the QR, and leave
it running. Everything measured logged out is only half the story: WhatsApp Web's memory is
mostly the synced mailbox, and a call cannot be tested at all without an account.

  .\login.ps1 -Engine cef        # scan the QR, then leave it or quit from the tray
  .\login.ps1 -Engine firefox

Each engine gets its OWN profile (`WhatsAppRs-live-<engine>`), so linking one does not
disturb the other, and neither touches `WhatsAppRs`, which belongs to the Servo build and
holds a live session. Linking a device does not unlink any other: WhatsApp allows several.

After linking, the head-to-head against the real account is:

  .\engine-bench.ps1 -Engine cef     -Runs 3 -Real
  .\engine-bench.ps1 -Engine firefox -Runs 3 -Real

Quit from the tray icon, not Task Manager. Both engines flush their session storage on a
clean shutdown; killing the process is how a WhatsApp login gets lost.
#>
param(
    [Parameter(Mandatory)][ValidateSet('cef', 'firefox', 'webview2')][string]$Engine,
    [string]$Exe,
    [int]$DebugPort = 0,
    [int]$InstancePort = 47980
)
$ErrorActionPreference = 'Stop'
if (-not $Exe) {
    $Exe = if ($Engine -eq 'cef') { 'D:\ct\cefapp\whatsapp.exe' } else { 'D:\NEWProjects\WhatsAppRs\target\release\whatsapp.exe' }
}
if (-not (Test-Path $Exe)) { throw "no executable at $Exe" }

$dataDir = "$env:LOCALAPPDATA\WhatsAppRs-live-$Engine"
New-Item -ItemType Directory -Force $dataDir | Out-Null
Set-Content -Path (Join-Path $dataDir 'mode.txt') -Value 'safe' -NoNewline
$env:WHATSAPP_RS_DATA_DIR      = $dataDir
$env:WHATSAPP_RS_INSTANCE_PORT = "$InstancePort"
$env:WHATSAPP_RS_ENGINE        = $Engine
if ($DebugPort -gt 0) { $env:WHATSAPP_RS_DEBUG_PORT = "$DebugPort" }
else { Remove-Item Env:WHATSAPP_RS_DEBUG_PORT -ErrorAction SilentlyContinue }

$log = "$PSScriptRoot\login-$Engine.err"
$p = Start-Process -FilePath $Exe -ArgumentList '--safe' -WorkingDirectory (Split-Path $Exe) -PassThru `
        -RedirectStandardOutput "$PSScriptRoot\login-$Engine.out" -RedirectStandardError $log
"launched $Engine as pid $($p.Id)"
"profile  $dataDir"
"log      $log"
""
"Scan the QR with your phone. The phone's linked-device list should say " +
    $(if ($Engine -eq 'cef') { '"Chrome"' } else { '"Firefox"' }) + "."
"When you are done, quit from the tray icon (not Task Manager)."
