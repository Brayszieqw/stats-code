# Feature: single-command-launcher
# Helper: archive_name
#
# Returns the distribution archive filename for a given version string.
# Template: stats-code-<version>-windows-x64.zip
#
# Single source of the archive naming convention, shared by release.ps1
# and ts-backend/scripts/release-meta.mjs (which derives the same name
# from packages/engine/package.json).
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
