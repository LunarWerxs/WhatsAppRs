<#
The end-to-end toast proof for a bundled-engine build.

Two sources, and they have to agree:

  * `notify-probe.js` - what the PAGE thinks happened.
  * `toast-db.py`     - what WINDOWS actually accepted, from its own notification database.

The second is the ground truth and the first is not. On the WebView2 round the page reported
"displayed" for every notification while Windows recorded none and nothing appeared on
screen, and that report was the entire basis of a "notifications work" claim that was false.

  .\notify-test.ps1 -Engine cef
  .\notify-test.ps1 -Engine cef -NoBridge     # the same run with our shim turned off,
                                              # to measure what the engine does by itself
#>
param(
    [Parameter(Mandatory)][ValidateSet('cef', 'firefox', 'webview2')][string]$Engine,
    [int]$Wait = 45,
    [switch]$NoBridge,
    [string]$Exe,
    [int]$DebugPort = 47966,
    [int]$InstancePort = 47967
)
$ErrorActionPreference = 'Continue'
. "$PSScriptRoot\pagescript.ps1"
Add-Type -AssemblyName System.Drawing, System.Windows.Forms
if (-not $Exe) {
    $Exe = if ($Engine -eq 'cef') { 'D:\ct\cefapp\whatsapp.exe' } else { 'D:\NEWProjects\WhatsAppRs\target\release\whatsapp.exe' }
}
$dataDir = "$env:LOCALAPPDATA\WhatsAppRs-notify-$Engine"
New-Item -ItemType Directory -Force $dataDir | Out-Null
Set-Content -Path (Join-Path $dataDir 'mode.txt') -Value 'safe' -NoNewline
$env:WHATSAPP_RS_DATA_DIR      = $dataDir
$env:WHATSAPP_RS_INSTANCE_PORT = "$InstancePort"
$env:WHATSAPP_RS_DEBUG_PORT    = "$DebugPort"
$env:WHATSAPP_RS_ENGINE        = $Engine
if ($NoBridge) { $env:WHATSAPP_RS_NOTIFY_BRIDGE = 'off' } else { Remove-Item Env:WHATSAPP_RS_NOTIFY_BRIDGE -ErrorAction SilentlyContinue }

# The Start Menu shortcut carrying the AppUserModelID is what makes Windows willing to draw
# a toast for this app at all. The app writes it at startup; check it is really there.
$lnk = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\WhatsApp Rs.lnk'
"shortcut : " + $(if (Test-Path $lnk) { "present ($lnk)" } else { 'MISSING - no toast will ever render' })

$before = Invoke-Python @("$PSScriptRoot\toast-db.py", "3")

# A previous run's instance may still hold the single-instance port; launching into that
# makes the new process exit 0 immediately, which reads as a crash that succeeded.
Wait-ForExit -Exe $Exe | Out-Null
$p = Start-Process -FilePath $Exe -ArgumentList '--safe' -WorkingDirectory (Split-Path $Exe) -PassThru `
        -RedirectStandardOutput "$PSScriptRoot\notify-$Engine.out" -RedirectStandardError "$PSScriptRoot\notify-$Engine.err"
Start-Sleep -Seconds $Wait
if ($p.HasExited) { "EXITED $($p.ExitCode)"; Get-Content "$PSScriptRoot\notify-$Engine.err" -Tail 20; exit 2 }

$client = if ($Engine -eq 'firefox') { "$PSScriptRoot\bidi-eval.py" } else { "$PSScriptRoot\cdp-eval.py" }
"bridge   : " + $(if ($NoBridge) { 'OFF (measuring the engine on its own)' } else { 'on' })
"page says: " + (Invoke-Python @($client, "$DebugPort", "$PSScriptRoot\notify-probe.js"))

Start-Sleep -Seconds 4
# Screenshot the bottom-right, where Windows draws toasts.
$b = [Drawing.Bitmap]::new(700, 400)
$s = [Windows.Forms.Screen]::PrimaryScreen.Bounds
[Drawing.Graphics]::FromImage($b).CopyFromScreen($s.Width - 700, $s.Height - 460, 0, 0, $b.Size)
$shot = "$PSScriptRoot\notify-$Engine$(if ($NoBridge) { '-nobridge' }).png"
$b.Save($shot)
"screenshot: $shot"

"windows says (its own database, newest first):"
(Invoke-Python @("$PSScriptRoot\toast-db.py", "5")) -split "`n" |
    Select-Object -First 14 | ForEach-Object { "  " + $_.TrimEnd() }

"--- app stderr, last 15 ---"
Get-Content "$PSScriptRoot\notify-$Engine.err" -Tail 15 -ErrorAction SilentlyContinue |
    ForEach-Object { $_.Substring(0, [Math]::Min(200, $_.Length)) }

Request-Quit -Exe $Exe
Start-Sleep -Seconds 3
Get-Process whatsapp -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $Exe } | Stop-Process -Force -ErrorAction SilentlyContinue
