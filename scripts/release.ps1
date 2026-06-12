<#
.SYNOPSIS
    Builds the single-file stats-code.exe (Node SEA) and assembles the
    Distribution_Archive: stats-code-<version>-windows-x64.zip.

.DESCRIPTION
    TypeScript-backend release pipeline (replaces the retired cargo build):
        1. Build the frontend (web/dist, embedded into the exe as SEA assets).
        2. Build the backend (embed templates/matrix + tsc project references).
        3. Produce stats-code.exe via Node SEA (npm run sea) and smoke-test it.
        4. Stage stats-code.exe + install.ps1 (+ README.md if present),
           write SHA256SUMS.txt, pack everything flat into the zip.

    Version source of truth: ts-backend/packages/engine/package.json.
    Archive naming: scripts/lib/archive-name.ps1::archive_name.

.NOTES
    Run from anywhere; paths resolve relative to the repo root.
    Requires Node.js >= 22 and installed npm dependencies (npm ci) in
    both web/ and ts-backend/.
#>

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$RepoRoot = Split-Path -Parent $PSScriptRoot
. (Join-Path $PSScriptRoot 'lib/archive-name.ps1')

$WebDir        = Join-Path $RepoRoot 'web'
$BackendDir    = Join-Path $RepoRoot 'ts-backend'
$EnginePkg     = Join-Path $BackendDir 'packages/engine/package.json'
$BuiltExe      = Join-Path $BackendDir 'build/stats-code.exe'
$InstallScript = Join-Path $RepoRoot 'install.ps1'
$ReadmeFile    = Join-Path $RepoRoot 'README.md'
$StageDir      = Join-Path $BackendDir 'build/release/stage'
$OutDir        = Join-Path $BackendDir 'build/release'

function Get-Sha256Lower {
    param([Parameter(Mandatory = $true)][string]$Path)
    (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Invoke-Npm {
    param(
        [Parameter(Mandatory = $true)][string]$WorkingDir,
        [Parameter(Mandatory = $true)][string[]]$Args
    )
    Push-Location $WorkingDir
    try {
        & npm.cmd @Args
        if ($LASTEXITCODE -ne 0) {
            throw "npm $($Args -join ' ') failed in $WorkingDir (exit $LASTEXITCODE)."
        }
    } finally {
        Pop-Location
    }
}

# ---------------------------------------------------------------------------
# Version + archive name
# ---------------------------------------------------------------------------

$Version = (Get-Content -LiteralPath $EnginePkg -Raw | ConvertFrom-Json).version
if (-not $Version) { throw "could not read version from $EnginePkg." }
$ArchiveName = archive_name -Version $Version
$ArchivePath = Join-Path $OutDir $ArchiveName

Write-Host "stats-code release $Version -> $ArchiveName"

# ---------------------------------------------------------------------------
# Step 1 - frontend build (embedded into the exe)
# ---------------------------------------------------------------------------

Write-Host ''
Write-Host '[1/4] building frontend (web/dist)'
Invoke-Npm -WorkingDir $WebDir -Args @('run', 'build')

# ---------------------------------------------------------------------------
# Step 2 - backend build + SEA exe + smoke test
# ---------------------------------------------------------------------------

Write-Host ''
Write-Host '[2/4] building backend + single-file exe (Node SEA)'
Invoke-Npm -WorkingDir $BackendDir -Args @('run', 'build')
Invoke-Npm -WorkingDir $BackendDir -Args @('run', 'sea')
if (-not (Test-Path -LiteralPath $BuiltExe -PathType Leaf)) {
    throw "SEA build reported success but '$BuiltExe' is missing."
}
Invoke-Npm -WorkingDir $BackendDir -Args @('run', 'smoke')

# ---------------------------------------------------------------------------
# Step 3 - stage files and write SHA256SUMS.txt
# ---------------------------------------------------------------------------

Write-Host ''
Write-Host "[3/4] staging files into $StageDir"

if (Test-Path -LiteralPath $StageDir) {
    Remove-Item -LiteralPath $StageDir -Recurse -Force
}
New-Item -ItemType Directory -Path $StageDir -Force | Out-Null

$StagedExe     = Join-Path $StageDir 'stats-code.exe'
$StagedInstall = Join-Path $StageDir 'install.ps1'
$StagedSums    = Join-Path $StageDir 'SHA256SUMS.txt'

Copy-Item -LiteralPath $BuiltExe      -Destination $StagedExe     -Force
Copy-Item -LiteralPath $InstallScript -Destination $StagedInstall -Force

$itemsToZip = @($StagedExe, $StagedInstall, $StagedSums)
if (Test-Path -LiteralPath $ReadmeFile -PathType Leaf) {
    $StagedReadme = Join-Path $StageDir 'README.md'
    Copy-Item -LiteralPath $ReadmeFile -Destination $StagedReadme -Force
    $itemsToZip += $StagedReadme
} else {
    Write-Host '  note: README.md not found at repo root; archive ships without it.'
}

# GNU coreutils sha256sum format (two-space separator, LF, trailing newline)
# so `sha256sum -c SHA256SUMS.txt` verifies the extracted archive in one shot.
$sumsLines = @(
    "$(Get-Sha256Lower -Path $StagedExe)  stats-code.exe",
    "$(Get-Sha256Lower -Path $StagedInstall)  install.ps1"
)
$sumsContent = ($sumsLines -join "`n") + "`n"
[System.IO.File]::WriteAllText($StagedSums, $sumsContent, [System.Text.UTF8Encoding]::new($false))
$sumsLines | ForEach-Object { Write-Host "  $_" }

# ---------------------------------------------------------------------------
# Step 4 - pack the Distribution_Archive (flat, no nested folder)
# ---------------------------------------------------------------------------

Write-Host ''
Write-Host "[4/4] writing $ArchivePath"

if (Test-Path -LiteralPath $ArchivePath) {
    Remove-Item -LiteralPath $ArchivePath -Force
}
Compress-Archive -Path $itemsToZip -DestinationPath $ArchivePath -Force

Write-Host ''
Write-Host 'Release build complete.'
Write-Host "  version : $Version"
Write-Host "  archive : $ArchivePath"
Write-Host "  sums    : $StagedSums"
