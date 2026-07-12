@echo off
setlocal
cd /d "%~dp0"
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\start-desktop.ps1"
if errorlevel 1 (
  echo.
  echo [Stats Code Desktop] start failed. See messages above.
  pause
)
endlocal
