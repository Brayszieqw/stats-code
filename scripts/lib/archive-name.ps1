# Feature: single-command-launcher
# Helper: archive_name
#
# Returns the distribution archive filename for a given version string.
# Template: stats-code-<version>-windows-x64.zip
#
# Mirrors the Rust helper in crates/stats-code/src/release.rs so that
# release.ps1 (Task 12.1) and the Rust property test (Task 12.3) agree
# byte-for-byte on the archive naming convention.
#
# Usage:
#   . "$PSScriptRoot/lib/archive-name.ps1"
#   $name = archive_name -Version "0.1.0"
#   # -> stats-code-0.1.0-windows-x64.zip

Set-StrictMode -Version Latest

function archive_name {
    [CmdletBinding()]
    [OutputType([string])]
    param(
        [Parameter(Mandatory = $true, Position = 0)]
        [ValidateNotNullOrEmpty()]
        [string]$Version
    )

    return "stats-code-$Version-windows-x64.zip"
}
