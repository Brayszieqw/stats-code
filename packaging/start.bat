@echo off
setlocal EnableExtensions
cd /d "%~dp0"
title Stats Code

if not exist "%~dp0stats-code.exe" (
  echo [ERROR] stats-code.exe not found next to this script.
  echo Place start.bat in the same folder as stats-code.exe, then try again.
  pause
  exit /b 1
)

echo Starting Stats Code...
echo - No API key required for Professional mode / deterministic statistics.
echo - Optional LLM can be configured later inside the app ^(your key stays local^).
echo.

start "" "%~dp0stats-code.exe"
exit /b 0
