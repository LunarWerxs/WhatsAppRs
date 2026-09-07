param([Parameter(Mandatory)][string]$Label, [string]$Prefs = '')
# Restart the real logged-in instance with a frame-timing probe, measure, and quit
# cleanly so the login survives. $Prefs overrides engine settings for the run.
$sp = 'C:\Users\blogi\AppData\Local\Temp\claude\D--NEWProjects\27409e72-a6c8-4a28-ba7f-b6aec022d82c\scratchpad'
$exe = 'D:\NEWProjects\WhatsAppRs\target\servo\whatsapp.exe'

$env:WHATSAPP_RS_INSTANCE_PORT = '47913'
Start-Process $exe -ArgumentList '--quit' -WorkingDirectory (Split-Path $exe) -Wait
$sw = [Diagnostics.Stopwatch]::StartNew()
while ((Get-Process whatsapp -ErrorAction SilentlyContinue) -and $sw.Elapsed.TotalSeconds -lt 40) { Start-Sleep 1 }

$env:WHATSAPP_RS_DATA_DIR = Join-Path $env:LOCALAPPDATA 'WhatsAppRs'
$env:WHATSAPP_RS_PROBE = '1'
$env:WHATSAPP_RS_EVAL = (Get-Content "$sp\jank-test.js" -Raw)
if ($Prefs) { $env:WHATSAPP_RS_PREFS = $Prefs } else { Remove-Item Env:WHATSAPP_RS_PREFS -ErrorAction SilentlyContinue }

$p = Start-Process $exe -ArgumentList '--safe' -WorkingDirectory (Split-Path $exe) -PassThru `
    -RedirectStandardError "$sp\jank-$Label.err" -RedirectStandardOutput "$sp\jank-$Label.out"
"[$Label] pid $($p.Id) prefs='$Prefs'"
Start-Sleep -Seconds 100
$p.Refresh()
"[$Label] working set $([math]::Round($p.WorkingSet64/1MB)) MB"
$found = Select-String -Path "$sp\jank-$Label.err" -Pattern 'JANK [^"\\]{0,160}' -AllMatches |
    ForEach-Object { $_.Matches.Value } | Select-Object -Unique -Last 1
if ($found) { "[$Label] $found" } else { "[$Label] no frame data yet" }
