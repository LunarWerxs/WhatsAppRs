<#
The regression test for the 2026-09-09 runaway, and the one this repo did not have.

On that day an NVIDIA driver reset took the D3D11 device away, Chromium retried the context
about seven thousand times a second for eight hours and forty-five minutes, and nothing in
the app noticed. `cef.log` reached 244 GB, the browser process reached 10.9 GB, and the
system drive got within about two hours of full. Every number in this repo up to then came
from a four-minute measurement on a login page, so nothing here could have caught it.

  .\soak.ps1                    # structural checks + a 10 minute plateau soak
  .\soak.ps1 -Minutes 0         # structural checks only, about 90 seconds
  .\soak.ps1 -Minutes 60        # a real soak

Fifteen checks in seven groups (twelve if `-Minutes 0` skips the plateau). The first group is
the root cause; the rest are the backstop:

  1. THE GPU IS OUT OF PROCESS. `--in-process-gpu` is what made the loop unbounded:
     `viz::GpuServiceImpl::MaybeExitOnContextLost` returns early when the GPU service is in
     the host process, so the context loss is never counted and Chromium never falls back to
     software. A `--type=gpu-process` child existing is the whole fix, observable from
     outside. If someone puts that switch back, this check fails and says so.
  2. The engine is holding `cef.log` open. Everything below depends on it.
  3. Deleting `cef.log` while it is held FAILS. This is not a curiosity: Chromium opens the
     log with no FILE_SHARE_DELETE, which is exactly why watchdog.rs truncates instead.
  4. Growing `cef.log` past its cap gets it rolled back to empty, under the engine's own
     open handle, and the onset snapshot is kept.
  5. A SUSTAINED firehose raises the watchdog's alarm, and the memory figure it reports
     covers the engine's CHILD processes and not just the browser process - which matters
     precisely because taking the GPU back out of process is what moves the next runaway
     into a child.
  6. Over the soak, the log stays bounded and private bytes do not climb.
  7. `--quit` still shuts it down cleanly, which is the thing that must never break: a
     killed Chromium skips its cookie flush and the phone has to re-pair.

Checks 4 and 5 drive the log directly rather than waiting for a GPU fault, because they are
testing the watchdog and the Windows file-sharing behaviour it depends on, and that needs no
GPU. To exercise the REAL path end to end, run this with `-Minutes 30` and press
Ctrl+Shift+Win+B once it is up: that restarts the display driver and delivers a genuine
DXGI_ERROR_DEVICE_REMOVED to the GPU process. Expect a one to two second blackout of
everything on screen that uses the GPU, and expect this app to still be running afterwards.
#>
param(
    [int]$Minutes = 10,
    [string]$Exe = (Join-Path (Split-Path $PSScriptRoot) 'target\release\whatsapp.exe'),
    # A small cap so the test writes megabytes instead of tens of them. The shipped default
    # is 16 MiB; watchdog.rs reads WHATSAPP_RS_LOG_CAP_BYTES so this needs no rebuild.
    [int]$CapMB = 1,
    [int]$InstancePort = 47941,
    [int]$StartTimeout = 120,
    [switch]$KeepProfile
)
$ErrorActionPreference = 'Stop'
. "$PSScriptRoot\pagescript.ps1"
if (-not (Test-Path $Exe)) { throw "no build at $Exe - run tools\build.ps1" }

$cap      = $CapMB * 1MB
$dataDir  = "$env:LOCALAPPDATA\WhatsAppRs-soak"
$log      = Join-Path $dataDir 'cef.log'
$onset    = Join-Path $dataDir 'cef.log.onset'
$errFile  = "$PSScriptRoot\soak.err"
$outFile  = "$PSScriptRoot\soak.out"

$script:failures = 0
function Check([string]$Name, [bool]$Ok, [string]$Detail = '') {
    if ($Ok) { Write-Host "PASS  $Name$(if ($Detail) { "  ($Detail)" })" }
    else { $script:failures++; Write-Host "FAIL  $Name$(if ($Detail) { "  ($Detail)" })" }
}

function Get-Tree([int]$RootPid) {
    $all = Get-CimInstance Win32_Process
    $set = @($RootPid)
    do {
        $n = @($all | Where-Object { $set -contains $_.ParentProcessId -and $set -notcontains $_.ProcessId } |
               ForEach-Object ProcessId)
        $set += $n
    } while ($n.Count -gt 0)
    return $all | Where-Object { $set -contains $_.ProcessId }
}

function Get-PrivateMB($tree) {
    [math]::Round((($tree | Measure-Object PrivatePageCount -Sum).Sum) / 1MB, 1)
}

