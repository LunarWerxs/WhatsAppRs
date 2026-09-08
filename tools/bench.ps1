<#
The memory and frame-timing bench.

Why it is shaped like this, because the shape is the point:

  * It runs each configuration MORE THAN ONCE and records every run. The most expensive
    mistake of the Servo round was trusting one sample: the same build measured 8.5 fps and
    31.2 fps on consecutive runs, and a "memory win" from a single reading turned out to halve
    the frame rate and save nothing. `bench-summary.py` prints medians AND the spread; a
    difference smaller than the spread is not a difference.
  * The clock starts when the PAGE reports itself ready, not when the process starts, so runs
    are comparable even when startup varies.
  * Memory is the whole process tree, working set and private bytes. Working set is what Task
    Manager shows and what every historical number here used; private bytes is what the machine
    actually loses. Both are recorded, always.
  * CPU seconds answer what neither memory number can: how much work was done to show the same
    page. It is the figure that exposed Firefox costing fifteen times what Chromium does.

  .\bench.ps1 -Runs 3
  .\bench.ps1 -Runs 3 -Real                       # the logged-in profile (see login.ps1)
  .\bench.ps1 -Runs 2 -Label nogpu -Switches 'disable-gpu'
#>
param(
    [string]$Label = 'default',
    [int]$Runs = 3,
    # Sample points, in seconds after the page reported itself ready.
    [int[]]$SampleAt = @(60, 240),
    [int]$ReadyTimeout = 180,
    # Use the real logged-in profile. Off by default: a scratch profile is logged out, which is
    # repeatable and cannot cost a login.
    [switch]$Real,
    # Extra Chromium switches for this run, "switch,switch=value". Recorded in the row with the
    # numbers, so what produced a result is never separated from it.
    [string]$Switches = '',
    [string]$Exe = (Join-Path (Split-Path $PSScriptRoot) 'target\release\whatsapp.exe'),
    [string]$OutFile = "$PSScriptRoot\bench.jsonl",
    [int]$DebugPort = 47930,
    [int]$InstancePort = 47931
)
$ErrorActionPreference = 'Stop'
. "$PSScriptRoot\pagescript.ps1"
if (-not (Test-Path $Exe)) { throw "no build at $Exe - run tools\build.ps1" }

$dataDir = if ($Real) { "$env:LOCALAPPDATA\WhatsAppRs" } else { "$env:LOCALAPPDATA\WhatsAppRs-bench" }
if ($Real -and -not (Test-Path $dataDir)) {
    throw "no logged-in profile yet. Run: .\login.ps1, scan the QR, then re-run with -Real."
}

$script:probeSeq = 0
function Invoke-PageScript([string]$File, [int]$Timeout = 60) {
    # See pagescript.ps1: `& python ...` from a detached script fails intermittently with
    # "No process is on the other end of the pipe" and, under ErrorActionPreference=Stop,
    # aborts the whole run.
    Invoke-Python @("$PSScriptRoot\cdp-eval.py", "$DebugPort", $File) -TimeoutSeconds $Timeout
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

function Measure-Tree($tree) {
    $cpu = (($tree | Measure-Object UserModeTime -Sum).Sum + ($tree | Measure-Object KernelModeTime -Sum).Sum) / 1e7
    [pscustomobject]@{
        processes  = @($tree).Count
        ws_mb      = [math]::Round((($tree | Measure-Object WorkingSetSize   -Sum).Sum) / 1MB, 1)
        private_mb = [math]::Round((($tree | Measure-Object PrivatePageCount -Sum).Sum) / 1MB, 1)
        cpu_s      = [math]::Round($cpu, 1)
    }
}

# Two benches at once produce numbers that are wrong and look right: one tree's sweep picks up
# the other's processes. Refuse rather than measure nonsense.
$others = @(Get-CimInstance Win32_Process |
    Where-Object { $_.Name -eq 'whatsapp.exe' -and $_.ExecutablePath -ne $Exe })
if ($others.Count -gt 0) {
    throw "another build is running (pids $($others.ProcessId -join ', ')). Stop it first: a concurrent run contaminates the process sweep."
}

"== $Label : $Runs runs, exe $Exe"
for ($run = 1; $run -le $Runs; $run++) {
    Wait-ForExit | Out-Null
    if (-not $Real) { Remove-Item -Recurse -Force $dataDir -ErrorAction SilentlyContinue }
    New-Item -ItemType Directory -Force $dataDir | Out-Null

    $env:WHATSAPP_RS_DATA_DIR      = $dataDir
    $env:WHATSAPP_RS_INSTANCE_PORT = "$InstancePort"
    $env:WHATSAPP_RS_DEBUG_PORT    = "$DebugPort"
    if ($Switches) { $env:WHATSAPP_RS_CEF_SWITCHES = $Switches } else { Remove-Item Env:WHATSAPP_RS_CEF_SWITCHES -ErrorAction SilentlyContinue }

    $errFile = "$PSScriptRoot\bench-$Label-$run.err"
    $t0 = Get-Date
    $p = Start-Process -FilePath $Exe -ArgumentList '--safe' -WorkingDirectory (Split-Path $Exe) -PassThru `
            -RedirectStandardOutput "$PSScriptRoot\bench-$Label-$run.out" -RedirectStandardError $errFile
    "  run $run : pid $($p.Id)"

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

    $diskKb = $null
    try { $diskKb = [math]::Round(((Get-ChildItem $dataDir -Recurse -File -ErrorAction SilentlyContinue | Measure-Object Length -Sum).Sum) / 1KB) } catch {}
    $os = Get-CimInstance Win32_OperatingSystem

    [pscustomobject]@{
        stamp       = (Get-Date).ToString('o')
        engine      = 'chromium'
        label       = $Label
        run         = $run
        real        = [bool]$Real
        switches    = $Switches
        ready_s     = $readyAt
        ready       = $ready
        samples     = $samples
        frames      = $frames
        state_after = Invoke-PageScript "$PSScriptRoot\page-state.js"
        profile_kb  = $diskKb
        machine     = [pscustomobject]@{ free_mb = [math]::Round($os.FreePhysicalMemory / 1KB) }
        exe         = $Exe
    } | ConvertTo-Json -Depth 6 -Compress | Add-Content -Path $OutFile

    # A clean shutdown, not a kill: Chromium flushes cookies on exit, and a kill is how a
    # WhatsApp login gets lost.
    Request-Quit -Exe $Exe
    Start-Sleep -Seconds 4
}
"wrote $OutFile"
