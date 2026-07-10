<#
.SYNOPSIS
    Dev launcher for Stats Code: ensure deps, build backend, start API (:8080)
    + Vite (:5173), wait until ready, open the browser.

.DESCRIPTION
    Single source of truth for the dev startup flow. Desktop / repo .bat files
    are thin shims that call this script.

    Steps:
      1. Preflight  - node/npm present; install node_modules if missing.
      2. Restart    - stop node.exe processes listening on 8080/5173.
      3. Build      - npm run build in ts-backend (dev-server runs dist/).
      4. Start      - backend + frontend in their own console windows.
      5. Ready      - poll /api/health and the Vite page, then open browser.

.PARAMETER NoBrowser
    Skip opening the browser at the end.

.PARAMETER NoRestart
    Reuse services already listening on 8080/5173 instead of restarting them.

.PARAMETER SkipInstall
    Do not run npm install when node_modules is missing (fail instead).
#>
[CmdletBinding()]
param(
    [switch]$NoBrowser,
    [switch]$NoRestart,
    [switch]$SkipInstall
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$RepoRoot    = Split-Path -Parent $PSScriptRoot
$BackendDir  = Join-Path $RepoRoot 'ts-backend'
$FrontendDir = Join-Path $RepoRoot 'web'
$BackendUrl  = 'http://127.0.0.1:8080/api/health'
$FrontendUrl = 'http://127.0.0.1:5173/'

function Fail([string]$msg) {
    Write-Host ''
    Write-Host "[ERROR] $msg" -ForegroundColor Red
    exit 1
}

function Invoke-NpmInstall([string]$Dir) {
    Write-Host "  npm install in $Dir ..."
    Push-Location $Dir
    try {
        if (Test-Path (Join-Path $Dir 'package-lock.json')) {
            & npm.cmd ci
            if ($LASTEXITCODE -ne 0) {
                Write-Host '  npm ci failed, falling back to npm install ...'
                & npm.cmd install
            }
        } else {
            & npm.cmd install
        }
        if ($LASTEXITCODE -ne 0) {
            Fail "npm install failed in $Dir (exit $LASTEXITCODE)."
        }
    } finally {
        Pop-Location
    }
}

Write-Host '============================================'
Write-Host '  Stats Code dev launcher'
Write-Host "  repo: $RepoRoot"
Write-Host '============================================'

# --- 1. preflight -----------------------------------------------------------

if (-not (Get-Command node -ErrorAction SilentlyContinue)) { Fail 'node not found in PATH.' }
if (-not (Get-Command npm.cmd -ErrorAction SilentlyContinue)) { Fail 'npm not found in PATH.' }
if (-not (Test-Path (Join-Path $BackendDir 'dev-server.mjs'))) { Fail "backend entry missing: $BackendDir\dev-server.mjs" }
if (-not (Test-Path (Join-Path $FrontendDir 'package.json'))) { Fail "frontend package missing: $FrontendDir\package.json" }

$backendMods = Join-Path $BackendDir 'node_modules'
$frontendMods = Join-Path $FrontendDir 'node_modules'
if (-not (Test-Path $backendMods)) {
    if ($SkipInstall) { Fail "ts-backend\node_modules missing - run 'npm ci' in ts-backend first." }
    Write-Host '[0/4] Installing backend dependencies...'
    Invoke-NpmInstall $BackendDir
}
if (-not (Test-Path $frontendMods)) {
    if ($SkipInstall) { Fail "web\node_modules missing - run 'npm ci' in web first." }
    Write-Host '[0/4] Installing frontend dependencies...'
    Invoke-NpmInstall $FrontendDir
}

# --- 2. stop stale services -------------------------------------------------

function Stop-ListenerOnPort([int]$Port) {
    $conns = Get-NetTCPConnection -LocalPort $Port -State Listen -ErrorAction SilentlyContinue
    foreach ($c in ($conns | Select-Object -ExpandProperty OwningProcess -Unique)) {
        $p = Get-Process -Id $c -ErrorAction SilentlyContinue
        # Stop both node (dev) and stats-code (production SEA) so ports free up.
        if ($p -and ($p.ProcessName -eq 'node' -or $p.ProcessName -eq 'stats-code')) {
            Write-Host "  stopping $($p.ProcessName).exe (pid $($p.Id)) on port $Port"
            Stop-Process -Id $p.Id -Force
        }
    }
}

function Test-PortListening([int]$Port) {
    return [bool](Get-NetTCPConnection -LocalPort $Port -State Listen -ErrorAction SilentlyContinue)
}

if (-not $NoRestart) {
    Write-Host '[1/4] Restarting any Stats services on 8080/5173...'
    Stop-ListenerOnPort 8080
    Stop-ListenerOnPort 5173
    # Also stop stray production instances not bound yet
    Get-Process -Name 'stats-code' -ErrorAction SilentlyContinue | ForEach-Object {
        Write-Host "  stopping leftover stats-code.exe (pid $($_.Id))"
        Stop-Process -Id $_.Id -Force
    }
    Start-Sleep -Seconds 1
} else {
    Write-Host '[1/4] -NoRestart: existing services will be reused.'
}

# --- 3. build backend (dev-server.mjs executes packages/api/dist) ------------

if (-not (Test-PortListening 8080)) {
    Write-Host '[2/4] Building backend (npm run build in ts-backend)...'
    Push-Location $BackendDir
    try {
        & npm.cmd run build
        if ($LASTEXITCODE -ne 0) { Fail "backend build failed (exit $LASTEXITCODE) - services not started." }
    } finally {
        Pop-Location
    }
    # Keep SPA assets in build/assets in sync with web/dist for prod-mode probes.
    $embed = Join-Path $BackendDir 'scripts\embed-assets.mjs'
    if ((Test-Path $embed) -and (Test-Path (Join-Path $FrontendDir 'dist\index.html'))) {
        Write-Host '  embedding web/dist into ts-backend/build/assets ...'
        & node $embed
    }
} else {
    Write-Host '[2/4] Port 8080 already serving - skipping build (reusing running backend).'
}

# --- 4. start services in their own windows ----------------------------------

Write-Host '[3/4] Starting services...'

if (-not (Test-PortListening 8080)) {
    Start-Process -FilePath 'cmd.exe' `
        -ArgumentList '/k', 'title Stats Backend && node dev-server.mjs' `
        -WorkingDirectory $BackendDir | Out-Null
    Write-Host '  backend window started (port 8080)'
} else {
    Write-Host '  backend already listening on 8080'
}

if (-not (Test-PortListening 5173)) {
    Start-Process -FilePath 'cmd.exe' `
        -ArgumentList '/k', 'title Stats Frontend && npm.cmd run dev -- --host 127.0.0.1' `
        -WorkingDirectory $FrontendDir | Out-Null
    Write-Host '  frontend window started (port 5173)'
} else {
    Write-Host '  frontend already listening on 5173'
}

# --- 5. wait until ready, open browser ---------------------------------------

Write-Host '[4/4] Waiting for services to become ready...'

function Wait-Url([string]$Url, [int]$Tries = 90) {
    for ($i = 0; $i -lt $Tries; $i++) {
        try {
            $res = Invoke-WebRequest -Uri $Url -UseBasicParsing -TimeoutSec 2
            if ($res.StatusCode -ge 200 -and $res.StatusCode -lt 500) { return $true }
        } catch { }
        Start-Sleep -Seconds 1
    }
    return $false
}

if (-not (Wait-Url $BackendUrl))  { Fail "backend not ready: $BackendUrl - check the 'Stats Backend' window." }
if (-not (Wait-Url $FrontendUrl)) { Fail "frontend not ready: $FrontendUrl - check the 'Stats Frontend' window." }

if (-not $NoBrowser) {
    Start-Process $FrontendUrl
}

Write-Host ''
Write-Host '============================================' -ForegroundColor Green
Write-Host "  Stats Code is ready: $FrontendUrl"          -ForegroundColor Green
Write-Host "  Backend health:      $BackendUrl"           -ForegroundColor Green
Write-Host '  Close the Backend/Frontend windows to stop.'-ForegroundColor Green
Write-Host '  Production shortcut: Desktop\Stats Code.lnk'-ForegroundColor Green
Write-Host '============================================' -ForegroundColor Green
exit 0