# Append to the log the engine is holding open. FileShare.ReadWrite is required: Chromium's
# own handle is FILE_APPEND_DATA, so a stricter share mode here is refused by Windows.
function Add-Firehose([string]$Path, [int64]$Bytes) {
    $chunk = ('x' * 4095) + "`n"
    $rounds = [math]::Ceiling($Bytes / $chunk.Length)
    $fs = [System.IO.File]::Open($Path, [System.IO.FileMode]::Append,
                                 [System.IO.FileAccess]::Write, [System.IO.FileShare]::ReadWrite)
    try {
        $w = New-Object System.IO.StreamWriter($fs)
        for ($i = 0; $i -lt $rounds; $i++) { $w.Write($chunk) }
        $w.Flush()
        $w.Dispose()
    } finally { $fs.Dispose() }
}

function LogSizeMB { if (Test-Path $log) { [math]::Round((Get-Item $log).Length / 1MB, 2) } else { 0 } }

# ---------------------------------------------------------------- start

# ⛔ REFUSE rather than clear the way. `Wait-ForExit` below force-stops whatever it still
# finds after 40 s, and the owner's real, logged-in app is a `whatsapp.exe` too - killing it
# skips Chromium's cookie flush and the phone has to re-pair. bench.ps1 carries the same guard
# for the same reason, and this script has no business being the one that loses a login.
$others = @(Get-CimInstance Win32_Process |
    Where-Object { $_.Name -eq 'whatsapp.exe' -and $_.ExecutablePath -ne $Exe })
if ($others.Count -gt 0) {
    # No backticks in this message: PowerShell eats them as escapes inside a double-quoted
    # string, so a --quit written as code would reach the reader mangled.
    throw ("another whatsapp.exe is running from $($others[0].ExecutablePath) " +
           "(pids $($others.ProcessId -join ', ')). Stop it yourself: whatsapp.exe --quit. " +
           "This script will not kill an instance that might be the real, logged-in one.")
}
# Our own build may be left over from a previous run. Ask it nicely before Wait-ForExit
# resorts to force.
Request-Quit -Exe $Exe
Wait-ForExit | Out-Null
if (-not $KeepProfile) { Remove-Item -Recurse -Force $dataDir -ErrorAction SilentlyContinue }
New-Item -ItemType Directory -Force $dataDir | Out-Null
Remove-Item $errFile, $outFile -ErrorAction SilentlyContinue

$env:WHATSAPP_RS_DATA_DIR       = $dataDir
$env:WHATSAPP_RS_INSTANCE_PORT  = "$InstancePort"
$env:WHATSAPP_RS_LOG_CAP_BYTES  = "$cap"

"== soak: $Exe"
"   profile $dataDir, log cap $CapMB MB, plateau $Minutes min"

# --minimized so it sits in the tray instead of taking the screen for however long this runs.
$proc = Start-Process -FilePath $Exe -ArgumentList '--minimized' -PassThru -WindowStyle Hidden `
            -WorkingDirectory (Split-Path $Exe) `
            -RedirectStandardOutput $outFile -RedirectStandardError $errFile

