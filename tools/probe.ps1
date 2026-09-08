<#
Run one page-side probe against the app and print what it returns.

  .\probe.ps1 -Script capability-probe.js     # is anything missing that WhatsApp needs
  .\probe.ps1 -Script webrtc-probe.js         # can a call happen, and on which codecs
  .\probe.ps1 -Script gfx-probe.js            # is it really using the GPU
  .\probe.ps1 -Script shim-check.js           # did the notification shim reach the page
  .\probe.ps1 -Script page-state.js -Real     # against the logged-in profile

Launches the app with a debugging port, waits, evaluates the script in the page, prints the
answer, stops the app.
#>
param(
    [Parameter(Mandatory)][string]$Script,
    [int]$Wait = 40,
    [string]$Exe = (Join-Path (Split-Path $PSScriptRoot) 'target\release\whatsapp.exe'),
    [switch]$Real,
    [switch]$Keep,
    [string]$Switches = '',
    [int]$DebugPort = 47962,
    [int]$InstancePort = 47963
)
$ErrorActionPreference = 'Continue'
. "$PSScriptRoot\pagescript.ps1"
if (-not (Test-Path $Exe)) { throw "no build at $Exe - run tools\build.ps1" }

$dataDir = if ($Real) { "$env:LOCALAPPDATA\WhatsAppRs" } else { "$env:LOCALAPPDATA\WhatsAppRs-probe" }
New-Item -ItemType Directory -Force $dataDir | Out-Null
$env:WHATSAPP_RS_DATA_DIR      = $dataDir
$env:WHATSAPP_RS_INSTANCE_PORT = "$InstancePort"
$env:WHATSAPP_RS_DEBUG_PORT    = "$DebugPort"
if ($Switches) { $env:WHATSAPP_RS_CEF_SWITCHES = $Switches } else { Remove-Item Env:WHATSAPP_RS_CEF_SWITCHES -ErrorAction SilentlyContinue }

# A previous probe's instance may still hold the single-instance port; launching into that
# makes the new process exit 0 immediately, which reads as a crash that succeeded.
Wait-ForExit | Out-Null
$p = Start-Process -FilePath $Exe -ArgumentList '--safe' -WorkingDirectory (Split-Path $Exe) -PassThru `
        -RedirectStandardOutput "$PSScriptRoot\probe.out" -RedirectStandardError "$PSScriptRoot\probe.err"
Start-Sleep -Seconds $Wait
if ($p.HasExited) { "EXITED $($p.ExitCode)"; Get-Content "$PSScriptRoot\probe.err" -Tail 20; exit 2 }

"page   : " + (Invoke-Python @("$PSScriptRoot\cdp-eval.py", "$DebugPort", "$PSScriptRoot\page-state.js"))
"result : " + (Invoke-Python @("$PSScriptRoot\cdp-eval.py", "$DebugPort", "$PSScriptRoot\$Script"))
"--- app stderr, last 12 ---"
Get-Content "$PSScriptRoot\probe.err" -Tail 12 -ErrorAction SilentlyContinue |
    ForEach-Object { $_.Substring(0, [Math]::Min(200, $_.Length)) }

if (-not $Keep) {
    Request-Quit -Exe $Exe
    Start-Sleep -Seconds 3
    Get-Process whatsapp -ErrorAction SilentlyContinue |
        Where-Object { $_.Path -eq $Exe } | Stop-Process -Force -ErrorAction SilentlyContinue
}
