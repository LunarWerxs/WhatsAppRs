<#
The control: the browser the OLD app drove, measured the same way as the two candidates.

This exists to check the METHOD, not to propose Chrome. Every number in the comparison is
"sum the working set and private bytes of the whole process tree", and that method can be
wrong in ways that are invisible from inside it. The C# wrapper measured 803 MB on a
logged-in account with this browser; if the same script gives a sane logged-out figure for
plain Chrome, the engine numbers beside it can be trusted. If it gives something absurd,
they cannot.

Uses a scratch profile in TEMP, so the owner's real Chrome profile is never touched and
never launched.

  .\chrome-control.ps1
#>
param(
    [int]$Wait = 60,
    [string]$Chrome = 'C:\Program Files\Google\Chrome\Application\chrome.exe'
)
$ErrorActionPreference = 'Continue'
if (-not (Test-Path $Chrome)) { throw "no Chrome at $Chrome" }

$profileDir = Join-Path $env:TEMP 'wa-chrome-control'
Remove-Item -Recurse -Force $profileDir -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $profileDir | Out-Null

# --app is what the C# wrapper used: a window with no tab strip and no URL bar.
$p = Start-Process $Chrome -PassThru -ArgumentList @(
    "--user-data-dir=$profileDir",
    '--app=https://web.whatsapp.com',
    '--no-first-run',
    '--no-default-browser-check'
)
"chrome pid $($p.Id), scratch profile $profileDir"
Start-Sleep -Seconds $Wait

$tree = Get-CimInstance Win32_Process | Where-Object { $_.CommandLine -like "*$profileDir*" }
$ws  = (($tree | Measure-Object WorkingSetSize   -Sum).Sum) / 1MB
$pv  = (($tree | Measure-Object PrivatePageCount -Sum).Sum) / 1MB
$cpu = ((($tree | Measure-Object UserModeTime -Sum).Sum) + (($tree | Measure-Object KernelModeTime -Sum).Sum)) / 1e7
"processes  : $(@($tree).Count)"
"workingset : $([math]::Round($ws,1)) MB"
"private    : $([math]::Round($pv,1)) MB"
"cpu        : $([math]::Round($cpu,1)) s"
$files = Get-ChildItem $profileDir -Recurse -File -ErrorAction SilentlyContinue
"profile    : $([math]::Round((($files | Measure-Object Length -Sum).Sum)/1MB,1)) MB in $($files.Count) files"

foreach ($x in $tree) { Stop-Process -Id $x.ProcessId -Force -ErrorAction SilentlyContinue }
Start-Sleep -Seconds 2
Remove-Item -Recurse -Force $profileDir -ErrorAction SilentlyContinue
"stopped, scratch profile removed"
