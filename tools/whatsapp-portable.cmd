@echo off
REM Run whatsapp-rs fully out of this one folder: engine, login, settings and log all land
REM beside this script instead of in %LOCALAPPDATA%. Put this file next to
REM whatsapp-rs-<version>-windows-x64.exe (or a renamed whatsapp.exe) and double-click it.
REM
REM Copy the folder to a USB stick afterwards and it moves with your login intact.
REM
REM WHAT IT STILL LEAVES ON THE HOST MACHINE: one Start Menu shortcut, "WhatsApp Rs".
REM That is not tidiness, it is a Windows requirement - Windows silently refuses to draw a
REM toast for an application it has no registered AppUserModelID for, and the shortcut is
REM what carries that id. Delete it afterwards if you want no trace, and accept that
REM notifications stop working. Nothing else is written outside this folder: no installer,
REM no registry keys, no Program Files, no uninstall entry.

setlocal

set "HERE=%~dp0"
set "WHATSAPP_RS_DATA_DIR=%HERE%data"
set "WHATSAPP_RS_ENGINE_DIR=%HERE%engine"

REM A DIFFERENT single-instance port from an installed copy's. Without this, running the
REM portable build while a normal one is already up does nothing visible: the app's single
REM instance lock is a loopback port, so the second start finds the port held, tells the
REM FIRST one to show itself, and exits. You would be looking at the installed copy's window
REM believing it was the portable one, with its own profile untouched.
if not defined WHATSAPP_RS_INSTANCE_PORT set "WHATSAPP_RS_INSTANCE_PORT=47914"

REM Take whichever exe is here: the released single file, or a plain whatsapp.exe.
set "EXE="
if exist "%HERE%whatsapp.exe" set "EXE=%HERE%whatsapp.exe"
for %%F in ("%HERE%whatsapp-rs-*-windows-x64.exe") do set "EXE=%%~fF"

if not defined EXE (
  echo No whatsapp exe found next to this script.
  echo Put whatsapp-rs-^<version^>-windows-x64.exe in: %HERE%
  pause
  exit /b 1
)

echo Running portable.
echo   exe    : %EXE%
echo   engine : %WHATSAPP_RS_ENGINE_DIR%
echo   profile: %WHATSAPP_RS_DATA_DIR%
echo.
echo First run unpacks about 346 MB into the engine folder and takes a few seconds.

start "" "%EXE%" %*
endlocal
