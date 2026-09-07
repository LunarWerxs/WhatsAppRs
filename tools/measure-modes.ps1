param([int]$Wait = 30)
$exe = 'D:\NEWProjects\WhatsAppRs\target\release\whatsapp.exe'
$dataDir = Join-Path $env:LOCALAPPDATA 'WhatsAppRs'

function Get-Tree($rootPid) {
    $all = Get-CimInstance Win32_Process
    $set = @($rootPid)
    do {
        $n = @($all | Where-Object { $set -contains $_.ParentProcessId -and $set -notcontains $_.ProcessId } | ForEach-Object ProcessId)
        $set += $n
    } while ($n.Count -gt 0)
    $all | Where-Object { $set -contains $_.ProcessId }
}

function Measure-Mode($flag) {
    $p = Start-Process $exe -ArgumentList $flag -PassThru
    Start-Sleep -Seconds $Wait
    $t = Get-Tree $p.Id
    $ws = ($t | Measure-Object WorkingSetSize -Sum).Sum / 1MB
    $pv = ($t | Measure-Object PrivatePageCount -Sum).Sum / 1MB
    $th = ($t | Measure-Object ThreadCount -Sum).Sum
    "$flag after $Wait seconds: processes=$($t.Count) workingSet=$([math]::Round($ws,1)) MB private=$([math]::Round($pv,1)) MB threads=$th"
    foreach ($x in $t) { "    $($x.Name) pid=$($x.ProcessId) ws=$([math]::Round($x.WorkingSetSize/1MB,1)) priv=$([math]::Round($x.PrivatePageCount/1MB,1))" }
    foreach ($x in $t) { Stop-Process -Id $x.ProcessId -Force -ErrorAction SilentlyContinue }
}

Measure-Mode '--light'
Measure-Mode '--safe'

# Restore the pre-session state: no stored mode, no test session store.
foreach ($f in @('mode.txt', 'light-session.db', 'light-session.db-shm', 'light-session.db-wal')) {
    $path = Join-Path $dataDir $f
    if (Test-Path $path) { Remove-Item -LiteralPath $path -Force }
}
"data dir now: " + ((Get-ChildItem $dataDir | Select-Object -ExpandProperty Name) -join ', ')
