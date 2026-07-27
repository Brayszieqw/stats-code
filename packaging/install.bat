@echo off
setlocal EnableExtensions
chcp 65001 >nul
cd /d "%~dp0"
title Stats Code Installer

if not exist "%~dp0stats-code.exe" (
  echo [ERROR] stats-code.exe not found next to this script.
  pause
  exit /b 1
)
if not exist "%~dp0install.ps1" (
  echo [ERROR] install.ps1 not found next to this script.
  pause
  exit /b 1
)

echo Installing Stats Code for the current Windows user...
echo - No admin / UAC required
echo - No API keys are copied or installed
echo - Optional demo data will be copied when present
echo.

powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0install.ps1"
set "CODE=%ERRORLEVEL%"
if not "%CODE%"=="0" (
  echo.
  echo Install failed with exit code %CODE%.
  pause
  exit /b %CODE%
)

echo.
echo Install finished.
echo - Desktop shortcut: "Stats Code"
echo - Or run start.bat next to the installed stats-code.exe
pause
exit /b 0