try {
    # Up means the engine has spawned its children and opened its log.
    $deadline = (Get-Date).AddSeconds($StartTimeout)
    $tree = @()
    while ((Get-Date) -lt $deadline) {
        $tree = @(Get-Tree $proc.Id)
        if ($tree.Count -ge 3 -and (Test-Path $log)) { break }
        Start-Sleep -Seconds 2
    }
    Check 'the app started' ($tree.Count -ge 3) "$($tree.Count) processes"
    if ($tree.Count -lt 3) { throw "the app never came up - see $errFile" }

    # 1. THE ROOT CAUSE. --in-process-gpu removes this child, and with it Chromium's
    #    context-loss crash counter and its automatic fallback to software rendering.
    $gpu = @($tree | Where-Object { $_.CommandLine -like '*--type=gpu-process*' })
    Check 'the GPU runs out of process' ($gpu.Count -eq 1) `
        "$($gpu.Count) gpu-process children; --in-process-gpu must never come back"

    # 2. Everything below is meaningless if nothing is holding the file.
    $held = $false
    try {
        $h = [System.IO.File]::Open($log, [System.IO.FileMode]::Open,
                                    [System.IO.FileAccess]::Read, [System.IO.FileShare]::None)
        $h.Dispose()
    } catch { $held = $true }
    Check 'the engine is holding cef.log open' $held

    # 3. Why watchdog.rs truncates rather than deletes: Chromium's handle grants
    #    FILE_SHARE_READ | FILE_SHARE_WRITE and NOT FILE_SHARE_DELETE.
    $deleteRefused = $false
    try { Remove-Item $log -ErrorAction Stop } catch { $deleteRefused = $true }
    Check 'deleting cef.log while it is held is refused' $deleteRefused `
        'this is why the watchdog truncates'

    # 4. Past the cap, and back under it within a couple of ticks.
    $before = LogSizeMB
    Add-Firehose $log ($cap * 2)
    $grown = LogSizeMB
    $rolled = $false
    $deadline = (Get-Date).AddSeconds(15)
    while ((Get-Date) -lt $deadline) {
        if ((Get-Item $log).Length -le $cap) { $rolled = $true; break }
        Start-Sleep -Milliseconds 500
    }
    Check 'the log is rolled when it passes the cap' $rolled `
        "$before MB -> $grown MB -> $(LogSizeMB) MB, cap $CapMB MB"
    # Freshness matters: the onset deliberately OUTLIVES the process (a user told to "quit and
    # reopen" must not thereby delete the evidence), so a stale one from a previous run would
    # otherwise make this check pass without the watchdog having done anything.
    $onsetFresh = (Test-Path $onset) -and ((Get-Item $onset).Length -gt 0) -and
                  ((Get-Item $onset).LastWriteTime -gt $proc.StartTime)
    Check 'the onset snapshot is kept, and written by THIS run' $onsetFresh `
        "$(if (Test-Path $onset) { [math]::Round((Get-Item $onset).Length/1MB,2) } else { 0 }) MB"
    Check 'the app survived the roll' (-not $proc.HasExited)

    # 5. Sustained, so the rate test trips rather than the size test. The watchdog wants
    #    four consecutive 1.5 s ticks over 2 MB; ten seconds of 2.5 MB/s clears that.
    for ($i = 0; $i -lt 10; $i++) {
        Add-Firehose $log 2621440
        Start-Sleep -Seconds 1
    }
    Start-Sleep -Seconds 4
    $errText = (Get-Content $errFile -Raw -ErrorAction SilentlyContinue)
    Check 'the watchdog reported the roll' ($errText -match 'watchdog: rolled cef\.log')
    Check 'the watchdog raised the alarm on a sustained firehose' `
        ($errText -match 'watchdog: The engine is in a fault loop')

    # The memory probe walks this process AND its engine children, because removing
    # --in-process-gpu is exactly what moves the next runaway out into a child. Proved
    # against the browser process's own figure rather than a magic constant: the renderer
    # alone is bigger than the browser process, so a tree reading must beat it comfortably.
    $treeMB = if ($errText -match 'rolled cef\.log at \d+ bytes, tree private (\d+) MB') { [int]$Matches[1] } else { -1 }
    $ownMB = try { [math]::Round((Get-Process -Id $proc.Id).PrivateMemorySize64 / 1MB, 0) } catch { -1 }
    Check 'the memory probe covers the engine children, not just this process' `
        (($treeMB -gt 0) -and ($ownMB -gt 0) -and ($treeMB -gt ($ownMB * 1.3))) `
        "watchdog saw $treeMB MB across the tree, browser process alone is $ownMB MB"

    # ...and that it KNOWS it saw all of it. Both ways the walk can fail degrade it to
    # "this process only", which silently raises the real ceiling about tenfold. The
    # watchdog says so once per run; here that line must be absent.
    Check 'the memory probe was not flying blind' `
        (-not ($errText -match 'undercount|probe is unavailable')) `
        'a short reading raises the alarm ceiling without moving the number that documents it'

    # 6. The soak proper. Baseline after a minute so Chromium's own warm-up is not counted
    #    as growth.
    if ($Minutes -gt 0) {
        Start-Sleep -Seconds 60
        $baseline = Get-PrivateMB (Get-Tree $proc.Id)
        $peakLog = 0
        $deadline = (Get-Date).AddMinutes($Minutes)
        while ((Get-Date) -lt $deadline -and -not $proc.HasExited) {
            Start-Sleep -Seconds 15
            $peakLog = [math]::Max($peakLog, (LogSizeMB))
        }
        $final = Get-PrivateMB (Get-Tree $proc.Id)
        $growth = [math]::Round($final - $baseline, 1)
        # The incident grew 1.18 GB/hour, which is 197 MB over ten minutes. A healthy run on
        # a logged-out page is flat; 150 MB over the whole window is generous and still an
        # order of magnitude under the fault.
        $allowed = [math]::Max(150, 15 * $Minutes)
        Check 'private bytes plateau' ($growth -lt $allowed) `
            "$baseline MB -> $final MB, +$growth MB over $Minutes min, allowed +$allowed"
        Check 'the log stayed bounded for the whole soak' ($peakLog -le ($CapMB * 2)) `
            "peak $peakLog MB against a $CapMB MB cap"
        Check 'the app was still running at the end' (-not $proc.HasExited)
    }
}
finally {
    # 7. The one that must never break.
    Request-Quit -Exe $Exe
    $gone = Wait-ForExit -TimeoutSeconds 25
    Check 'the app quit cleanly on --quit' $gone 'a kill here would skip the cookie flush'
}

""
if ($script:failures -eq 0) { "SOAK PASSED"; exit 0 }
"SOAK FAILED: $script:failures check(s)"
exit 1
