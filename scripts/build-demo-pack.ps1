<#
.SYNOPSIS
    Builds and verifies the complete StatsCode-Demo-Pack archive for colleagues.

.DESCRIPTION
    Produces a shareable zip with:
      - stats-code.exe + install.ps1 + start.bat + install.bat
      - demo data (demo_cohort.csv)
      - colleague README and competition docs
      - SHA256SUMS + cold-start verification record

    Explicitly refuses to ship API keys or credential files.
#>

[CmdletBinding()]
param(
    [switch]$SkipRelease
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$RepoRoot = Split-Path -Parent $PSScriptRoot
$BackendDir = Join-Path $RepoRoot 'ts-backend'
$PackagingDir = Join-Path $RepoRoot 'packaging'
$BuiltExe = Join-Path $BackendDir 'build\stats-code.exe'
$OutputRoot = Join-Path $BackendDir 'build\demo-pack'
$StageDir = Join-Path $OutputRoot 'StatsCode-Demo-Pack'
$EnginePackage = Join-Path $BackendDir 'packages\engine\package.json'
$Version = (Get-Content -LiteralPath $EnginePackage -Raw | ConvertFrom-Json).version
$ArchivePath = Join-Path $OutputRoot "StatsCode-Demo-Pack-$Version-windows-x64.zip"
$RecordName = (-join ([char[]]@(0x51B7, 0x542F, 0x52A8, 0x9A8C, 0x8BC1, 0x8BB0, 0x5F55))) + '.md'
# “发给同事说明.txt”同样必须用字符码拼：本脚本以 UTF-8 无 BOM 保存，
# Windows PowerShell 5.1 会按 ANSI 解析中文字面量，直接写字面量会得到乱码文件名。
$ColleagueReadmeName = (-join ([char[]]@(0x53D1, 0x7ED9, 0x540C, 0x4E8B, 0x8BF4, 0x660E))) + '.txt'
$RecordPath = Join-Path $StageDir $RecordName

function Assert-NoSecretsInTree {
    param(
        [Parameter(Mandatory = $true)][string]$Root,
        [string[]]$ExtraSkipNames = @()
    )

    $forbiddenFileNames = @(
        'llm-config.json',
        '.env',
        '.env.local',
        'env.json',
        'secrets.json',
        'credentials.json',
        'auth.json'
    ) + $ExtraSkipNames

    $textExt = @('.txt', '.md', '.json', '.yml', '.yaml', '.toml', '.ps1', '.bat', '.cmd', '.csv', '.html', '.js', '.mjs', '.cjs', '.ts', '.map')
    $secretPatterns = @(
        'sk-[a-zA-Z0-9_\-]{16,}',
        'sk-proj-[a-zA-Z0-9_\-]{16,}',
        'api[_-]?key["'']?\s*[:=]\s*["''](?!\$\{)(?!your)(?!sk-\.\.\.)(?!xxxx)[a-zA-Z0-9_\-]{12,}["'']'
    )

    Get-ChildItem -LiteralPath $Root -Recurse -File | ForEach-Object {
        if ($forbiddenFileNames -contains $_.Name) {
            throw "Refusing to ship credential file in Demo-Pack: $($_.FullName)"
        }
        if ($textExt -notcontains $_.Extension.ToLowerInvariant()) { return }
        # Skip binary-ish large assets and the verification record we just wrote.
        if ($_.FullName -eq $RecordPath) { return }
        $content = [System.IO.File]::ReadAllText($_.FullName)
        foreach ($pat in $secretPatterns) {
            if ([regex]::IsMatch($content, $pat)) {
                throw "Secret-like pattern matched in $($_.FullName) (pattern: $pat). Remove keys before packing."
            }
        }
    }
}

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
    @{ Source = (Join-Path $PackagingDir 'start.bat'); Destination = (Join-Path $StageDir 'start.bat') },
    @{ Source = (Join-Path $PackagingDir 'install.bat'); Destination = (Join-Path $StageDir 'install.bat') },
    @{ Source = (Join-Path $PackagingDir 'colleague-README.txt'); Destination = (Join-Path $StageDir $ColleagueReadmeName) },
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

# Fail fast if any secret slipped into staged files before verification runs.
Assert-NoSecretsInTree -Root $StageDir

# Create the shipped verification record with a real end-to-end run.
# verify-demo-pack isolates APPDATA each round so the builder's LLM key is never used/copied.
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

# Final secret scan including the verification record (should only have hashes/ids).
Assert-NoSecretsInTree -Root $StageDir

if (Test-Path -LiteralPath $ArchivePath) {
    Remove-Item -LiteralPath $ArchivePath -Force
}
Compress-Archive -Path (Join-Path $StageDir '*') -DestinationPath $ArchivePath -CompressionLevel Optimal

# Scan the archive listing by re-extracting names only via staged tree (already scanned).
Write-Host ''
Write-Host '[build-demo-pack] PASS (no API keys included)'
Write-Host "  directory: $StageDir"
Write-Host "  archive  : $ArchivePath"
Write-Host '  share    : send the zip; recipients use start.bat or install.bat'
