<#
.SYNOPSIS
    Start Stats Code as a desktop app (Electron shell + local backend).
    UI opens inside the application window — not the system browser.
#>

[CmdletBinding()]
param(
    [switch]$SkipInstall,
    [switch]$BuildBackend
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$RepoRoot = Split-Path -Parent $PSScriptRoot
$DesktopDir = Join-Path $RepoRoot 'desktop'
$BackendDir = Join-Path $RepoRoot 'ts-backend'
$BackendExe = Join-Path $BackendDir 'build\stats-code.exe'
$BinJs = Join-Path $BackendDir 'packages\api\dist\bin.js'

function Fail([string]$msg) {
    Write-Host ''
    Write-Host "[ERROR] $msg" -ForegroundColor Red
    exit 1
}

Write-Host '============================================'
Write-Host '  Stats Code desktop launcher'
Write-Host "  repo: $RepoRoot"
Write-Host '  UI: in-app Electron window (backend --no-browser)'
Write-Host '  share to colleagues: scripts\build-demo-pack.ps1'
Write-Host '============================================'

if (-not (Test-Path -LiteralPath $DesktopDir -PathType Container)) {
    Fail "desktop package missing: $DesktopDir"
}

if (-not (Get-Command node -ErrorAction SilentlyContinue)) { Fail 'node not found in PATH.' }
if (-not (Get-Command npm.cmd -ErrorAction SilentlyContinue)) { Fail 'npm not found in PATH.' }

Push-Location $DesktopDir
try {
    if (-not $SkipInstall) {
        if (-not (Test-Path -LiteralPath (Join-Path $DesktopDir 'node_modules\electron'))) {
            Write-Host '[1/3] npm install (desktop)...'
            & npm.cmd install
            if ($LASTEXITCODE -ne 0) { Fail "npm install failed in desktop (exit $LASTEXITCODE)" }
        } else {
            Write-Host '[1/3] desktop dependencies present'
        }
    } else {
        Write-Host '[1/3] -SkipInstall: not installing desktop deps'
    }

    $hasExe = Test-Path -LiteralPath $BackendExe -PathType Leaf
    $hasBin = Test-Path -LiteralPath $BinJs -PathType Leaf
    if (-not $hasExe -and -not $hasBin) {
        if ($BuildBackend) {
            Write-Host '[2/3] backend artifact missing — building SEA (npm run sea)...'
            Push-Location $BackendDir
            try {
                & npm.cmd run build
                if ($LASTEXITCODE -ne 0) { Fail "backend build failed (exit $LASTEXITCODE)" }
                & npm.cmd run sea
                if ($LASTEXITCODE -ne 0) { Fail "backend sea failed (exit $LASTEXITCODE)" }
            } finally {
                Pop-Location
            }
            if (-not (Test-Path -LiteralPath $BackendExe -PathType Leaf) -and -not (Test-Path -LiteralPath $BinJs -PathType Leaf)) {
                Fail "backend still missing after build: $BackendExe"
            }
        } else {
            Write-Host '[2/3] WARNING: backend artifact missing' -ForegroundColor Yellow
            Write-Host "  expected: $BackendExe"
            Write-Host '  or:      ts-backend\packages\api\dist\bin.js'
            Write-Host '  fix:     cd ts-backend; npm run build; npm run sea'
            Write-Host '  or:      scripts\start-desktop.ps1 -BuildBackend'
            Write-Host '  continuing — Electron may fail until backend exists.'
        }
    } else {
        if ($hasExe) {
            Write-Host "[2/3] backend SEA: $BackendExe"
        } else {
            Write-Host "[2/3] backend bin.js: $BinJs"
        }
    }

    Write-Host '[3/3] launching Electron shell...'
    & npm.cmd start
    if ($LASTEXITCODE -ne 0) { Fail "electron start failed (exit $LASTEXITCODE)" }
} finally {
    Pop-Location
}
