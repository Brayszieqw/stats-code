<#
.SYNOPSIS
    Verifies a StatsCode-Demo-Pack with three isolated cold-start runs.
#>

[CmdletBinding()]
param(
    [string]$PackDir = (Join-Path $PSScriptRoot '..\ts-backend\build\demo-pack\StatsCode-Demo-Pack'),
    [switch]$SkipChecksum
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$PackDir = [System.IO.Path]::GetFullPath($PackDir)
$ExePath = Join-Path $PackDir 'stats-code.exe'
$DataPath = Join-Path $PackDir 'data\demo_cohort.csv'
$SumsPath = Join-Path $PackDir 'SHA256SUMS.txt'
$RecordName = (-join ([char[]]@(0x51B7, 0x542F, 0x52A8, 0x9A8C, 0x8BC1, 0x8BB0, 0x5F55))) + '.md'
$RecordPath = Join-Path $PackDir $RecordName

foreach ($required in @($ExePath, $DataPath)) {
    if (-not (Test-Path -LiteralPath $required -PathType Leaf)) {
        throw "Demo-Pack required file is missing: $required"
    }
}

function Test-PackChecksums {
    if (-not (Test-Path -LiteralPath $SumsPath -PathType Leaf)) {
        throw "Checksum file is missing: $SumsPath"
    }

    foreach ($line in [System.IO.File]::ReadAllLines($SumsPath, [System.Text.Encoding]::UTF8)) {
        if ([string]::IsNullOrWhiteSpace($line)) { continue }
        if ($line -notmatch '^([0-9a-fA-F]{64})  (.+)$') {
            throw "Cannot parse checksum entry: $line"
        }
        $expected = $Matches[1].ToLowerInvariant()
        $relativePath = $Matches[2].Replace('/', [System.IO.Path]::DirectorySeparatorChar)
        $target = Join-Path $PackDir $relativePath
        if (-not (Test-Path -LiteralPath $target -PathType Leaf)) {
            throw "Checksummed file is missing: $relativePath"
        }
        $actual = (Get-FileHash -LiteralPath $target -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($actual -ne $expected) {
            throw "SHA-256 mismatch: $relativePath"
        }
    }
}

function Wait-StatsCodePort {
    param(
        [Parameter(Mandatory = $true)][System.Diagnostics.Process]$Process,
        [int]$TimeoutSeconds = 20
    )

    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    while ([DateTime]::UtcNow -lt $deadline) {
        $Process.Refresh()
        if ($Process.HasExited) {
            throw "stats-code.exe exited before listening (exit $($Process.ExitCode))."
        }
        $listener = Get-NetTCPConnection -State Listen -OwningProcess $Process.Id -ErrorAction SilentlyContinue |
            Where-Object { $_.LocalPort -ge 8080 -and $_.LocalPort -le 8200 } |
            Select-Object -First 1
        if ($listener) { return [int]$listener.LocalPort }
        Start-Sleep -Milliseconds 200
    }
    throw 'stats-code.exe did not listen on a port from 8080 to 8200 within 20 seconds.'
}

function Invoke-ColdStartRound {
    param([Parameter(Mandatory = $true)][int]$Round)

    $isolatedAppData = Join-Path ([System.IO.Path]::GetTempPath()) ("stats-code-demo-round-{0}-{1}" -f $Round, [guid]::NewGuid())
    New-Item -ItemType Directory -Path $isolatedAppData -Force | Out-Null
    $previousAppData = $env:APPDATA
    $process = $null
    $startedAt = [DateTimeOffset]::Now
    try {
        $env:APPDATA = $isolatedAppData
        $process = Start-Process -FilePath $ExePath -ArgumentList '--no-browser' -WorkingDirectory $PackDir -WindowStyle Hidden -PassThru
        $port = Wait-StatsCodePort -Process $process
        $baseUri = "http://127.0.0.1:$port"

        $health = Invoke-RestMethod -Method Get -Uri "$baseUri/api/health" -TimeoutSec 10
        if ($health.status -ne 'ok') { throw "Unexpected health response: $($health | ConvertTo-Json -Compress)" }

        $session = Invoke-RestMethod -Method Post -Uri "$baseUri/api/sessions" -ContentType 'application/json' -Body '{}' -TimeoutSec 10
        if (-not $session.id) { throw 'Session creation did not return an id.' }

        $dataBase64 = [Convert]::ToBase64String([System.IO.File]::ReadAllBytes($DataPath))
        $uploadBody = @{ filename = 'demo_cohort.csv'; data = $dataBase64 } | ConvertTo-Json -Compress
        $dataset = Invoke-RestMethod -Method Post -Uri "$baseUri/api/sessions/$($session.id)/datasets" -ContentType 'application/json' -Body $uploadBody -TimeoutSec 20
        if (-not $dataset.dataset_id -or $dataset.row_count -lt 1) { throw 'Dataset upload returned an invalid summary.' }

        $runBody = @{
            skill_id = 'tableone'
            dataset_id = $dataset.dataset_id
            args = @{
                group = 'disease'
                continuous = @('age', 'bmi')
                categorical = @('sex', 'smoke')
            }
        } | ConvertTo-Json -Depth 5 -Compress
        $run = Invoke-RestMethod -Method Post -Uri "$baseUri/api/sessions/$($session.id)/run" -ContentType 'application/json' -Body $runBody -TimeoutSec 30
        if ($run.analysis.algorithm_id -ne 'tableone' -or $run.analysis.run_status -ne 'completed') {
            throw 'Table One did not return a completed run.'
        }

        $elapsed = [math]::Round(([DateTimeOffset]::Now - $startedAt).TotalSeconds, 2)
        return [pscustomobject]@{
            Round = $Round
            Port = $port
            Rows = $dataset.row_count
            DatasetSha256 = $dataset.sha256
            RunId = $run.analysis.run_id
            Seconds = $elapsed
        }
    } finally {
        if ($process) {
            $process.Refresh()
            if (-not $process.HasExited) {
                Stop-Process -Id $process.Id -Force
                $process.WaitForExit(5000) | Out-Null
            }
        }
        $env:APPDATA = $previousAppData
        if (Test-Path -LiteralPath $isolatedAppData) {
            Remove-Item -LiteralPath $isolatedAppData -Recurse -Force
        }
    }
}

if (-not $SkipChecksum) {
    Test-PackChecksums
    Write-Host '[verify-demo-pack] SHA-256 static files: PASS'
}

$version = (& $ExePath --version).Trim()
$results = @(1..3 | ForEach-Object { Invoke-ColdStartRound -Round $_ })

$record = @(
    '# Stats Code Cold-start Verification Record',
    '',
    "- Build version: $version",
    "- Verified at: $([DateTimeOffset]::Now.ToString('yyyy-MM-dd HH:mm:ss zzz'))",
    '- Path: every round used a fresh isolated APPDATA and executed start, health, session creation, CSV upload, Table One, and shutdown.',
    '- Criteria: every run returned algorithm_id=tableone and run_status=completed; every process was stopped.',
    '',
    '| Round | Port | Rows | Dataset SHA-256 | run_id | Seconds |',
    '|---:|---:|---:|---|---|---:|'
)
foreach ($result in $results) {
    $record += "| $($result.Round) | $($result.Port) | $($result.Rows) | $($result.DatasetSha256) | $($result.RunId) | $($result.Seconds) |"
}
$record += @('', '**Verdict: PASS**')
[System.IO.File]::WriteAllText($RecordPath, ($record -join "`n") + "`n", [System.Text.UTF8Encoding]::new($false))

$results | Format-Table Round, Port, Rows, RunId, Seconds -AutoSize
Write-Host "[verify-demo-pack] three cold-start runs: PASS -> $RecordPath"
