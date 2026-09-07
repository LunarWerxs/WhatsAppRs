param(
    [Parameter(Mandatory)][ValidateSet('Cancel','Safe','Light','Escape','List')] [string]$Action,
    [string]$Exe = 'D:\NEWProjects\WhatsAppRs\target\release\whatsapp.exe',
    [string]$Shot = "$PSScriptRoot\picker-$Action.png",
    [int]$SettleSeconds = 4
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes, System.Drawing

$dataDir = Join-Path $env:LOCALAPPDATA 'WhatsAppRs'
$modeFile = Join-Path $dataDir 'mode.txt'
if (Test-Path $modeFile) { Remove-Item $modeFile }

$p = Start-Process -FilePath $Exe -ArgumentList '--choose' -PassThru
"launched pid $($p.Id)"

$root = [System.Windows.Automation.AutomationElement]::RootElement
$cond = New-Object System.Windows.Automation.PropertyCondition(
    [System.Windows.Automation.AutomationElement]::ProcessIdProperty, $p.Id)
$dlg = $null
for ($i = 0; $i -lt 40 -and -not $dlg; $i++) {
    Start-Sleep -Milliseconds 250
    $wins = $root.FindAll([System.Windows.Automation.TreeScope]::Children, $cond)
    foreach ($w in $wins) { if ($w.Current.ClassName -eq '#32770') { $dlg = $w } }
}
if (-not $dlg) {
    "NO DIALOG after 10s; process exited=$($p.HasExited)"
    exit 2
}
"dialog: title='$($dlg.Current.Name)' class=$($dlg.Current.ClassName) rect=$($dlg.Current.BoundingRectangle)"

# Screenshot the dialog.
$r = $dlg.Current.BoundingRectangle
$bmp = New-Object System.Drawing.Bitmap([int]$r.Width, [int]$r.Height)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen([int]$r.X, [int]$r.Y, 0, 0, $bmp.Size)
$bmp.Save($Shot, [System.Drawing.Imaging.ImageFormat]::Png)
$g.Dispose(); $bmp.Dispose()
"screenshot: $Shot"

# Everything with a name, so the text and buttons are on record.
$all = $dlg.FindAll([System.Windows.Automation.TreeScope]::Descendants,
    [System.Windows.Automation.Condition]::TrueCondition)
$buttons = @()
foreach ($e in $all) {
    $c = $e.Current
    if ($c.Name) { "  [$($c.ControlType.ProgrammaticName -replace 'ControlType.','')] $($c.Name -replace "`n",' / ')" }
    if ($c.ControlType -eq [System.Windows.Automation.ControlType]::Button) { $buttons += $e }
}

if ($Action -eq 'List') { Stop-Process -Id $p.Id -Force; exit 0 }

if ($Action -eq 'Escape') {
    $dlg.SetFocus()
    Add-Type -AssemblyName System.Windows.Forms
    [System.Windows.Forms.SendKeys]::SendWait('{ESC}')
    "sent Escape"
} else {
    $want = switch ($Action) { 'Cancel' { 'Cancel' } 'Safe' { 'Safe mode' } 'Light' { 'Light mode' } }
    $target = $buttons | Where-Object { $_.Current.Name -like "$want*" } | Select-Object -First 1
    if (-not $target) { "BUTTON '$want' NOT FOUND"; Stop-Process -Id $p.Id -Force; exit 3 }
    $inv = $target.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern)
    $inv.Invoke()
    "clicked: $($target.Current.Name -replace "`n",' / ')"
}

Start-Sleep -Seconds $SettleSeconds
$p.Refresh()
"process exited: $($p.HasExited)"
if (Test-Path $modeFile) { "mode.txt: '$(Get-Content $modeFile -Raw)'" } else { "mode.txt: (absent)" }

if (-not $p.HasExited) {
    $wins = $root.FindAll([System.Windows.Automation.TreeScope]::Children, $cond)
    foreach ($w in $wins) { "  window: title='$($w.Current.Name)' class=$($w.Current.ClassName) rect=$($w.Current.BoundingRectangle)" }
    $proc = Get-Process -Id $p.Id
    "working set: $([math]::Round($proc.WorkingSet64/1MB,1)) MB, main window title='$($proc.MainWindowTitle)'"
    Stop-Process -Id $p.Id -Force
    "killed pid $($p.Id)"
}
