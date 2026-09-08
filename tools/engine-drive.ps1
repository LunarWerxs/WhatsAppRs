<#
Launch one engine build, wait, screenshot the app window, print the process tree and the
page's own view of itself, then stop it cleanly. The quick "does it work at all" check;
`engine-bench.ps1` is the one that produces numbers worth quoting.

  .\engine-drive.ps1 -Engine firefox
  .\engine-drive.ps1 -Engine cef -Wait 45 -Keep
#>
param(
    [Parameter(Mandatory)][ValidateSet('cef', 'firefox', 'webview2')][string]$Engine,
    [int]$Wait = 45,
    [string]$Exe,
    [string]$Tag,
    [switch]$Keep,
    [switch]$Real,
    [string]$OutDir = $PSScriptRoot,
    [int]$DebugPort = 47952,
    [int]$InstancePort = 47953
)
$ErrorActionPreference = 'Continue'
. "$PSScriptRoot\pagescript.ps1"
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes, System.Drawing
if (-not $Tag) { $Tag = $Engine }
if (-not $Exe) {
    $Exe = if ($Engine -eq 'cef') { 'D:\ct\cefapp\whatsapp.exe' } else { 'D:\NEWProjects\WhatsAppRs\target\release\whatsapp.exe' }
}
if (-not (Test-Path $Exe)) { throw "no executable at $Exe" }

$dataDir = if ($Real) { "$env:LOCALAPPDATA\WhatsAppRs" } else { "$env:LOCALAPPDATA\WhatsAppRs-drive-$Engine" }
New-Item -ItemType Directory -Force $dataDir | Out-Null
Set-Content -Path (Join-Path $dataDir 'mode.txt') -Value 'safe' -NoNewline
$env:WHATSAPP_RS_DATA_DIR      = $dataDir
$env:WHATSAPP_RS_INSTANCE_PORT = "$InstancePort"
$env:WHATSAPP_RS_DEBUG_PORT    = "$DebugPort"
$env:WHATSAPP_RS_ENGINE        = $Engine

$p = Start-Process -FilePath $Exe -ArgumentList '--safe' -WorkingDirectory (Split-Path $Exe) -PassThru `
        -RedirectStandardOutput "$OutDir\$Tag.out" -RedirectStandardError "$OutDir\$Tag.err"
"launched pid $($p.Id) : $Exe (engine $Engine)"
Start-Sleep -Seconds $Wait
if ($p.HasExited) {
    "EXITED early with $($p.ExitCode)"
    Get-Content "$OutDir\$Tag.err" -Tail 25 -ErrorAction SilentlyContinue
    exit 2
}

$root = [System.Windows.Automation.AutomationElement]::RootElement
$cond = New-Object System.Windows.Automation.PropertyCondition(
    [System.Windows.Automation.AutomationElement]::ProcessIdProperty, $p.Id)
$win = $null
foreach ($w in $root.FindAll([System.Windows.Automation.TreeScope]::Children, $cond)) {
    if ($w.Current.BoundingRectangle.Width -gt 200) { $win = $w }
}
if ($win) {
    "window: '$($win.Current.Name)' class=$($win.Current.ClassName) rect=$($win.Current.BoundingRectangle)"
    try { Add-Type -AssemblyName Microsoft.VisualBasic; [Microsoft.VisualBasic.Interaction]::AppActivate($p.Id); Start-Sleep -Milliseconds 700 } catch {}
    $r = $win.Current.BoundingRectangle
    $bmp = New-Object System.Drawing.Bitmap([int]$r.Width, [int]$r.Height)
    [System.Drawing.Graphics]::FromImage($bmp).CopyFromScreen([int]$r.X, [int]$r.Y, 0, 0, $bmp.Size)
    $bmp.Save("$OutDir\$Tag.png", [System.Drawing.Imaging.ImageFormat]::Png)
    "screenshot: $OutDir\$Tag.png"
} else {
    "NO WINDOW found for pid $($p.Id)"
}

$all = Get-CimInstance Win32_Process
$set = @($p.Id)
do {
    $n = @($all | Where-Object { $set -contains $_.ParentProcessId -and $set -notcontains $_.ProcessId } | ForEach-Object ProcessId)
    $set += $n
} while ($n.Count -gt 0)
# Only for the Firefox run: its processes are re-parented out of our subtree, but sweeping
# them in unconditionally counts a concurrent Firefox into a Chromium measurement.
if ($Engine -eq 'firefox') {
    $ffx = @($all | Where-Object { $_.ExecutablePath -like '*\firefox\firefox.exe' } | ForEach-Object ProcessId)
    $set = ($set + $ffx) | Sort-Object -Unique
}
$tree = $all | Where-Object { $set -contains $_.ProcessId }
"tree: $(@($tree).Count) processes, working set $([math]::Round((($tree | Measure-Object WorkingSetSize -Sum).Sum)/1MB,1)) MB, private $([math]::Round((($tree | Measure-Object PrivatePageCount -Sum).Sum)/1MB,1)) MB"
foreach ($x in $tree) { "    $($x.Name) pid=$($x.ProcessId) ws=$([math]::Round($x.WorkingSetSize/1MB,1))" }

$client = if ($Engine -eq 'firefox') { "$PSScriptRoot\bidi-eval.py" } else { "$PSScriptRoot\cdp-eval.py" }
"page: " + (Invoke-Python @($client, "$DebugPort", "$PSScriptRoot\page-state.js"))

"--- stderr, last 25 lines ---"
Get-Content "$OutDir\$Tag.err" -Tail 25 -ErrorAction SilentlyContinue |
    ForEach-Object { $_.Substring(0, [Math]::Min(200, $_.Length)) }

if (-not $Keep) {
    Request-Quit -Exe $Exe
    Start-Sleep -Seconds 3
    # Only this run's tree. An earlier version killed every bundled Firefox on the
    # machine, which silently destroyed a benchmark running in another window.
    foreach ($x in $tree) { Stop-Process -Id $x.ProcessId -Force -ErrorAction SilentlyContinue }
    "stopped"
} else {
    "left running as pid $($p.Id)"
}
