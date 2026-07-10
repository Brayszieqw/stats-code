@echo off
setlocal EnableExtensions
chcp 65001 >nul
title Stats Code 开发启动

rem 开发模式：起后端 :8080 + 前端 Vite :5173
rem 生产单文件 exe 请用桌面「Stats Code」快捷方式
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\start-stats.ps1" %*
set "CODE=%ERRORLEVEL%"
if not "%CODE%"=="0" (
  echo.
  echo Launcher failed with exit code %CODE%. See messages above.
  pause
)
exit /b %CODE%