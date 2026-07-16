@echo off
setlocal EnableExtensions
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
echo Install finished. You can double-click the Desktop shortcut "Stats Code",
echo or run start.bat in the install folder.
pause
exit /b 0
