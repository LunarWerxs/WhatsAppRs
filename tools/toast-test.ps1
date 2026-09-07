param(
    [int]$Port = 9333,
    [string]$What = 'both',
    [int]$LoadWait = 20,
    [string]$Tag = 'app'
)
# End to end: launch safe mode with a debugging port, confirm the Start Menu
# shortcut was written with the AppUserModelID, fire a notification inside the
# page, and watch the desktop for the Windows toast.
$sp = $PSScriptRoot
$exe = 'D:\NEWProjects\WhatsAppRs\target\release\whatsapp.exe'
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=$Port --remote-allow-origins=*"
$p = Start-Process $exe -ArgumentList '--safe' -PassThru
"launched pid $($p.Id); waiting $LoadWait s for the page"
Start-Sleep -Seconds $LoadWait

$lnk = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\WhatsApp Rs.lnk'
if (Test-Path $lnk) {
    $sh = New-Object -ComObject WScript.Shell
    $s = $sh.CreateShortcut($lnk)
    $sa = New-Object -ComObject Shell.Application
    $aumid = $sa.NameSpace((Split-Path $lnk)).ParseName((Split-Path $lnk -Leaf)).ExtendedProperty('System.AppUserModel.ID')
    "shortcut: target=$($s.TargetPath)"
    "shortcut: workdir=$($s.WorkingDirectory) desc=$($s.Description) AUMID='$aumid' mtime=$((Get-Item $lnk).LastWriteTime.ToString('o'))"
} else {
    "shortcut: MISSING at $lnk"
}

$shot = "$sp\toast-$Tag.png"
python "$sp\cdp-notify.py" $Port $What
Start-Sleep -Milliseconds 1500
pwsh -NoProfile -File "$sp\shot-br.ps1" -Out $shot
Start-Sleep -Seconds 3
"--- notification database, newest first ---"
python "$sp\toast-db.py" 4

Stop-Process -Id $p.Id -Force
Start-Sleep -Seconds 2
Get-CimInstance Win32_Process -Filter "Name='msedgewebview2.exe'" | Where-Object { $_.CommandLine -match 'WhatsAppRs' } | ForEach-Object { Stop-Process -Id $_.ProcessId -Force }
Remove-Item -LiteralPath (Join-Path $env:LOCALAPPDATA 'WhatsAppRs\mode.txt') -Force -ErrorAction SilentlyContinue
"cleaned up"
