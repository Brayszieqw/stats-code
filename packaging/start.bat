@echo off
setlocal EnableExtensions
chcp 65001 >nul
cd /d "%~dp0"
title Stats Code

if not exist "%~dp0stats-code.exe" (
  echo [ERROR] stats-code.exe not found next to this script.
  echo Place start.bat in the same folder as stats-code.exe, then try again.
  pause
  exit /b 1
)

echo Starting Stats Code...
echo - Professional mode works without an API key ^(deterministic stats^).
echo - Optional LLM can be configured later inside the app ^(key stays local^).
echo - Demo CSV: data\demo_cohort.csv  ^(or use in-app one-click load^)
echo.

start "" "%~dp0stats-code.exe"
exit /b 0
