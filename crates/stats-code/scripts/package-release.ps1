param(
    [string]$Configuration = "release",
    [string]$OutputRoot = "target/stats-code-release",
    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"
$RepoRoot = Resolve-Path (Join-Path $PSScriptRoot "..\..\..")
$PackageRoot = Join-Path $RepoRoot $OutputRoot
$Metadata = cargo metadata --no-deps --format-version 1 | ConvertFrom-Json
$StatsPackage = $Metadata.packages | Where-Object { $_.name -eq "stats-code" } | Select-Object -First 1
if (-not $StatsPackage) {
    throw "Could not find stats-code package metadata."
}

$Version = $StatsPackage.version
$TargetTriple = if ($IsWindows -or $env:OS -eq "Windows_NT") { "windows-x64" } else { "portable" }
$PackageName = "stats-code-$Version-$TargetTriple"
$StageDir = Join-Path $PackageRoot $PackageName
$ArchivePath = Join-Path $PackageRoot "$PackageName.zip"
$BinaryName = if ($IsWindows -or $env:OS -eq "Windows_NT") { "stats-code.exe" } else { "stats-code" }
$BinaryPath = Join-Path $RepoRoot "target\$Configuration\$BinaryName"

if (-not $SkipBuild) {
    $BuildArgs = @("build", "--locked", "-p", "stats-code")
    if ($Configuration -eq "release") {
        $BuildArgs += "--release"
    } elseif ($Configuration -ne "debug") {
        throw "Unsupported configuration '$Configuration'. Use 'release' or 'debug'."
    }
    cargo @BuildArgs
}

if (-not (Test-Path -LiteralPath $BinaryPath -PathType Leaf)) {
    throw "Binary not found at $BinaryPath"
}

New-Item -ItemType Directory -Force -Path $PackageRoot | Out-Null
if (Test-Path -LiteralPath $StageDir) {
    Remove-Item -LiteralPath $StageDir -Recurse -Force
}
New-Item -ItemType Directory -Force -Path $StageDir | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $StageDir "examples\data") | Out-Null

Copy-Item -LiteralPath $BinaryPath -Destination (Join-Path $StageDir $BinaryName) -Force
Copy-Item -LiteralPath (Join-Path $RepoRoot "crates\stats-code\README.md") -Destination (Join-Path $StageDir "README.md") -Force
Copy-Item -LiteralPath (Join-Path $RepoRoot "crates\stats-code\examples\analysis.example.yaml") -Destination (Join-Path $StageDir "examples\analysis.example.yaml") -Force
Copy-Item -LiteralPath (Join-Path $RepoRoot "crates\stats-code\examples\data\demo_cohort.csv") -Destination (Join-Path $StageDir "examples\data\demo_cohort.csv") -Force
Copy-Item -LiteralPath (Join-Path $RepoRoot "crates\stats-code\examples\data\demo_cohort.dictionary.csv") -Destination (Join-Path $StageDir "examples\data\demo_cohort.dictionary.csv") -Force

$InstallText = @"
Stats Code $Version

1. Put $BinaryName somewhere on PATH, or run it from this folder.
2. Check the local install:
   .\$BinaryName doctor
3. Try the bundled demo:
   .\$BinaryName init demo-project
   cd demo-project
   ..\$BinaryName check analysis.yaml
   ..\$BinaryName workflow run analysis.yaml --out stats-code-artifacts --no-chat --allow-unenforced-survey --allow-unenforced-privacy --allow-warnings
   ..\$BinaryName report verify stats-code-artifacts

The demo data are synthetic and are only for workflow smoke testing.
"@
Set-Content -LiteralPath (Join-Path $StageDir "INSTALL.txt") -Value $InstallText -Encoding UTF8

$Quickstart = @"
# Stats Code Quickstart

Use this package for local reproducible epidemiology/statistics workflows.

## First check

````powershell
.\$BinaryName doctor
````

## Demo workflow

````powershell
.\$BinaryName init demo-project
cd demo-project
..\$BinaryName check analysis.yaml
..\$BinaryName workflow run analysis.yaml --out stats-code-artifacts --no-chat --allow-unenforced-survey --allow-unenforced-privacy --allow-warnings
..\$BinaryName report verify stats-code-artifacts
````

Open `stats-code-artifacts\report\report.md` for the human-readable report and
`stats-code-artifacts\audit\evidence-index.json` for the evidence trail.
"@
Set-Content -LiteralPath (Join-Path $StageDir "QUICKSTART.md") -Value $Quickstart -Encoding UTF8

if (Test-Path -LiteralPath $ArchivePath) {
    Remove-Item -LiteralPath $ArchivePath -Force
}
Compress-Archive -LiteralPath $StageDir -DestinationPath $ArchivePath -Force

$ChecksumPath = Join-Path $PackageRoot "SHA256SUMS.txt"
$Hashes = @(
    Get-FileHash -Algorithm SHA256 -LiteralPath $ArchivePath
    Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $StageDir $BinaryName)
)
$ChecksumLines = $Hashes | ForEach-Object {
    "$($_.Hash.ToLowerInvariant())  $([IO.Path]::GetFileName($_.Path))"
}
Set-Content -LiteralPath $ChecksumPath -Value $ChecksumLines -Encoding ASCII

[PSCustomObject]@{
    status = "ok"
    package = $StageDir
    archive = $ArchivePath
    checksums = $ChecksumPath
} | ConvertTo-Json
