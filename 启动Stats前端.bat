@echo off
setlocal EnableExtensions
chcp 65001 >nul
title Stats Code Launcher

set "ROOT=D:\stats code\repo"
set "BACKEND_DIR=%ROOT%\ts-backend"
set "FRONTEND_DIR=%ROOT%\web"
set "BACKEND_URL=http://127.0.0.1:8080/api/health"
set "FRONTEND_URL=http://127.0.0.1:5173/"
set "RESTART_EXISTING=1"

echo ============================================
echo   Stats Code launcher
echo ============================================
echo.

if not exist "%BACKEND_DIR%\dev-server.mjs" (
  echo [ERROR] Backend entry not found: %BACKEND_DIR%\dev-server.mjs
  pause
  exit /b 1
)

if not exist "%FRONTEND_DIR%\package.json" (
  echo [ERROR] Frontend package not found: %FRONTEND_DIR%\package.json
  pause
  exit /b 1
)

where node >nul 2>nul
if errorlevel 1 (
  echo [ERROR] node was not found in PATH.
  pause
  exit /b 1
)

where npm >nul 2>nul
if errorlevel 1 (
  echo [ERROR] npm was not found in PATH.
  pause
  exit /b 1
)

if "%RESTART_EXISTING%"=="1" (
  echo [0/5] Restarting existing Stats node services on ports 8080/5173...
  call :StopNodeOnPort 8080
  call :StopNodeOnPort 5173
  powershell -NoProfile -ExecutionPolicy Bypass -Command "Start-Sleep -Seconds 1" >nul
)

echo [1/5] Building backend so dist matches current sources...
pushd "%BACKEND_DIR%"
call npm.cmd run build
if errorlevel 1 (
  popd
  echo.
  echo [ERROR] Backend build failed; not starting stale services. See errors above.
  pause
  exit /b 1
)
popd

powershell -NoProfile -ExecutionPolicy Bypass -Command "if (Get-NetTCPConnection -LocalPort 8080 -State Listen -ErrorAction SilentlyContinue) { exit 0 } else { exit 1 }"
if errorlevel 1 (
  echo [2/5] Starting backend on port 8080...
  start "Stats Backend" /D "%BACKEND_DIR%" cmd /k node dev-server.mjs
) else (
  echo [2/5] Backend port 8080 is already in use; reusing the running service.
)

powershell -NoProfile -ExecutionPolicy Bypass -Command "if (Get-NetTCPConnection -LocalPort 5173 -State Listen -ErrorAction SilentlyContinue) { exit 0 } else { exit 1 }"
if errorlevel 1 (
  echo [3/5] Starting frontend on port 5173...
  start "Stats Frontend" /D "%FRONTEND_DIR%" cmd /k npm.cmd run dev -- --host 127.0.0.1
) else (
  echo [3/5] Frontend port 5173 is already in use; reusing the running service.
)

echo [4/5] Waiting for services to become ready...
powershell -NoProfile -ExecutionPolicy Bypass -Command "$urls=@('%BACKEND_URL%','%FRONTEND_URL%'); foreach($url in $urls){ $ok=$false; for($i=0; $i -lt 45; $i++){ try { $res=Invoke-WebRequest -Uri $url -UseBasicParsing -TimeoutSec 2; if($res.StatusCode -ge 200 -and $res.StatusCode -lt 500){ $ok=$true; break } } catch {}; Start-Sleep -Seconds 1 }; if(-not $ok){ Write-Host '[ERROR] Service not ready:' $url; exit 1 } }"
if errorlevel 1 (
  echo.
  echo [ERROR] Stats Code did not become ready in time.
  echo Check the Backend and Frontend command windows for details.
  pause
  exit /b 1
)

echo [5/5] Opening Stats Code...
start "" "%FRONTEND_URL%"

echo.
echo ============================================
echo   Stats Code is ready: %FRONTEND_URL%
echo   Backend health: %BACKEND_URL%
echo   Close the Backend and Frontend windows to stop services.
echo ============================================
echo.
powershell -NoProfile -ExecutionPolicy Bypass -Command "Start-Sleep -Seconds 3" >nul
exit /b 0

:StopNodeOnPort
set "PORT=%~1"
for /f "tokens=5" %%p in ('netstat -ano ^| findstr /R /C:":%PORT% .*LISTENING"') do (
  for /f "tokens=1" %%n in ('tasklist /FI "PID eq %%p" /NH 2^>nul') do (
    if /I "%%n"=="node.exe" (
      echo   stopping node.exe on port %PORT% ^(PID %%p^)
      taskkill /F /PID %%p >nul 2>nul
    )
  )
)
exit /b 0
