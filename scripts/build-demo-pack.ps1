<#
.SYNOPSIS
    Builds and verifies the complete StatsCode-Demo-Pack archive.
#>

[CmdletBinding()]
param(
    [switch]$SkipRelease
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$RepoRoot = Split-Path -Parent $PSScriptRoot
$BackendDir = Join-Path $RepoRoot 'ts-backend'
$BuiltExe = Join-Path $BackendDir 'build\stats-code.exe'
$OutputRoot = Join-Path $BackendDir 'build\demo-pack'
$StageDir = Join-Path $OutputRoot 'StatsCode-Demo-Pack'
$EnginePackage = Join-Path $BackendDir 'packages\engine\package.json'
$Version = (Get-Content -LiteralPath $EnginePackage -Raw | ConvertFrom-Json).version
$ArchivePath = Join-Path $OutputRoot "StatsCode-Demo-Pack-$Version-windows-x64.zip"
$RecordName = (-join ([char[]]@(0x51B7, 0x542F, 0x52A8, 0x9A8C, 0x8BC1, 0x8BB0, 0x5F55))) + '.md'
$RecordPath = Join-Path $StageDir $RecordName

if (-not $SkipRelease) {
    & (Join-Path $PSScriptRoot 'release.ps1')
}
if (-not (Test-Path -LiteralPath $BuiltExe -PathType Leaf)) {
    throw "Build artifact is missing: $BuiltExe. Retry without -SkipRelease."
}

if (Test-Path -LiteralPath $StageDir) {
    Remove-Item -LiteralPath $StageDir -Recurse -Force
}
New-Item -ItemType Directory -Path (Join-Path $StageDir 'data') -Force | Out-Null

$copies = @(
    @{ Source = $BuiltExe; Destination = (Join-Path $StageDir 'stats-code.exe') },
    @{ Source = (Join-Path $RepoRoot 'install.ps1'); Destination = (Join-Path $StageDir 'install.ps1') },
    @{ Source = (Join-Path $RepoRoot 'web\public\demo_cohort.csv'); Destination = (Join-Path $StageDir 'data\demo_cohort.csv') },
    @{ Source = (Join-Path $PSScriptRoot 'verify-demo-pack.ps1'); Destination = (Join-Path $StageDir 'verify-demo-pack.ps1') }
)
foreach ($copy in $copies) {
    if (-not (Test-Path -LiteralPath $copy.Source -PathType Leaf)) {
        throw "Demo-Pack source file is missing: $($copy.Source)"
    }
    Copy-Item -LiteralPath $copy.Source -Destination $copy.Destination -Force
}

$DocsDir = Join-Path $RepoRoot 'docs\competition'
Get-ChildItem -LiteralPath $DocsDir -File | ForEach-Object {
    Copy-Item -LiteralPath $_.FullName -Destination (Join-Path $StageDir $_.Name) -Force
}

# Create the shipped verification record with a real end-to-end run.
& (Join-Path $PSScriptRoot 'verify-demo-pack.ps1') -PackDir $StageDir -SkipChecksum

$sumsPath = Join-Path $StageDir 'SHA256SUMS.txt'
$staticFiles = Get-ChildItem -LiteralPath $StageDir -File -Recurse |
    Where-Object { $_.FullName -ne $RecordPath -and $_.FullName -ne $sumsPath } |
    Sort-Object FullName
$sumLines = foreach ($file in $staticFiles) {
    $relative = $file.FullName.Substring($StageDir.Length).TrimStart([char[]]'\/').Replace('\', '/')
    $hash = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    "$hash  $relative"
}
[System.IO.File]::WriteAllText($sumsPath, ($sumLines -join "`n") + "`n", [System.Text.UTF8Encoding]::new($false))

# Verify all checksums and run three more isolated cold starts before packing.
& (Join-Path $PSScriptRoot 'verify-demo-pack.ps1') -PackDir $StageDir

if (Test-Path -LiteralPath $ArchivePath) {
    Remove-Item -LiteralPath $ArchivePath -Force
}
Compress-Archive -Path (Join-Path $StageDir '*') -DestinationPath $ArchivePath -CompressionLevel Optimal

Write-Host ''
Write-Host '[build-demo-pack] PASS'
Write-Host "  directory: $StageDir"
Write-Host "  archive  : $ArchivePath"

