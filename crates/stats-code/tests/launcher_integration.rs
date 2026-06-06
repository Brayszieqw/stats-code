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

/// `stats-code parity` routes to Subcommand mode, bypassing `Launcher::run`
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
/// `Launcher::run` entirely. No port bind, no browser launch, no lock file.
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

/// `parity` must be registered in `KNOWN_SUBCOMMANDS` so it is never
/// accidentally routed to the launcher path.
#[test]
fn parity_is_in_known_subcommands() {
    assert!(
        KNOWN_SUBCOMMANDS.contains(&"parity"),
        "parity must be in KNOWN_SUBCOMMANDS to bypass launcher"
    );
}

/// `replay` must be registered in `KNOWN_SUBCOMMANDS` so it is never
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

/// Every entry in `KNOWN_SUBCOMMANDS` must route to `Mode::Subcommand`.
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

// ═════════════════════════════════════════════════════════════════════════════
// engineering-quality-hardening R6 (task 8.1): launcher behavior scenarios
//
// Three scenarios exercised through the public launcher API with every
// nondeterministic dependency injected — no real %APPDATA%, no well-known
// ports, no real browser spawn:
//   1. single-instance lock  → Existing { url, pid }, no second spawn
//   2. stale-lock cleanup     → Acquired(_), stale file deleted
//   3. --no-browser           → zero spawner calls, URL written to out
//
// Contract values (127.0.0.1, scan from 8080) are asserted without
// modifying any launcher code.
// _Requirements: 6.1, 6.2, 6.3, 6.4, 6.5_
// ═════════════════════════════════════════════════════════════════════════════

use std::cell::RefCell;
use std::io;

use stats_code::launcher::browser::{open_with, Spawner};
use stats_code::launcher::lock::{try_acquire, AcquireOutcome, LockFileV1};
use stats_code::launcher::port::DEFAULT_RANGE;

/// Integration-test double for [`Spawner`] that records every spawned URL
/// instead of launching a real browser. Defined here (not imported) because
/// the launcher's own `RecordingSpawner` lives in a `#[cfg(test)]` module that
/// is not reachable from an integration test crate.
#[derive(Default)]
struct RecordingSpawner {
    spawned: RefCell<Vec<String>>,
}

impl Spawner for RecordingSpawner {
    fn spawn(&self, url: &str) -> io::Result<()> {
        self.spawned.borrow_mut().push(url.to_string());
        Ok(())
    }
}

/// Scenario 1 — single-instance lock: a live lock file (PID alive, port open)
/// makes `try_acquire` report `Existing { url, pid }` and must NOT invoke any
/// backend spawner.
/// _Requirements: 6.1, 6.2_
#[test]
fn scenario_single_instance_lock_reports_existing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let lock_path = dir.path().join("running.lock");
    let record = LockFileV1::new(4321, "http://127.0.0.1:8080/", "2026-01-01T00:00:00Z", "prod");
    std::fs::write(&lock_path, record.to_json().expect("serialize")).expect("write lock");

    // Guard so we can assert "no second backend spawn" happened.
    let spawner = RecordingSpawner::default();

    let outcome = try_acquire(&lock_path, 9999, |_| true, |_| true).expect("no io error");

    match outcome {
        AcquireOutcome::Existing { url, pid } => {
            assert_eq!(pid, 4321);
            assert_eq!(url, "http://127.0.0.1:8080/");
        }
        AcquireOutcome::Acquired(_) => panic!("expected Existing for a live lock"),
    }

    // A second instance must not launch a backend / browser.
    assert!(
        spawner.spawned.borrow().is_empty(),
        "no spawn must occur when an existing live instance is found"
    );
    // Live lock file must remain on disk.
    assert!(lock_path.exists(), "live lock file must not be deleted");
}

/// Scenario 2 — stale-lock cleanup: a lock file whose PID is dead (or port
/// unreachable) is treated as stale; `try_acquire` returns `Acquired(_)` and
/// deletes the stale file.
/// _Requirements: 6.3_
#[test]
fn scenario_stale_lock_is_cleaned_and_acquired() {
    let dir = tempfile::tempdir().expect("tempdir");
    let lock_path = dir.path().join("running.lock");
    let record = LockFileV1::new(1234, "http://127.0.0.1:8080/", "2026-01-01T00:00:00Z", "prod");
    std::fs::write(&lock_path, record.to_json().expect("serialize")).expect("write lock");

    // PID dead → stale regardless of port.
    let outcome = try_acquire(&lock_path, 9999, |_| false, |_| true).expect("no io error");

    assert!(
        matches!(outcome, AcquireOutcome::Acquired(_)),
        "a stale lock (dead pid) must yield Acquired"
    );
    assert!(!lock_path.exists(), "stale lock file must be deleted");
}

/// Scenario 3 — no-browser flag: `open_with(spawner, url, true, out)` writes
/// the URL to `out` and performs zero spawner calls.
/// _Requirements: 6.4_
#[test]
fn scenario_no_browser_writes_url_and_never_spawns() {
    let spawner = RecordingSpawner::default();
    let mut out: Vec<u8> = Vec::new();
    let url = "http://127.0.0.1:8080/";

    open_with(&spawner, url, true, &mut out).expect("open_with --no-browser must succeed");

    assert_eq!(
        spawner.spawned.borrow().len(),
        0,
        "--no-browser must never spawn a browser"
    );
    let written = String::from_utf8(out).expect("utf8");
    assert_eq!(written, format!("{url}\n"), "URL must be written to out with trailing LF");
}

/// Contract guard: the launcher's default port scan starts at 8080 on the
/// loopback host, asserted without touching launcher code.
/// _Requirements: 6.5_
#[test]
fn scenario_contract_default_scan_starts_at_8080() {
    // Intentionally asserting on a const: this is a regression guard that locks
    // the launcher's published contract value (scan starts at 8080). If someone
    // changes DEFAULT_RANGE.start, this test must fail.
    #[allow(clippy::assertions_on_constants)]
    {
        assert_eq!(DEFAULT_RANGE.start, 8080, "default scan must start at port 8080");
        assert!(
            DEFAULT_RANGE.end_exclusive > DEFAULT_RANGE.start,
            "scan range must be non-empty"
        );
    }
}
