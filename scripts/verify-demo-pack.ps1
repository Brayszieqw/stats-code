<#
.SYNOPSIS
    Verifies a StatsCode-Demo-Pack with three isolated cold-start runs.
#>

[CmdletBinding()]
param(
    [string]$PackDir,
    [switch]$SkipChecksum
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

if ([string]::IsNullOrWhiteSpace($PackDir)) {
    # This script ships INSIDE the demo pack, and colleague-README.txt tells the
    # recipient to run it with no arguments from the extracted folder. There it
    # sits beside stats-code.exe, so prefer its own directory; the repo-relative
    # build output is only the fallback for running it from a source checkout.
    $SelfDir = $PSScriptRoot
    if (Test-Path -LiteralPath (Join-Path $SelfDir 'stats-code.exe') -PathType Leaf) {
        $PackDir = $SelfDir
    } else {
        $PackDir = Join-Path $SelfDir '..\ts-backend\build\demo-pack\StatsCode-Demo-Pack'
    }
}

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

        $protocolBody = @{
            status = 'Approved'
            research_question = 'Are baseline characteristics different between disease groups?'
            study_design = 'cross_sectional'
            population = 'De-identified demonstration cohort participants'
            eligibility_criteria = 'One unique row per participant with complete Table One fields'
            exposure = 'Baseline demographic and smoking characteristics'
            comparator = 'Disease status groups'
            outcome = 'Disease status'
            time_zero = 'Baseline assessment'
            follow_up = 'Cross-sectional analysis; no longitudinal follow-up'
            analysis_unit = 'Participant'
            estimand = 'Descriptive between-group differences in baseline characteristics'
            confounders = 'Not applicable to the descriptive Table One analysis'
            missing_data_strategy = 'Block the demonstration run if selected fields are missing'
            primary_analysis = 'Table One by disease status'
            sensitivity_analysis = 'Verify the same approved specification after isolated cold starts'
        } | ConvertTo-Json -Depth 4 -Compress
        $protocolSession = Invoke-RestMethod -Method Patch -Uri "$baseUri/api/sessions/$($session.id)/protocol" -ContentType 'application/json' -Body $protocolBody -TimeoutSec 10
        $protocol = $protocolSession.research_protocol
        if (
            $protocol.status -ne 'Approved' -or
            $protocol.version -lt 1 -or
            -not $protocol.approval_id -or
            $protocol.content_sha256 -notmatch '^[0-9a-f]{64}$'
        ) {
            throw 'Server did not return a valid approved research protocol.'
        }

        $analysisArgs = @{
            group = 'disease'
            continuous = @('age', 'bmi')
            categorical = @('sex', 'smoke')
        }
        $auditBody = @{
            skill_id = 'tableone'
            args = $analysisArgs
            expected_protocol_version = $protocol.version
        } | ConvertTo-Json -Depth 6 -Compress
        $audit = Invoke-RestMethod -Method Post -Uri "$baseUri/api/sessions/$($session.id)/datasets/$($dataset.dataset_id)/audit" -ContentType 'application/json' -Body $auditBody -TimeoutSec 30
        if (
            $audit.status -ne 'passed' -or
            -not $audit.audit_id -or
            $audit.audit_sha256 -notmatch '^[0-9a-f]{64}$' -or
            $audit.protocol_version -ne $protocol.version -or
            $audit.dataset_sha256 -ne $dataset.sha256 -or
            $audit.skill_id -ne 'tableone' -or
            $audit.run_spec_sha256 -notmatch '^[0-9a-f]{64}$'
        ) {
            $auditSummary = $audit | ConvertTo-Json -Depth 8 -Compress
            throw "Dataset did not pass the server audit: $auditSummary"
        }

        $approvalBody = @{
            skill_id = 'tableone'
            dataset_id = $dataset.dataset_id
            args = $analysisArgs
            expected_protocol_version = $protocol.version
            expected_audit_id = $audit.audit_id
            expected_audit_sha256 = $audit.audit_sha256
            audit_roles = $audit.roles
        } | ConvertTo-Json -Depth 8 -Compress
        $plan = Invoke-RestMethod -Method Post -Uri "$baseUri/api/sessions/$($session.id)/analysis-plans/approve" -ContentType 'application/json' -Body $approvalBody -TimeoutSec 30
        if (
            $plan.status -ne 'Approved' -or
            -not $plan.plan_id -or
            -not $plan.approval_id -or
            $plan.protocol_version -ne $protocol.version -or
            $plan.protocol_sha256 -ne $protocol.content_sha256 -or
            $plan.protocol_approval_id -ne $protocol.approval_id -or
            $plan.dataset_id -ne $dataset.dataset_id -or
            $plan.dataset_sha256 -ne $dataset.sha256 -or
            $plan.skill_id -ne 'tableone' -or
            $plan.run_spec_sha256 -ne $audit.run_spec_sha256 -or
            $plan.audit_id -ne $audit.audit_id -or
            $plan.audit_sha256 -ne $audit.audit_sha256
        ) {
            throw 'Server approval did not preserve the reviewed protocol, dataset, and audit bindings.'
        }

        $runBody = @{
            skill_id = 'tableone'
            dataset_id = $dataset.dataset_id
            args = $analysisArgs
            plan_id = $plan.plan_id
        } | ConvertTo-Json -Depth 6 -Compress
        $run = Invoke-RestMethod -Method Post -Uri "$baseUri/api/sessions/$($session.id)/run" -ContentType 'application/json' -Body $runBody -TimeoutSec 30
        if (
            $run.analysis.algorithm_id -ne 'tableone' -or
            $run.analysis.run_status -ne 'completed' -or
            $run.analysis.plan_id -ne $plan.plan_id
        ) {
            throw 'Table One did not return a completed run bound to the approved plan.'
        }

        $elapsed = [math]::Round(([DateTimeOffset]::Now - $startedAt).TotalSeconds, 2)
        return [pscustomobject]@{
            Round = $Round
            Port = $port
            Rows = $dataset.row_count
            DatasetSha256 = $dataset.sha256
            ProtocolVersion = $protocol.version
            ProtocolApprovalId = $protocol.approval_id
            ProtocolSha256 = $protocol.content_sha256
            AuditStatus = $audit.status
            AuditId = $audit.audit_id
            AuditSha256 = $audit.audit_sha256
            PlanId = $plan.plan_id
            PlanApprovalId = $plan.approval_id
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
    '- Path: every round used a fresh isolated APPDATA and executed start, health, session creation, CSV upload, server protocol approval, dataset audit, exact-audit plan approval, plan-bound Table One, and shutdown.',
    '- Criteria: every audit returned status=passed; every server-issued protocol/audit/plan binding matched; every run returned algorithm_id=tableone, run_status=completed, and the approved plan_id; every process was stopped.',
    '',
    '| Round | Port | Rows | Protocol | Audit | Plan | run_id | Seconds |',
    '|---:|---:|---:|---|---|---|---|---:|'
)
foreach ($result in $results) {
    $record += "| $($result.Round) | $($result.Port) | $($result.Rows) | v$($result.ProtocolVersion) / Approved | $($result.AuditStatus) | Approved | $($result.RunId) | $($result.Seconds) |"
}
$record += @('', '## Server-issued bindings')
foreach ($result in $results) {
    $record += @(
        '',
        "### Round $($result.Round)",
        '',
        "- Dataset: sha256=$($result.DatasetSha256)",
        "- Protocol: version=$($result.ProtocolVersion); approval_id=$($result.ProtocolApprovalId); sha256=$($result.ProtocolSha256)",
        "- Audit: status=$($result.AuditStatus); audit_id=$($result.AuditId); sha256=$($result.AuditSha256)",
        "- Plan: plan_id=$($result.PlanId); approval_id=$($result.PlanApprovalId); audit_id=$($result.AuditId)",
        "- Run: run_id=$($result.RunId); plan_id=$($result.PlanId)"
    )
}
$record += @('', '**Verdict: PASS**')
[System.IO.File]::WriteAllText($RecordPath, ($record -join "`n") + "`n", [System.Text.UTF8Encoding]::new($false))

$results | Format-Table Round, Port, Rows, ProtocolVersion, AuditStatus, PlanId, RunId, Seconds -AutoSize
Write-Host "[verify-demo-pack] three cold-start runs: PASS -> $RecordPath"
