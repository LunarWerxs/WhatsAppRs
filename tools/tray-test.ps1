<#
Close-to-tray, restore, and clean quit, on either bundled-engine build.

These are the behaviours the whole design exists for, and both new engines put a foreign
window inside ours, so neither inherits them for free:

  * the Chromium build has CEF's child window in ours,
  * the Firefox build has another PROCESS's window in ours.

Hiding a parent hides its children on Windows, so close-to-tray should just work - but
"should" is how the last three silent failures in this project started, so it is measured.

  .\tray-test.ps1 -Engine cef
  .\tray-test.ps1 -Engine firefox
#>
param(
    [Parameter(Mandatory)][ValidateSet('cef', 'firefox', 'webview2')][string]$Engine,
    [int]$Wait = 40,
    [string]$Exe,
    [int]$InstancePort = 47971
)
$ErrorActionPreference = 'Continue'
. "$PSScriptRoot\pagescript.ps1"
Add-Type -AssemblyName System.Windows.Forms
Add-Type @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class TrayT {
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr l);
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowTextW(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern IntPtr SendMessageTimeout(IntPtr h, uint msg, IntPtr w, IntPtr l, uint flags, uint ms, out IntPtr res);
  public const uint WM_CLOSE = 0x0010;
  public static IntPtr Top(uint pid) {
    IntPtr found = IntPtr.Zero;
    EnumWindows(delegate(IntPtr h, IntPtr l) {
      uint p; GetWindowThreadProcessId(h, out p);
      if (p != pid || !IsWindowVisible(h)) return true;
      var t = new StringBuilder(256); GetWindowTextW(h, t, 256);
      if (t.ToString() != "WhatsApp") return true;
      found = h; return false;
    }, IntPtr.Zero);
    return found;
  }
  public static bool Visible(IntPtr h) { return IsWindowVisible(h); }
  public static void Close(IntPtr h) { IntPtr r; SendMessageTimeout(h, WM_CLOSE, IntPtr.Zero, IntPtr.Zero, 2, 3000, out r); }
}
'@

if (-not $Exe) {
    $Exe = if ($Engine -eq 'cef') { 'D:\ct\cefapp\whatsapp.exe' } else { 'D:\NEWProjects\WhatsAppRs\target\release\whatsapp.exe' }
}
$dataDir = "$env:LOCALAPPDATA\WhatsAppRs-tray-$Engine"
New-Item -ItemType Directory -Force $dataDir | Out-Null
Set-Content -Path (Join-Path $dataDir 'mode.txt') -Value 'safe' -NoNewline
$env:WHATSAPP_RS_DATA_DIR      = $dataDir
$env:WHATSAPP_RS_INSTANCE_PORT = "$InstancePort"
$env:WHATSAPP_RS_ENGINE        = $Engine
Remove-Item Env:WHATSAPP_RS_DEBUG_PORT -ErrorAction SilentlyContinue

Wait-ForExit | Out-Null
$p = Start-Process -FilePath $Exe -ArgumentList '--safe' -WorkingDirectory (Split-Path $Exe) -PassThru `
        -RedirectStandardOutput "$PSScriptRoot\tray-$Engine.out" -RedirectStandardError "$PSScriptRoot\tray-$Engine.err"
Start-Sleep -Seconds $Wait
if ($p.HasExited) { "FAIL: exited early with $($p.ExitCode)"; exit 2 }

$hwnd = [TrayT]::Top([uint32]$p.Id)
if ($hwnd -eq [IntPtr]::Zero) { "FAIL: no window titled 'WhatsApp' for pid $($p.Id)"; exit 2 }
"window   : $hwnd visible=$([TrayT]::Visible($hwnd))"

# 1. Close to tray: the window goes, the process stays.
[TrayT]::Close($hwnd)
Start-Sleep -Seconds 3
$p.Refresh()
$survived = -not $p.HasExited
$hidden = -not [TrayT]::Visible($hwnd)
"close    : process survived=$survived  window hidden=$hidden  -> $(if ($survived -and $hidden) { 'PASS' } else { 'FAIL' })"

# 2. A second launch must not start a second app; it asks the first to surface.
$second = Start-Process -FilePath $Exe -ArgumentList '--safe' -WorkingDirectory (Split-Path $Exe) -PassThru -WindowStyle Hidden
Start-Sleep -Seconds 4
$second.Refresh()
Start-Sleep -Seconds 2
$hwnd2 = [TrayT]::Top([uint32]$p.Id)
$restored = ($hwnd2 -ne [IntPtr]::Zero) -and [TrayT]::Visible($hwnd2)
"restore  : second launch exited=$($second.HasExited)  first window back=$restored  -> $(if ($second.HasExited -and $restored) { 'PASS' } else { 'FAIL' })"

# 3. A clean quit, which is what keeps the WhatsApp login: both engines flush on shutdown.
Request-Quit -Exe $Exe
$deadline = (Get-Date).AddSeconds(20)
while ((Get-Date) -lt $deadline -and -not $p.HasExited) { Start-Sleep -Milliseconds 500; $p.Refresh() }
"quit     : process exited=$($p.HasExited)  -> $(if ($p.HasExited) { 'PASS' } else { 'FAIL - --quit was ignored' })"

if ($Engine -eq 'firefox') {
    Start-Sleep -Seconds 3
    $orphans = @(Get-CimInstance Win32_Process | Where-Object { $_.ExecutablePath -like '*\firefox\firefox.exe' })
    "orphans  : $($orphans.Count) firefox processes left  -> $(if ($orphans.Count -eq 0) { 'PASS' } else { 'FAIL - the job object did not take them' })"
}

Wait-ForExit | Out-Null
