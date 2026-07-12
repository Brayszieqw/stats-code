<#
.SYNOPSIS
    Start Stats Code as a desktop app (Electron shell + local backend).
    UI opens inside the application window — not the system browser.
#>

[CmdletBinding()]
param(
    [switch]$SkipInstall
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$RepoRoot = Split-Path -Parent $PSScriptRoot
$DesktopDir = Join-Path $RepoRoot 'desktop'
$BackendExe = Join-Path $RepoRoot 'ts-backend\build\stats-code.exe'
$BinJs = Join-Path $RepoRoot 'ts-backend\packages\api\dist\bin.js'

if (-not (Test-Path -LiteralPath $DesktopDir -PathType Container)) {
    throw "desktop package missing: $DesktopDir"
}

Push-Location $DesktopDir
try {
    if (-not $SkipInstall) {
        if (-not (Test-Path -LiteralPath (Join-Path $DesktopDir 'node_modules\electron'))) {
            Write-Host '[start-desktop] npm install (desktop)'
            & npm.cmd install
            if ($LASTEXITCODE -ne 0) { throw "npm install failed in desktop (exit $LASTEXITCODE)" }
        }
    }

    if (-not (Test-Path -LiteralPath $BackendExe -PathType Leaf) -and -not (Test-Path -LiteralPath $BinJs -PathType Leaf)) {
        Write-Host '[start-desktop] backend artifact missing — building SEA is recommended'
        Write-Host "  expected: $BackendExe"
        Write-Host '  or run:  cd ts-backend; npm run build; npm run sea'
    }

    Write-Host '[start-desktop] launching Electron shell (in-app UI, --no-browser backend)'
    & npm.cmd start
    if ($LASTEXITCODE -ne 0) { throw "electron start failed (exit $LASTEXITCODE)" }
} finally {
    Pop-Location
}
