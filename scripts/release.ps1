<#
.SYNOPSIS
    Builds stats-code.exe in release/prod mode and assembles the
    Distribution_Archive for the single-command-launcher feature.

.DESCRIPTION
    Implements the Release_Script contract from the single-command-launcher
    spec (Requirements 13.1 - 13.5):

        1. Resolve the workspace package version from the root Cargo.toml
           (`[workspace.package].version`), with fallback to
           `crates/stats-code/Cargo.toml`.
        2. Build stats-code.exe in release mode without the `dev-vite`
           feature (prod = web/dist embedded via rust-embed).
        3. Stage `target/stats-code-release/` with stats-code.exe,
           install.ps1, README.md.
        4. Compute SHA-256 hashes for stats-code.exe and install.ps1,
           write `SHA256SUMS.txt` (each line: <lowercase-hex>  <filename>).
        5. Pack the four staged files into
           `stats-code-<version>-windows-x64.zip` at the same staging dir.

.NOTES
    Preconditions:
      - `install.ps1` MUST exist at the repository root.
      - `README.md` MUST exist at the repository root.
    Both are part of the Distribution_Archive contract (R13.2). The script
    fails early with a clear error if either is missing.

    Archive naming uses the shared helper
    `scripts/lib/archive-name.ps1::archive_name`, mirroring the Rust helper
    in `crates/stats-code/src/release.rs` so PowerShell + Rust agree on the
    template `stats-code-<version>-windows-x64.zip` (Property 13).

    SHA256SUMS.txt format mirrors the Linux `sha256sum` convention:
        <64-hex-lowercase><two-spaces><filename>
    so downstream consumers can verify with `sha256sum -c` (Property 14).
#>

