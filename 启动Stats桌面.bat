@echo off
setlocal EnableExtensions
chcp 65001 >nul
cd /d "%~dp0"
title Stats Code 桌面启动

rem 桌面壳：Electron 应用内窗口 + 本机后端（--no-browser）
rem 开发浏览器热更新请用「启动Stats前端.bat」
rem 发给同事请用 Demo-Pack：scripts\build-demo-pack.ps1 产物中的 start.bat
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\start-desktop.ps1" %*
set "CODE=%ERRORLEVEL%"
if not "%CODE%"=="0" (
  echo.
  echo Desktop launcher failed with exit code %CODE%. See messages above.
  pause
)
exit /b %CODE%