//! Feature: single-command-launcher
//!
//! Release packaging helpers shared between the Rust test fixtures and
//! the PowerShell `release.ps1` (Task 12.1). The single source of truth for
//! the Distribution_Archive naming convention defined in Requirement 13.1.
//!
//! The PowerShell mirror lives at `scripts/lib/archive-name.ps1` and must
//! produce a byte-identical filename for the same input version string.

/// Returns the Distribution_Archive filename for the given version.
///
/// Template: `stats-code-{version}-windows-x64.zip`.
///
/// This helper performs no validation on `version`; callers (e.g. the
/// release script) are responsible for supplying a value that matches the
/// `crates/stats-code` Cargo version.
pub fn archive_name(version: &str) -> String {
    format!("stats-code-{version}-windows-x64.zip")
}

#[cfg(test)]
mod tests {
    use super::*;

    // Feature: single-command-launcher, Property 13 fixture:
    //   archive_name(v) == format!("stats-code-{v}-windows-x64.zip").
    // This unit test pins the template for an example version and acts as
    // the unit-test fixture referenced by Task 12.2; the proptest
    // counterpart lives in Task 12.3.
    #[test]
    fn archive_name_formats_template_for_example_version() {
        assert_eq!(
            archive_name("0.1.0"),
            "stats-code-0.1.0-windows-x64.zip"
        );
    }

    #[test]
    fn archive_name_preserves_arbitrary_version_string() {
        // The template must apply verbatim to any version-like input,
        // including pre-release suffixes commonly produced by SemVer.
        assert_eq!(
            archive_name("1.2.3-rc.4+build.5"),
            "stats-code-1.2.3-rc.4+build.5-windows-x64.zip"
        );
    }
}