[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# ---------------------------------------------------------------------------
# Paths & helpers
# ---------------------------------------------------------------------------

# scripts/release.ps1 lives one level under the repo root.
$RepoRoot      = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$WorkspaceToml = Join-Path $RepoRoot 'Cargo.toml'
$CrateToml     = Join-Path $RepoRoot 'crates/stats-code/Cargo.toml'
$InstallScript = Join-Path $RepoRoot 'install.ps1'
$ReadmeFile    = Join-Path $RepoRoot 'README.md'
$ReleaseExe    = Join-Path $RepoRoot 'target/release/stats-code.exe'
$StageDir      = Join-Path $RepoRoot 'target/stats-code-release'

. (Join-Path $PSScriptRoot 'lib/archive-name.ps1')

function Get-WorkspaceVersion {
    [OutputType([string])]
    param(
        [Parameter(Mandatory = $true)] [string]$WorkspaceTomlPath,
        [Parameter(Mandatory = $true)] [string]$CrateTomlPath
    )

    # Primary source: [workspace.package].version in the root Cargo.toml.
    if (Test-Path -LiteralPath $WorkspaceTomlPath -PathType Leaf) {
        $content = Get-Content -LiteralPath $WorkspaceTomlPath -Raw
        # Match the [workspace.package] table block, then the first version =
        # "..." inside it. Stop at the next [section] header or end of file.
        $blockMatch = [regex]::Match(
            $content,
            '(?ms)^\[workspace\.package\]\s*\r?\n(?<body>.*?)(?=^\[|\Z)'
        )
        if ($blockMatch.Success) {
            $verMatch = [regex]::Match(
                $blockMatch.Groups['body'].Value,
                '(?m)^\s*version\s*=\s*"(?<v>[^"]+)"'
            )
            if ($verMatch.Success) {
                return $verMatch.Groups['v'].Value
            }
        }
    }

    # Fallback: scan crates/stats-code/Cargo.toml for an explicit
    # `version = "..."` line under [package]. (The current crate inherits
    # from the workspace via `version.workspace = true`, so this branch is
    # only a safety net for configurations that pin a literal version.)
    if (Test-Path -LiteralPath $CrateTomlPath -PathType Leaf) {
        $content = Get-Content -LiteralPath $CrateTomlPath -Raw
        $blockMatch = [regex]::Match(
            $content,
            '(?ms)^\[package\]\s*\r?\n(?<body>.*?)(?=^\[|\Z)'
        )
        if ($blockMatch.Success) {
            $verMatch = [regex]::Match(
                $blockMatch.Groups['body'].Value,
                '(?m)^\s*version\s*=\s*"(?<v>[^"]+)"'
            )
            if ($verMatch.Success) {
                return $verMatch.Groups['v'].Value
            }
        }
    }

    throw "Unable to resolve crate version: neither [workspace.package].version in '$WorkspaceTomlPath' nor [package].version in '$CrateTomlPath' is a literal string."
}

function Get-Sha256Lower {
    [OutputType([string])]
    param([Parameter(Mandatory = $true)] [string]$Path)

    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

# ---------------------------------------------------------------------------
# Step 0 - Validate preconditions
# ---------------------------------------------------------------------------

if (-not (Test-Path -LiteralPath $InstallScript -PathType Leaf)) {
    throw "Precondition failed: '$InstallScript' is missing. Distribution_Archive (R13.2) requires install.ps1 at the repository root. Author it (task 13.1) before running release.ps1."
}

if (-not (Test-Path -LiteralPath $ReadmeFile -PathType Leaf)) {
    throw "Precondition failed: '$ReadmeFile' is missing. Distribution_Archive (R13.2) requires README.md at the repository root. Author it before running release.ps1."
}

$Version = Get-WorkspaceVersion -WorkspaceTomlPath $WorkspaceToml -CrateTomlPath $CrateToml
$ArchiveName = archive_name -Version $Version
$ArchivePath = Join-Path $StageDir $ArchiveName

Write-Host "stats-code release builder"
Write-Host "  repo root    : $RepoRoot"
Write-Host "  version      : $Version"
Write-Host "  staging dir  : $StageDir"
Write-Host "  archive name : $ArchiveName"

# ---------------------------------------------------------------------------
# Step 1 - cargo build --release -p stats-code (prod, no dev-vite)
# ---------------------------------------------------------------------------

Write-Host ''
Write-Host '[1/4] cargo build --release -p stats-code'
Push-Location $RepoRoot
try {
    & cargo build --release -p stats-code
    if ($LASTEXITCODE -ne 0) {
        throw "cargo build failed with exit code $LASTEXITCODE."
    }
}
finally {
    Pop-Location
}

if (-not (Test-Path -LiteralPath $ReleaseExe -PathType Leaf)) {
    throw "cargo build reported success but '$ReleaseExe' is missing."
}

# ---------------------------------------------------------------------------
# Step 2 - Stage target/stats-code-release/
# ---------------------------------------------------------------------------

Write-Host ''
Write-Host "[2/4] staging files into $StageDir"

# Wipe a previous run's stage so we never ship stale bytes.
if (Test-Path -LiteralPath $StageDir) {
    Remove-Item -LiteralPath $StageDir -Recurse -Force
}
New-Item -ItemType Directory -Path $StageDir -Force | Out-Null

$StagedExe     = Join-Path $StageDir 'stats-code.exe'
$StagedInstall = Join-Path $StageDir 'install.ps1'
$StagedReadme  = Join-Path $StageDir 'README.md'
$StagedSums    = Join-Path $StageDir 'SHA256SUMS.txt'

Copy-Item -LiteralPath $ReleaseExe    -Destination $StagedExe     -Force
Copy-Item -LiteralPath $InstallScript -Destination $StagedInstall -Force
Copy-Item -LiteralPath $ReadmeFile    -Destination $StagedReadme  -Force

# ---------------------------------------------------------------------------
# Step 3 - Compute SHA-256 hashes and write SHA256SUMS.txt
# ---------------------------------------------------------------------------

Write-Host ''
Write-Host '[3/4] computing SHA-256 for stats-code.exe and install.ps1'

$ExeHash     = Get-Sha256Lower -Path $StagedExe
$InstallHash = Get-Sha256Lower -Path $StagedInstall

# Two-space separator matches the GNU coreutils sha256sum format so that
# `sha256sum -c SHA256SUMS.txt` from inside the extracted archive verifies
# both files in one shot.
$sumsLines = @(
    "$ExeHash  stats-code.exe",
    "$InstallHash  install.ps1"
)
# Use UTF8 without BOM and LF line endings to keep the file portable; the
# trailing newline matches the sha256sum tool convention.
$sumsContent = ($sumsLines -join "`n") + "`n"
[System.IO.File]::WriteAllText($StagedSums, $sumsContent, [System.Text.UTF8Encoding]::new($false))

Write-Host "  stats-code.exe : $ExeHash"
Write-Host "  install.ps1    : $InstallHash"

# ---------------------------------------------------------------------------
# Step 4 - Pack Distribution_Archive
# ---------------------------------------------------------------------------

Write-Host ''
Write-Host "[4/4] writing $ArchivePath"

if (Test-Path -LiteralPath $ArchivePath) {
    Remove-Item -LiteralPath $ArchivePath -Force
}

# Pack only the four expected files, each at the archive root (no nested
# folder). Compress-Archive expands `*` into the staged file list and stores
# them with their leaf names, satisfying R13.2's "root directory contains
# and only contains" clause.
$itemsToZip = @(
    $StagedExe,
    $StagedInstall,
    $StagedReadme,
    $StagedSums
)
Compress-Archive -Path $itemsToZip -DestinationPath $ArchivePath -Force

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------

Write-Host ''
Write-Host 'Release build complete.'
Write-Host "  version : $Version"
Write-Host "  archive : $ArchivePath"
Write-Host "  sha256  : $StagedSums"
