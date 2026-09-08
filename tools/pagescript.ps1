# Shared helper: run a python probe and return its output, without a console.
#
# Dot-source it: . "$PSScriptRoot\pagescript.ps1"
#
# `& python ...` cannot be used from these scripts. They run detached, with no console, and
# PowerShell's native-command path then fails intermittently with the Win32 error
# "No process is on the other end of the pipe" while querying console mode. It is not
# deterministic, which is worse than if it were: it silently blanked three probes in one run
# and left the results looking like the browser had nothing to say. Start-Process with
# redirected files never touches a console.

$script:pageScriptSeq = 0

function Invoke-Python {
    param(
        [Parameter(Mandatory)][string[]]$Arguments,
        [int]$TimeoutSeconds = 90
    )
    $script:pageScriptSeq++
    $o = Join-Path $env:TEMP "wa-py-$PID-$script:pageScriptSeq.out"
    $e = Join-Path $env:TEMP "wa-py-$PID-$script:pageScriptSeq.err"
    try {
        $proc = Start-Process -FilePath 'python' -ArgumentList $Arguments -PassThru -WindowStyle Hidden `
                    -RedirectStandardOutput $o -RedirectStandardError $e
        if (-not $proc.WaitForExit($TimeoutSeconds * 1000)) {
            Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
            return "(probe timed out after ${TimeoutSeconds}s)"
        }
        $out = (Get-Content $o -Raw -ErrorAction SilentlyContinue)
        if ($proc.ExitCode -ne 0) {
            $err = (Get-Content $e -Raw -ErrorAction SilentlyContinue)
            $line = ($err -split "`n" | Where-Object { $_.Trim() } | Select-Object -Last 1)
            return "(probe failed, exit $($proc.ExitCode): $line)"
        }
        return ($out | Out-String).Trim()
    } catch {
        return "(probe could not start: $($_.Exception.Message))"
    } finally {
        Remove-Item $o, $e -ErrorAction SilentlyContinue
    }
}

# Wait until no engine build is running, so the next launch takes the single-instance lock
# instead of seeing one held and exiting with code 0 - which looks exactly like a crash that
# returned success, and blanked two probes before it was noticed.
#
# It waits for ANY build, not just $Exe. The single-instance lock is a TCP port, and the
# scripts share one port across engines, so a Chromium instance still shutting down blocks a
# Firefox launch just as effectively as another Firefox would. Matching only on $Exe was the
# first version and it missed exactly that case.
#
# The Servo build is excluded by path: it is the owner's real, logged-in instance and must
# never be waited on or stopped.
function Wait-ForExit {
    param([string]$Exe, [int]$TimeoutSeconds = 40)
    $ours = { @(Get-CimInstance Win32_Process |
        Where-Object { $_.Name -eq 'whatsapp.exe' -and $_.ExecutablePath -notlike '*target\servo*' }) }
    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    while ((Get-Date) -lt $deadline) {
        if ((& $ours).Count -eq 0) { return $true }
        Start-Sleep -Milliseconds 500
    }
    foreach ($p in (& $ours)) { Stop-Process -Id $p.ProcessId -Force -ErrorAction SilentlyContinue }
    Start-Sleep -Seconds 2
    return $false
}
