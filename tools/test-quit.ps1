# Prove that --quit shuts a running instance down cleanly and that the cookie jar,
# which only gets written on a clean shutdown, actually lands on disk.
$sp = 'C:\Users\blogi\AppData\Local\Temp\claude\D--NEWProjects\27409e72-a6c8-4a28-ba7f-b6aec022d82c\scratchpad'
$exe = 'D:\NEWProjects\WhatsAppRs\target\servo\whatsapp.exe'
$data = Join-Path $env:LOCALAPPDATA 'WhatsAppRs-quittest'
if (Test-Path $data) { Remove-Item -LiteralPath $data -Recurse -Force }
New-Item -ItemType Directory -Force $data | Out-Null

$env:WHATSAPP_RS_DATA_DIR = $data
$env:WHATSAPP_RS_INSTANCE_PORT = '47996'
foreach ($v in 'WHATSAPP_RS_QUIT_AFTER', 'WHATSAPP_RS_EVAL', 'WHATSAPP_RS_PROBE') { Remove-Item "Env:$v" -ErrorAction SilentlyContinue }

$p = Start-Process $exe -ArgumentList '--safe' -WorkingDirectory (Split-Path $exe) -PassThru `
    -RedirectStandardError "$sp\quittest.err" -RedirectStandardOutput "$sp\quittest.out"
"launched pid $($p.Id); waiting for the page"
Start-Sleep -Seconds 25

"sending --quit"
$q = Start-Process $exe -ArgumentList '--quit' -WorkingDirectory (Split-Path $exe) -PassThru -Wait
$sw = [Diagnostics.Stopwatch]::StartNew()
while (-not $p.HasExited -and $sw.Elapsed.TotalSeconds -lt 40) { Start-Sleep -Milliseconds 500 }

if ($p.HasExited) {
    "the app exited on its own after $([int]$sw.Elapsed.TotalSeconds)s, code $($p.ExitCode)"
} else {
    "STILL RUNNING after 40s - clean quit did not work"
    Stop-Process -Id $p.Id -Force
}
"--- state files written ---"
Get-ChildItem (Join-Path $data 'servo') -File -ErrorAction SilentlyContinue | Select-Object Name, Length
Get-Content "$sp\quittest.err" -ErrorAction SilentlyContinue | Where-Object { $_ -match 'clean shutdown' }
