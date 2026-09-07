param(
    # 'demo' = sample chats, no network. 'pair' = the real thing up to the QR; never scanned.
    [ValidateSet('demo', 'pair')] [string]$Mode = 'demo',
    # 'light', 'dark', or '' to follow the Windows setting.
    [string]$Theme = '',
    [int]$Wait = 4,
    [string]$Exe = 'D:\NEWProjects\WhatsAppRs\target\release\whatsapp.exe',
    [string]$OutDir = $PSScriptRoot
)
# Drives the light-mode window and screenshots it: the pairing screen, the chat
# list, and a round trip through the send box. The list and bubbles are drawn by
# the app (no controls to find), so clicks go by position. Kills the app afterwards.
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes, System.Drawing, System.Windows.Forms
Add-Type -Namespace Drive -Name Mouse -MemberDefinition '[DllImport("user32.dll")] public static extern void mouse_event(uint f, uint x, uint y, uint d, System.UIntPtr e);'

if ($Theme) { $env:WHATSAPP_RS_THEME = $Theme } else { Remove-Item Env:WHATSAPP_RS_THEME -ErrorAction SilentlyContinue }
$tag = if ($Theme) { "$Mode-$Theme" } else { $Mode }
$flag = if ($Mode -eq 'demo') { '--light-demo' } else { '--light' }
$p = Start-Process -FilePath $Exe -ArgumentList $flag -PassThru
"launched pid $($p.Id) with $flag theme='$Theme'"
Start-Sleep -Seconds $Wait

$root = [System.Windows.Automation.AutomationElement]::RootElement
$cond = New-Object System.Windows.Automation.PropertyCondition(
    [System.Windows.Automation.AutomationElement]::ProcessIdProperty, $p.Id)
$win = $null
foreach ($w in $root.FindAll([System.Windows.Automation.TreeScope]::Children, $cond)) {
    if ($w.Current.ClassName -eq 'WhatsAppRsLight') { $win = $w }
}
if (-not $win) { "NO WINDOW; exited=$($p.HasExited)"; if (-not $p.HasExited) { Stop-Process -Id $p.Id -Force }; exit 2 }
$r = $win.Current.BoundingRectangle
"window: '$($win.Current.Name)' rect=$r"
# Bring it in front of whatever the operator has open, or clicks land elsewhere.
Add-Type -AssemblyName Microsoft.VisualBasic
try { [Microsoft.VisualBasic.Interaction]::AppActivate($p.Id) } catch { $win.SetFocus() }
# And out from under any overlay that hit-tests over the default position (an
# always-on-top sidebar lives at the left of this screen).
try {
    $win.GetCurrentPattern([System.Windows.Automation.TransformPattern]::Pattern).Move(1500, 300)
    Start-Sleep -Milliseconds 400
    $r = $win.Current.BoundingRectangle
    "moved to rect=$r"
} catch { "move failed: $_" }
Start-Sleep -Milliseconds 400

function Shot($file) {
    $bmp = New-Object System.Drawing.Bitmap([int]$r.Width, [int]$r.Height)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen([int]$r.X, [int]$r.Y, 0, 0, $bmp.Size)
    $bmp.Save($file, [System.Drawing.Imaging.ImageFormat]::Png)
    $g.Dispose(); $bmp.Dispose()
    "screenshot: $file"
}
function Click($x, $y) {
    [System.Windows.Forms.Cursor]::Position = New-Object System.Drawing.Point([int]$x, [int]$y)
    [Drive.Mouse]::mouse_event(2, 0, 0, 0, [UIntPtr]::Zero)
    [Drive.Mouse]::mouse_event(4, 0, 0, 0, [UIntPtr]::Zero)
}

Shot "$OutDir\light-$tag-1.png"

if ($Mode -eq 'demo') {
    # Geometry at 96 dpi: title bar ~31, header 59, search 49, rows 72; left pane 30%
    # of the client width clamped to 300..420; composer is the bottom 62.
    $titleBar = 31
    $leftW = [Math]::Min([Math]::Max(($r.Width - 16) * 0.30, 300), 420)
    $rowX = $r.X + 150
    $rowY = $r.Y + $titleBar + 59 + 49 + 36
    [System.Windows.Forms.Cursor]::Position = New-Object System.Drawing.Point([int]$rowX, [int]$rowY)
    Start-Sleep -Milliseconds 200
    $under = [System.Windows.Automation.AutomationElement]::FromPoint((New-Object System.Windows.Point($rowX, $rowY)))
    "under cursor before click: class='$($under.Current.ClassName)' name='$($under.Current.Name)' pid=$($under.Current.ProcessId)"
    # Twice: the first click on a freshly activated window sometimes only activates.
    Click $rowX $rowY
    Start-Sleep -Milliseconds 400
    Click $rowX $rowY
    Start-Sleep -Milliseconds 600
    Shot "$OutDir\light-$tag-1b.png"
    Click ($r.X + $leftW + 200) ($r.Bottom - 8 - 31)          # the send box
    Start-Sleep -Milliseconds 400
    "focused: " + [System.Windows.Automation.AutomationElement]::FocusedElement.Current.ControlType.ProgrammaticName
    [System.Windows.Forms.SendKeys]::SendWait('Hello from the send box{ENTER}')
    Start-Sleep -Seconds 2
    Shot "$OutDir\light-$tag-2.png"
}

$proc = Get-Process -Id $p.Id
"working set: $([math]::Round($proc.WorkingSet64/1MB,1)) MB, private: $([math]::Round($proc.PrivateMemorySize64/1MB,1)) MB, threads: $($proc.Threads.Count)"
Stop-Process -Id $p.Id -Force
"killed"
