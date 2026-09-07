param(
    [string]$Exe = 'D:\NEWProjects\WhatsAppRs\target\servo\whatsapp.exe',
    [int]$Wait = 25,
    # A scratch profile and its own instance lock, so a real logged-in instance is untouched.
    [string]$DataDir = "$env:LOCALAPPDATA\WhatsAppRs-test",
    [string]$OutDir = $PSScriptRoot,
    [string]$Tag = 'safe'
)
# Launches safe mode, waits for the page, screenshots the window, prints memory
# for the whole process tree, and kills it. Works for either engine build.
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes, System.Drawing
New-Item -ItemType Directory -Force $DataDir | Out-Null
$env:WHATSAPP_RS_DATA_DIR = $DataDir
$env:WHATSAPP_RS_INSTANCE_PORT = '47998'
if (-not $env:RUST_LOG) { $env:RUST_LOG = 'warn,servo=info,whatsapp_rs=info' }
# stderr carries the engine log (RUST_LOG) and the app's own load-status lines.
$p = Start-Process -FilePath $Exe -ArgumentList '--safe' -WorkingDirectory (Split-Path $Exe) -PassThru `
    -RedirectStandardOutput "$OutDir\$Tag.out" -RedirectStandardError "$OutDir\$Tag.err"
"launched pid $($p.Id) from $Exe"
Start-Sleep -Seconds $Wait
if ($p.HasExited) { "EXITED early with $($p.ExitCode)"; exit 2 }

$root = [System.Windows.Automation.AutomationElement]::RootElement
$cond = New-Object System.Windows.Automation.PropertyCondition(
    [System.Windows.Automation.AutomationElement]::ProcessIdProperty, $p.Id)
$win = $null
foreach ($w in $root.FindAll([System.Windows.Automation.TreeScope]::Children, $cond)) {
    if ($w.Current.BoundingRectangle.Width -gt 200) { $win = $w }
}
if ($win) {
    "window: '$($win.Current.Name)' class=$($win.Current.ClassName) rect=$($win.Current.BoundingRectangle)"
    try { Add-Type -AssemblyName Microsoft.VisualBasic; [Microsoft.VisualBasic.Interaction]::AppActivate($p.Id); Start-Sleep -Milliseconds 500 } catch {}
    $r = $win.Current.BoundingRectangle
    $bmp = New-Object System.Drawing.Bitmap([int]$r.Width, [int]$r.Height)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen([int]$r.X, [int]$r.Y, 0, 0, $bmp.Size)
    $file = "$OutDir\$Tag.png"
    $bmp.Save($file, [System.Drawing.Imaging.ImageFormat]::Png)
    $g.Dispose(); $bmp.Dispose()
    "screenshot: $file"
} else {
    "NO WINDOW found for pid $($p.Id)"
}

$all = Get-CimInstance Win32_Process
$set = @($p.Id)
do {
    $n = @($all | Where-Object { $set -contains $_.ParentProcessId -and $set -notcontains $_.ProcessId } | ForEach-Object ProcessId)
    $set += $n
} while ($n.Count -gt 0)
$tree = $all | Where-Object { $set -contains $_.ProcessId }
"tree: $($tree.Count) processes, working set $([math]::Round(($tree | Measure-Object WorkingSetSize -Sum).Sum/1MB,1)) MB, private $([math]::Round(($tree | Measure-Object PrivatePageCount -Sum).Sum/1MB,1)) MB"
foreach ($x in $tree) { "    $($x.Name) pid=$($x.ProcessId) ws=$([math]::Round($x.WorkingSetSize/1MB,1))" }
foreach ($x in $tree) { Stop-Process -Id $x.ProcessId -Force -ErrorAction SilentlyContinue }
Start-Sleep -Milliseconds 500
"--- stderr (engine log), last 40 lines ---"
Get-Content "$OutDir\$Tag.err" -Tail 40 -ErrorAction SilentlyContinue | ForEach-Object { $_.Substring(0, [Math]::Min(240, $_.Length)) }
"killed; data dir now: " + ((Get-ChildItem $DataDir -Recurse -File | Measure-Object Length -Sum).Sum / 1KB) + " KB in " + ((Get-ChildItem $DataDir -Recurse -File | Measure-Object).Count) + " files"
