<#
Head-to-head bench for the two bundled-engine builds, and the OS webview as a control.

Why it is shaped like this, because the shape is the whole point:

  * It runs each configuration MORE THAN ONCE and reports every run. The most expensive
    mistake of the Servo round was trusting one sample: the same build measured 8.5 fps
    and 31.2 fps on consecutive runs, and a "memory win" derived from one reading turned
    out to halve the frame rate and save nothing. `summarise-bench.ps1` prints medians
    and the spread; a difference smaller than the spread is not a difference.
  * The clock starts when the PAGE is ready, not when the process starts. Chromium and
    Firefox reach a drawn chat list at different times, so "memory at 60 s after launch"
    would be comparing two different moments.
  * Memory is the whole process tree, working set and private bytes, the same method
    FINDINGS.md used for the C# wrapper (803 MB) and the OS webview. Working set
    double-counts shared pages across processes, which flatters a one-process engine;
    private bytes is the honest per-process number. Both are recorded, always.

  .\engine-bench.ps1 -Engine cef -Runs 3
  .\engine-bench.ps1 -Engine firefox -Runs 3 -Label trimmed
  .\engine-bench.ps1 -Engine firefox -Runs 3 -Label stock -Stock
  .\engine-bench.ps1 -Engine cef -Runs 1 -Real        # the real logged-in account
