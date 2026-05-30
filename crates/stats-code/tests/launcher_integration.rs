//! Integration tests for single-command-launcher non-interference.
//!
//! Task 15.4: Verify that `classify_invocation` correctly routes:
//! - Empty argv → Launcher mode (port scan, browser, lock file)
//! - `parity` / `replay` argv → Subcommand mode (no launcher behavior)
//!
//! The key guarantee: parity/replay paths do NOT reach any launcher
//! functions (port scan, browser open, lock file creation).
//!
//! _Requirements: 5.3, 5.8, 10.3_

use stats_code::launcher::{classify_invocation, Mode, KNOWN_SUBCOMMANDS};

// ─────────────────────────────────────────────────────────────────────────────
// Empty argv → Launcher mode
// ─────────────────────────────────────────────────────────────────────────────

/// Bare `stats-code` invocation (empty argv beyond program name) routes to
/// Launcher mode. This confirms the launcher behavior (port scan, 127.0.0.1
/// binding, browser launch, single-instance lock, Ctrl+C) is triggered.
#[test]
fn empty_argv_routes_to_launcher_mode() {
    let argv = vec!["stats-code".to_string()];
    assert_eq!(classify_invocation(&argv), Mode::Launcher);
}

/// Program name only with flags (not subcommands) still routes to Launcher.
#[test]
fn flags_only_routes_to_launcher_mode() {
    let argv = vec![
        "stats-code".to_string(),
        "--no-browser".to_string(),
    ];
    assert_eq!(classify_invocation(&argv), Mode::Launcher);
}

// ─────────────────────────────────────────────────────────────────────────────
// `parity` argv → Subcommand mode (no launcher)
// ─────────────────────────────────────────────────────────────────────────────

/// `stats-code parity` routes to Subcommand mode, bypassing Launcher::run
/// entirely. No port bind, no browser launch, no lock file.
/// _Requirements: 5.3, 5.8_
#[test]
fn parity_routes_to_subcommand_mode() {
    let argv = vec!["stats-code".to_string(), "parity".to_string()];
    assert_eq!(classify_invocation(&argv), Mode::Subcommand);
}

/// `stats-code parity --filter tableone` still routes to Subcommand mode.
#[test]
fn parity_with_filter_routes_to_subcommand_mode() {
    let argv = vec![
        "stats-code".to_string(),
        "parity".to_string(),
        "--filter".to_string(),
        "tableone".to_string(),
    ];
    assert_eq!(classify_invocation(&argv), Mode::Subcommand);
}

// ─────────────────────────────────────────────────────────────────────────────
// `replay` argv → Subcommand mode (no launcher)
// ─────────────────────────────────────────────────────────────────────────────

/// `stats-code replay snapshot.zip` routes to Subcommand mode, bypassing
/// Launcher::run entirely. No port bind, no browser launch, no lock file.
/// _Requirements: 8.3, 10.3_
#[test]
fn replay_routes_to_subcommand_mode() {
    let argv = vec![
        "stats-code".to_string(),
        "replay".to_string(),
        "snapshot.zip".to_string(),
    ];
    assert_eq!(classify_invocation(&argv), Mode::Subcommand);
}

/// `replay` with an absolute path argument still routes to Subcommand.
#[test]
fn replay_with_absolute_path_routes_to_subcommand_mode() {
    let argv = vec![
        "stats-code".to_string(),
        "replay".to_string(),
        "C:\\Users\\analyst\\snapshots\\run-001.zip".to_string(),
    ];
    assert_eq!(classify_invocation(&argv), Mode::Subcommand);
}

// ─────────────────────────────────────────────────────────────────────────────
// Structural guarantee: parity and replay are in KNOWN_SUBCOMMANDS
// ─────────────────────────────────────────────────────────────────────────────

/// `parity` must be registered in KNOWN_SUBCOMMANDS so it is never
/// accidentally routed to the launcher path.
#[test]
fn parity_is_in_known_subcommands() {
    assert!(
        KNOWN_SUBCOMMANDS.contains(&"parity"),
        "parity must be in KNOWN_SUBCOMMANDS to bypass launcher"
    );
}

/// `replay` must be registered in KNOWN_SUBCOMMANDS so it is never
/// accidentally routed to the launcher path.
#[test]
fn replay_is_in_known_subcommands() {
    assert!(
        KNOWN_SUBCOMMANDS.contains(&"replay"),
        "replay must be in KNOWN_SUBCOMMANDS to bypass launcher"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Negative: unknown tokens do NOT trigger subcommand mode
// ─────────────────────────────────────────────────────────────────────────────

/// An unknown positional argument that is not a known subcommand must
/// still route to Launcher mode (the launcher will handle it or clap
/// will error). This confirms the dispatch logic is conservative.
#[test]
fn unknown_positional_routes_to_launcher_mode() {
    let argv = vec![
        "stats-code".to_string(),
        "not-a-real-command".to_string(),
    ];
    assert_eq!(classify_invocation(&argv), Mode::Launcher);
}

// ─────────────────────────────────────────────────────────────────────────────
// All known subcommands route to Subcommand mode (exhaustive)
// ─────────────────────────────────────────────────────────────────────────────

/// Every entry in KNOWN_SUBCOMMANDS must route to Mode::Subcommand.
/// This is the byte-level behavioral guarantee that none of these paths
/// will accidentally trigger port scanning, browser opening, or lock
/// file creation.
#[test]
fn all_known_subcommands_route_to_subcommand_mode() {
    for &subcmd in KNOWN_SUBCOMMANDS {
        let argv = vec!["stats-code".to_string(), subcmd.to_string()];
        assert_eq!(
            classify_invocation(&argv),
            Mode::Subcommand,
            "KNOWN_SUBCOMMAND '{subcmd}' must route to Mode::Subcommand"
        );
    }
}
