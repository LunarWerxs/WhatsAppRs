<#
Measure Meta's own WhatsApp for Windows, by exactly the same method as everything else.

Not a proposal - DECISIONS.md #1 rejected that app permanently - but "is the real app lighter
than a browser?" deserves a number rather than an opinion, and it keeps being asked. It is
already installed and already linked on this machine, so unlike every other row in the
comparison this one is measured with a REAL ACCOUNT: the toughest possible comparison for our
builds and the fairest one for Meta's.

It only launches the app. It opens no chat and sends nothing, and it stops only the processes
it started, so anything already running is left exactly as it was.

**It matches on the executable path, not the process name.** Our own build is also called
`whatsapp.exe`, so a name match would fold this app's memory into ours, or ours into this one,
depending which ran first.

  .\official-app.ps1
#>
param([int]$Wait = 90)
$ErrorActionPreference = 'Continue'

$pkg = Get-AppxPackage -Name '5319275A.WhatsAppDesktop' | Select-Object -First 1
if (-not $pkg) { throw "Meta's WhatsApp for Windows is not installed on this machine" }
"package     : $($pkg.PackageFullName)"

$files = Get-ChildItem $pkg.InstallLocation -Recurse -File -ErrorAction SilentlyContinue
"on disk     : $([math]::Round((($files | Measure-Object Length -Sum).Sum)/1MB,1)) MB in $($files.Count) files"
"built on    : " + $(if (Test-Path (Join-Path $pkg.InstallLocation 'WebView2Loader.dll')) {
    'WebView2 - Chromium, the same engine class as ours' } else { 'no WebView2 loader found' })

# Exact membership, no tree walk.
#
# Two earlier versions of this got it wrong in opposite directions. Matching on the install
# path alone found ONE process at 256 MB and missed the browser entirely, because this app is a
# WebView2 shell and WebView2's renderer, GPU and utility processes are `msedgewebview2.exe`
# running out of the Edge WebView runtime folder. Adding a parent-child walk then swept in
# svchost, fontdrvhost and most of the machine, because a re-parented process leaves an orphan
# whose parent id resolves to something that owns everything.
#
# So: the app's own executables by path, plus the WebView2 processes that were handed a
# user-data-dir inside this package. Both are exact, and neither can run away.
function Get-AppTree($installLocation, $familyName) {
    $all = Get-CimInstance Win32_Process
    @($all | Where-Object {
        ($_.ExecutablePath -and $_.ExecutablePath.StartsWith($installLocation, 'OrdinalIgnoreCase')) -or
        ($_.Name -eq 'msedgewebview2.exe' -and $_.CommandLine -and $_.CommandLine -like "*$familyName*")
    })
}

$before = @(Get-AppTree $pkg.InstallLocation $pkg.PackageFamilyName | ForEach-Object ProcessId)
if ($before.Count -gt 0) { "note        : $($before.Count) of its processes were already running; those are left alone" }

Start-Process "shell:AppsFolder\$($pkg.PackageFamilyName)!App"
$deadline = (Get-Date).AddSeconds($Wait)
do {
    Start-Sleep -Seconds 3
    $tree = Get-AppTree $pkg.InstallLocation $pkg.PackageFamilyName
} while ($tree.Count -eq 0 -and (Get-Date) -lt $deadline)

if ($tree.Count -eq 0) {
    "FAILED to start - nothing running out of $($pkg.InstallLocation)"
    "(protocol activation can be refused from a non-interactive shell; try launching it by hand)"
    exit 2
}
# Let it settle for the rest of the window, the same as every other measurement here.
$left = ($deadline - (Get-Date)).TotalSeconds
if ($left -gt 0) { Start-Sleep -Seconds ([int]$left) }
$tree = Get-AppTree $pkg.InstallLocation $pkg.PackageFamilyName

$cpu = ((($tree | Measure-Object UserModeTime -Sum).Sum) + (($tree | Measure-Object KernelModeTime -Sum).Sum)) / 1e7
"processes   : $($tree.Count)"
"working set : $([math]::Round((($tree | Measure-Object WorkingSetSize   -Sum).Sum)/1MB,1)) MB"
"private     : $([math]::Round((($tree | Measure-Object PrivatePageCount -Sum).Sum)/1MB,1)) MB"
"cpu         : $([math]::Round($cpu,1)) s"
foreach ($x in ($tree | Sort-Object WorkingSetSize -Descending)) {
    "    $($x.Name) pid=$($x.ProcessId) ws=$([math]::Round($x.WorkingSetSize/1MB,1)) MB"
}

# Its message history, which is the fair comparison against our profile directory.
$data = Join-Path $env:LOCALAPPDATA "Packages\$($pkg.PackageFamilyName)"
if (Test-Path $data) {
    $d = (Get-ChildItem $data -Recurse -File -ErrorAction SilentlyContinue | Measure-Object Length -Sum).Sum / 1MB
    "profile     : {0:N1} MB" -f $d
}

foreach ($x in $tree) {
    if ($before -notcontains $x.ProcessId) { Stop-Process -Id $x.ProcessId -Force -ErrorAction SilentlyContinue }
}
"stopped what this script started; $($before.Count) pre-existing process(es) left alone"