#>
param(
    [Parameter(Mandatory)][ValidateSet('cef', 'firefox', 'webview2')][string]$Engine,
    [string]$Label = 'default',
    [int]$Runs = 3,
    # Sample points, in seconds after the page reported itself ready.
    [int[]]$SampleAt = @(60, 240),
    [int]$ReadyTimeout = 180,
    # Use the real logged-in profile. Off by default: a scratch profile is logged out,
    # which is repeatable and cannot cost a login.
    [switch]$Real,
    # Firefox with none of the trimming prefs, to measure whether they do anything.
    [switch]$Stock,
    # Extra Firefox prefs for this run, "name=value;name=value". Exists so a pref sweep
    # needs no rebuild, and so what produced a number is recorded in the row with it.
    [string]$Prefs = '',
    # Extra Chromium switches for the CEF build, "switch,switch=value".
    [string]$Switches = '',
    [string]$Exe,
    [string]$OutFile = "$PSScriptRoot\bench-engines.jsonl",
    [int]$DebugPort = 47930,
    [int]$InstancePort = 47931
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing

if (-not $Exe) {
    $Exe = switch ($Engine) {
        'cef'      { 'D:\ct\cefapp\whatsapp.exe' }
        default    { 'D:\NEWProjects\WhatsAppRs\target\release\whatsapp.exe' }
    }
}
if (-not (Test-Path $Exe)) { throw "no executable at $Exe" }

# -Real means the persistent, LOGGED-IN profile for this engine, created by login.ps1.
# It is deliberately NOT %LOCALAPPDATA%\WhatsAppRs: that one belongs to the Servo build
# and holds a live WhatsApp session, and a bench must never be able to disturb it.
$dataDir = if ($Real) { "$env:LOCALAPPDATA\WhatsAppRs-live-$Engine" } else { "$env:LOCALAPPDATA\WhatsAppRs-bench-$Engine" }
if ($Real -and -not (Test-Path (Join-Path $dataDir 'mode.txt'))) {
    throw "no logged-in profile for $Engine yet. Run: .\login.ps1 -Engine $Engine, scan the QR, then re-run."
}
$evalClient = if ($Engine -eq 'firefox') { "$PSScriptRoot\bidi-eval.py" } else { "$PSScriptRoot\cdp-eval.py" }

$script:probeSeq = 0
function Invoke-PageScript([string]$File, [int]$Timeout = 60) {
    # `& python ...` cannot be used here. A sweep runs detached, with no console, and
    # PowerShell's native-command path then fails intermittently with the Win32 error
    # "No process is on the other end of the pipe" while querying console mode - which
    # killed a whole configuration mid-sweep, because $ErrorActionPreference is Stop.
    # Start-Process with redirected files does not touch a console at all.
    $script:probeSeq++
    $o = Join-Path $env:TEMP "wa-probe-$PID-$script:probeSeq.out"
    $e = Join-Path $env:TEMP "wa-probe-$PID-$script:probeSeq.err"
    try {
        $proc = Start-Process -FilePath 'python' -ArgumentList @($evalClient, "$DebugPort", $File) `
                    -PassThru -WindowStyle Hidden -RedirectStandardOutput $o -RedirectStandardError $e
        if (-not $proc.WaitForExit($Timeout * 1000)) {
            Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
            return $null
        }
        if ($proc.ExitCode -ne 0) { return $null }
        return ((Get-Content $o -Raw -ErrorAction SilentlyContinue) | Out-String).Trim()
    } catch {
        Write-Host "    (probe failed to launch: $($_.Exception.Message))"
        return $null
    } finally {
        Remove-Item $o, $e -ErrorAction SilentlyContinue
    }
}

function Get-Tree([int]$RootPid) {
    $all = Get-CimInstance Win32_Process
    $set = @($RootPid)
    do {
        $n = @($all | Where-Object { $set -contains $_.ParentProcessId -and $set -notcontains $_.ProcessId } |
               ForEach-Object ProcessId)
        $set += $n
    } while ($n.Count -gt 0)
    # Firefox re-execs through a launcher, so some of its processes are re-parented away
    # from our subtree. Sweep them in by image path as well, or the Firefox number is a
    # large undercount and the comparison is worthless.
    #
    # ONLY when this run is the Firefox one. Doing it unconditionally counted another
    # session's Firefox into a Chromium measurement and produced 1673 MB for a build that
    # measures around 550: a wrong number that looked entirely plausible.
    if ($Engine -eq 'firefox') {
        $ffx = @($all | Where-Object { $_.ExecutablePath -like '*\firefox\firefox.exe' } | ForEach-Object ProcessId)
        $set = ($set + $ffx) | Sort-Object -Unique
    }
    return $all | Where-Object { $set -contains $_.ProcessId }
}

function Measure-Tree($tree) {
    # CPU is cumulative since each process started, in 100 ns units. It answers a question
    # memory cannot: how much WORK did this engine do to put the same page on screen. On a
    # 32-core machine an engine can be busy without ever feeling slow, and this is the only
    # number in the run that notices.
    $cpu = (($tree | Measure-Object UserModeTime -Sum).Sum + ($tree | Measure-Object KernelModeTime -Sum).Sum) / 1e7
    [pscustomobject]@{
        processes  = @($tree).Count
        ws_mb      = [math]::Round((($tree | Measure-Object WorkingSetSize   -Sum).Sum) / 1MB, 1)
        private_mb = [math]::Round((($tree | Measure-Object PrivatePageCount -Sum).Sum) / 1MB, 1)
        cpu_s      = [math]::Round($cpu, 1)
    }
}

function Stop-Everything {
    # Our own executable only. firefox_view.rs holds its browser in a kill-on-close job
    # object, so stopping the host takes the browser with it, and killing every Firefox by
    # image path would reach into anything else running on this machine.
    Get-Process whatsapp -ErrorAction SilentlyContinue |
        Where-Object { $_.Path -eq $Exe } | Stop-Process -Force -ErrorAction SilentlyContinue
    Start-Sleep -Seconds 2
}

# Two benches at once produce numbers that are wrong and look right: the process sweep of
# one picks up the other's browser. Refuse rather than measure nonsense.
$others = @(Get-CimInstance Win32_Process |
    Where-Object { $_.Name -eq 'whatsapp.exe' -and $_.ExecutablePath -ne $Exe -and $_.ExecutablePath -notlike '*target\servo*' })
if ($others.Count -gt 0) {
    throw "another engine build is running (pids $($others.ProcessId -join ', ')). Stop it first: a concurrent run contaminates the process sweep."
}

"== $Engine / $Label : $Runs runs, exe $Exe"
for ($run = 1; $run -le $Runs; $run++) {
    Stop-Everything
    if (-not $Real) { Remove-Item -Recurse -Force $dataDir -ErrorAction SilentlyContinue }
    New-Item -ItemType Directory -Force $dataDir | Out-Null

    $env:WHATSAPP_RS_DATA_DIR     = $dataDir
    $env:WHATSAPP_RS_INSTANCE_PORT = "$InstancePort"
    $env:WHATSAPP_RS_DEBUG_PORT   = "$DebugPort"
    $env:WHATSAPP_RS_ENGINE       = $Engine
    if ($Stock) { $env:WHATSAPP_RS_FF_STOCK = '1' } else { Remove-Item Env:WHATSAPP_RS_FF_STOCK -ErrorAction SilentlyContinue }
    if ($Prefs)    { $env:WHATSAPP_RS_FF_PREFS = $Prefs }       else { Remove-Item Env:WHATSAPP_RS_FF_PREFS -ErrorAction SilentlyContinue }
    if ($Switches) { $env:WHATSAPP_RS_CEF_SWITCHES = $Switches } else { Remove-Item Env:WHATSAPP_RS_CEF_SWITCHES -ErrorAction SilentlyContinue }
    # Firefox's crash reporter is a whole extra process for a browser that is never going
    # to file a report from here.
    $env:MOZ_CRASHREPORTER_DISABLE = '1'
    # The OS-webview build has no code that reads WHATSAPP_RS_DEBUG_PORT: WebView2 takes its
    # switches from this environment variable instead, and it needs --remote-allow-origins
    # as well or it refuses the debugging websocket. Without both, the page probe never
    # connects and the run reports "never became ready" for a window that is on screen and
    # working, which is exactly what happened the first time.
    if ($Engine -eq 'webview2') {
        $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=$DebugPort --remote-allow-origins=*"
    } else {
        Remove-Item Env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS -ErrorAction SilentlyContinue
    }
    # Skip the first-run picker: light mode is retired, safe mode is the only mode.
    Set-Content -Path (Join-Path $dataDir 'mode.txt') -Value 'safe' -NoNewline

    $errFile = "$PSScriptRoot\bench-$Engine-$Label-$run.err"
    $t0 = Get-Date
    $p = Start-Process -FilePath $Exe -ArgumentList '--safe' -WorkingDirectory (Split-Path $Exe) -PassThru `
            -RedirectStandardOutput "$PSScriptRoot\bench-$Engine-$Label-$run.out" -RedirectStandardError $errFile
    "  run $run : pid $($p.Id)"

    # Wait for the page to say it is ready, and record how long that took: startup time is
    # a real difference between an in-process engine and a browser we have to launch.
    $ready = $null; $readyAt = $null
    $deadline = (Get-Date).AddSeconds($ReadyTimeout)
    while ((Get-Date) -lt $deadline) {
        Start-Sleep -Seconds 3
        if ($p.HasExited) { break }
        $state = Invoke-PageScript "$PSScriptRoot\page-state.js"
        if ($state -and $state -match '"state"\s*:\s*"(chats|qr)"') {
            $ready = $state
            $readyAt = [math]::Round(((Get-Date) - $t0).TotalSeconds, 1)
            break
        }
    }
    if ($p.HasExited) {
        "  run $run : EXITED early ($($p.ExitCode)); last stderr:"
        Get-Content $errFile -Tail 15 -ErrorAction SilentlyContinue | ForEach-Object { "      $_" }
        continue
    }
    if (-not $ready) { "  run $run : page never became ready within $ReadyTimeout s" }
    else { "  run $run : ready in $readyAt s -> $ready" }

    $samples = @()
    $readyTime = Get-Date
    foreach ($at in $SampleAt) {
        $wait = $at - ((Get-Date) - $readyTime).TotalSeconds
        if ($wait -gt 0) { Start-Sleep -Seconds ([int][math]::Ceiling($wait)) }
        $m = Measure-Tree (Get-Tree $p.Id)
        $samples += [pscustomobject]@{ at_s = $at; processes = $m.processes; ws_mb = $m.ws_mb; private_mb = $m.private_mb; cpu_s = $m.cpu_s }
        "  run $run : t+$at s  $($m.processes) proc  ws $($m.ws_mb) MB  private $($m.private_mb) MB  cpu $($m.cpu_s) s"
    }

    $frames = Invoke-PageScript "$PSScriptRoot\frames.js" 60
    "  run $run : frames $frames"
    $stateAfter = Invoke-PageScript "$PSScriptRoot\page-state.js"

    $diskKb = $null
    try { $diskKb = [math]::Round(((Get-ChildItem $dataDir -Recurse -File -ErrorAction SilentlyContinue | Measure-Object Length -Sum).Sum) / 1KB) } catch {}

    # This machine runs a dozen other agent sessions. Record what else was on it, so a run
    # taken while the box was under load is identifiable afterwards instead of quietly
    # sitting in the median.
    $os = Get-CimInstance Win32_OperatingSystem
    $machine = [pscustomobject]@{
        free_mb     = [math]::Round($os.FreePhysicalMemory / 1KB)
        processes   = (Get-CimInstance Win32_Process | Measure-Object).Count
    }

    $row = [pscustomobject]@{
        stamp      = (Get-Date).ToString('o')
        engine     = $Engine
        label      = $Label
        run        = $run
        real       = [bool]$Real
        prefs      = $Prefs
        switches   = $Switches
        stock      = [bool]$Stock
        ready_s    = $readyAt
        ready      = $ready
        samples    = $samples
        frames     = $frames
        state_after= $stateAfter
        profile_kb = $diskKb
        machine    = $machine
        exe        = $Exe
    }
    $row | ConvertTo-Json -Depth 6 -Compress | Add-Content -Path $OutFile

    # A clean shutdown, not a kill: Chromium flushes cookies on exit and Firefox flushes
    # its profile, and on both a kill is how you lose a WhatsApp login.
    & $Exe --quit 2>&1 | Out-Null
    Start-Sleep -Seconds 4
    Stop-Everything
}
"wrote $OutFile"
