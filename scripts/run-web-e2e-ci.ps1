[CmdletBinding()]
param(
    [ValidateRange(1, 65535)]
    [int]$Port = 8080,

    [ValidateRange(5, 300)]
    [int]$StartupTimeoutSeconds = 60
)

$ErrorActionPreference = 'Stop'
$RepoRoot = Split-Path -Parent $PSScriptRoot
$BackendDir = Join-Path $RepoRoot 'ts-backend'
$WebDir = Join-Path $RepoRoot 'web'
$LogDir = Join-Path $WebDir 'output\playwright\ci'
$HealthUrl = "http://127.0.0.1:$Port/api/health"
$BaseUrl = "http://127.0.0.1:$Port"
$BackendStdout = Join-Path $LogDir 'backend.stdout.log'
$BackendStderr = Join-Path $LogDir 'backend.stderr.log'

function Restore-EnvironmentVariable([string]$Name, [string]$Value) {
    if ($null -eq $Value) {
        Remove-Item "Env:$Name" -ErrorAction SilentlyContinue
    } else {
        Set-Item "Env:$Name" $Value
    }
}

function Wait-ForHealth([System.Diagnostics.Process]$Process) {
    $deadline = [DateTimeOffset]::UtcNow.AddSeconds($StartupTimeoutSeconds)
    while ([DateTimeOffset]::UtcNow -lt $deadline) {
        if ($Process.HasExited) {
            throw "backend exited before becoming ready (exit $($Process.ExitCode))"
        }
        try {
            $response = Invoke-WebRequest -Uri $HealthUrl -UseBasicParsing -TimeoutSec 2
            if ($response.StatusCode -eq 200) {
                return
            }
        } catch {
            # The server may still be starting.
        }
        Start-Sleep -Milliseconds 500
    }
    throw "backend did not become ready at $HealthUrl within $StartupTimeoutSeconds seconds"
}

function Read-LogTail([string]$Path) {
    if (Test-Path -LiteralPath $Path) {
        return (Get-Content -LiteralPath $Path -Tail 40) -join [Environment]::NewLine
    }
    return '<missing>'
}

$existingListener = Get-NetTCPConnection -State Listen -LocalPort $Port -ErrorAction SilentlyContinue
if ($existingListener) {
    throw "port $Port is already in use by PID $($existingListener.OwningProcess -join ', ')"
}

New-Item -ItemType Directory -Force -Path $LogDir | Out-Null
Remove-Item -LiteralPath $BackendStdout, $BackendStderr -Force -ErrorAction SilentlyContinue

Push-Location $BackendDir
try {
    & node scripts/embed-assets.mjs
    if ($LASTEXITCODE -ne 0) {
        throw "frontend asset embedding failed with exit $LASTEXITCODE"
    }
} finally {
    Pop-Location
}

$savedEnvironment = @{
    STATS_URL = $env:STATS_URL
    API_URL = $env:API_URL
    DEMO_SLOW_MO = $env:DEMO_SLOW_MO
    DEMO_STEP_PAUSE = $env:DEMO_STEP_PAUSE
}

$backendProcess = $null
$testExitCode = 1
try {
    $node = (Get-Command node -ErrorAction Stop).Source
    $backendProcess = Start-Process `
        -FilePath $node `
        -ArgumentList 'dev-server.mjs' `
        -WorkingDirectory $BackendDir `
        -PassThru `
        -WindowStyle Hidden `
        -RedirectStandardOutput $BackendStdout `
        -RedirectStandardError $BackendStderr

    Wait-ForHealth $backendProcess

    $env:STATS_URL = $BaseUrl
    $env:API_URL = $BaseUrl
    $env:DEMO_SLOW_MO = '0'
    $env:DEMO_STEP_PAUSE = '0'

    Push-Location $WebDir
    try {
        & npm.cmd run test:e2e
        $testExitCode = $LASTEXITCODE
    } finally {
        Pop-Location
    }

    if ($testExitCode -ne 0) {
        throw "browser E2E failed with exit $testExitCode"
    }
} catch {
    Write-Host "Browser E2E orchestration failed: $($_.Exception.Message)"
    Write-Host '--- backend stdout (tail) ---'
    Write-Host (Read-LogTail $BackendStdout)
    Write-Host '--- backend stderr (tail) ---'
    Write-Host (Read-LogTail $BackendStderr)
    throw
} finally {
    foreach ($entry in $savedEnvironment.GetEnumerator()) {
        Restore-EnvironmentVariable $entry.Key $entry.Value
    }
    if ($backendProcess -and -not $backendProcess.HasExited) {
        Stop-Process -Id $backendProcess.Id -Force -ErrorAction SilentlyContinue
        Wait-Process -Id $backendProcess.Id -Timeout 10 -ErrorAction SilentlyContinue
    }
}

Write-Host 'Browser E2E passed; backend process stopped.'
