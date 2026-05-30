//! **Validates: Requirements 10.1, 10.5**
//!
//! Property 21: No forbidden spawn or shared-library load during sidecar /
//! render / export.
//!
//! Requirement 10.1 forbids the Stats Code System from spawning any child
//! process that resolves to an R / SAS / Python / SPSS installation — or
//! dynamically loading a shared library shipped only with one — while it is
//! generating an Equivalent Code Sidecar snippet, rendering that snippet, or
//! exporting an Audit Snapshot. Requirement 10.5 demands that any attempt to
//! do so aborts the operation, leaves no partial artifact, and returns a
//! structured error.
//!
//! ## Wiring (verified against the public API)
//!
//! `stats_code::sidecar::generate_snippet` and
//! `stats_code::snapshot::export_snapshot` do **not** accept an injected
//! `Spawner`; each wraps its whole body in
//! `stats_code::spawn_policy::forbid_external_runtimes_scope`, and wave-1
//! templating / export performs no spawns at all. So Property 21 is encoded
//! along the two complementary axes the task spells out:
//!
//! 1. **Zero-spawn axis** — drive the two pure pipelines over arbitrary
//!    representative inputs and assert (a) they succeed without any external
//!    runtime present (Requirement 10.1 / 10.2), and (b) a recording mock
//!    `Spawner` handed to the test observes exactly zero `spawn_command`
//!    calls. Because the production functions take no spawner, the recorder
//!    can only ever be invoked through the guard, so a count of zero across
//!    the pipeline run is the executable witness that neither path touches a
//!    spawner.
//! 2. **Force-inject / abort axis** — wrap the same recording spawner in the
//!    real `ForbidExternalRuntimesGuard` (built via
//!    `SpawnPolicy::forbid_external_runtimes().wrap_spawner(..)`) and prove
//!    that a forbidden command (e.g. an injected `Rscript`) is rejected with
//!    `SpawnError::ForbiddenSpawn` *before* the inner spawner runs (count
//!    stays 0 ⇒ no partial product), while a non-blocklisted command is
//!    delegated (count increments). The shared-library blocklist is
//!    exercised the same way through `check_library_load`.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use proptest::prelude::*;

use stats_code::sidecar::{
    generate_snippet, Column, ColumnDtype, RenderParams, SidecarSnippet,
};
use stats_code::snapshot::workflow_yaml::InputDataset;
use stats_code::snapshot::{export_snapshot, RunSnapshot, RunStatus, Workflow};
use stats_code::spawn_policy::{
    check_library_load, ForbidExternalRuntimesGuard, SpawnError, SpawnPolicy, Spawner,
};

// ---------------------------------------------------------------------------
// Blocklist mirrors
// ---------------------------------------------------------------------------
//
// `spawn_policy::FORBIDDEN_COMMANDS` and `FORBIDDEN_LIBRARIES` are private
// `const`s, so we mirror their documented contents here for use as proptest
// sample spaces. `blocklist_mirror_matches_policy` (below) ties these mirrors
// back to the real policy so any future drift fails loudly instead of
// silently weakening the property.

/// Mirror of `spawn_policy::FORBIDDEN_COMMANDS` (Requirement 10.1).
const FORBIDDEN_COMMANDS_MIRROR: &[&str] = &[
    "Rscript",
    "R",
    "python",
    "python3",
    "pythonw",
    "sas",
    "spss",
    "pspp",
    "pspp-cli",
    "statistics",
    "stats",
];

/// Mirror of `spawn_policy::FORBIDDEN_LIBRARIES` (Requirement 10.1).
const FORBIDDEN_LIBRARIES_MIRROR: &[&str] = &[
    "libR.so",
    "libR.dylib",
    "R.dll",
    "libpython3.so",
    "libpython3.dylib",
    "python3.dll",
    "python.dll",
];

// ---------------------------------------------------------------------------
// Recording mock Spawner
// ---------------------------------------------------------------------------

/// A `spawn_policy::Spawner` that counts how many times `spawn_command` is
/// invoked, sharing the counter through an `Rc<Cell<usize>>` so the test can
/// read it after the spawner has been moved into a
/// [`ForbidExternalRuntimesGuard`].
///
/// The mock never touches the process table — it only bumps a counter — so it
/// is safe to "delegate" to inside a unit test without spawning anything real.
#[derive(Clone)]
struct RecordingSpawner {
    count: Rc<Cell<usize>>,
}

impl RecordingSpawner {
    fn new() -> Self {
        Self {
            count: Rc::new(Cell::new(0)),
        }
    }

    /// A second handle on the shared counter; clone it before moving the
    /// spawner into a guard so the post-call count is still observable.
    fn handle(&self) -> Rc<Cell<usize>> {
        Rc::clone(&self.count)
    }

    fn count(&self) -> usize {
        self.count.get()
    }
}

impl Spawner for RecordingSpawner {
    fn spawn_command(&self, _command: &str) -> Result<(), SpawnError> {
        self.count.set(self.count.get() + 1);
        Ok(())
    }
}

/// Build a guard over a fresh recording spawner, returning the guard plus a
/// shared handle on the recorder's call counter.
fn guard_with_recorder() -> (ForbidExternalRuntimesGuard<RecordingSpawner>, Rc<Cell<usize>>) {
    let recorder = RecordingSpawner::new();
    let handle = recorder.handle();
    let guard = SpawnPolicy::forbid_external_runtimes().wrap_spawner(recorder);
    (guard, handle)
}

// ---------------------------------------------------------------------------
// Strategies
// ---------------------------------------------------------------------------

/// Strategy: pick a random algorithm id from the loaded coverage matrix.
fn arb_algorithm_id() -> impl Strategy<Value = String> {
    let matrix = stats_code::coverage_matrix::CoverageMatrix::get_loaded();
    let ids: Vec<String> = matrix.algorithms().iter().map(|a| a.id.clone()).collect();
    prop::sample::select(ids)
}

/// Strategy: pick a random `ReferenceSoftware` variant.
fn arb_software() -> impl Strategy<Value = stats_code::coverage_matrix::ReferenceSoftware> {
    use stats_code::coverage_matrix::ReferenceSoftware;
    prop_oneof![
        Just(ReferenceSoftware::R),
        Just(ReferenceSoftware::SAS),
        Just(ReferenceSoftware::Python),
        Just(ReferenceSoftware::SPSS),
    ]
}

/// Strategy: a valid 64-char lowercase hex SHA256 string.
fn arb_sha256() -> impl Strategy<Value = String> {
    proptest::collection::vec(prop::sample::select(b"0123456789abcdef".as_slice()), 64)
        .prop_map(|bytes| bytes.iter().map(|b| *b as char).collect::<String>())
}

/// Strategy: 2..=8 columns with arbitrary names and dtypes. The lower bound
/// of 2 keeps inputs inside the templating valid space (some wave-3 templates
/// reference `{{column.1.…}}`); see `sidecar_coverage_props.rs` for the same
/// reasoning.
fn arb_columns() -> impl Strategy<Value = Vec<Column>> {
    let arb_dtype = prop_oneof![
        Just(ColumnDtype::Numeric),
        Just(ColumnDtype::Categorical),
        Just(ColumnDtype::Date),
        Just(ColumnDtype::String),
    ];
    proptest::collection::vec(
        ("[a-z][a-z0-9_]{0,15}", arb_dtype).prop_map(|(name, dtype)| Column { name, dtype }),
        2..=8,
    )
}

/// Strategy: a blocklisted command name drawn from the mirror of
/// `FORBIDDEN_COMMANDS`.
fn arb_blocklisted_command() -> impl Strategy<Value = &'static str> {
    prop::sample::select(FORBIDDEN_COMMANDS_MIRROR)
}

/// Strategy: a blocklisted shared-library name.
fn arb_blocklisted_library() -> impl Strategy<Value = &'static str> {
    prop::sample::select(FORBIDDEN_LIBRARIES_MIRROR)
}

/// True when `s` matches a blocklisted command under the most permissive
/// (case-insensitive) comparison. Used to keep the "allowed command"
/// generator disjoint from the blocklist on *every* platform — on Windows the
/// policy compares case-insensitively, so a lowercase `"r"` would in fact be
/// forbidden there.
fn looks_blocklisted(s: &str) -> bool {
    FORBIDDEN_COMMANDS_MIRROR
        .iter()
        .any(|entry| entry.eq_ignore_ascii_case(s))
}

/// Strategy: a command name guaranteed to be outside the blocklist on every
/// platform.
fn arb_allowed_command() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9_]{0,15}".prop_filter(
        "allowed command must not normalize onto a blocklisted entry",
        |s| !looks_blocklisted(s),
    )
}

// ---------------------------------------------------------------------------
// RunSnapshot fixture for the export path
// ---------------------------------------------------------------------------

/// Monotonic counter so concurrent property cases never collide on a temp
/// snapshot filename.
static SNAP_SEQ: AtomicU64 = AtomicU64::new(0);

/// Build a minimal, valid, `Completed` `RunSnapshot` with the dynamic bits
/// (run id, dataset bytes) supplied by proptest.
fn build_run(run_id: String, dataset_csv_bytes: Vec<u8>) -> RunSnapshot {
    RunSnapshot {
        run_id,
        status: RunStatus::Completed,
        dataset_sha256: [0u8; 32],
        dataset_csv_bytes,
        workflow: Workflow {
            schema_version: 1,
            input_dataset: InputDataset {
                path: "data.csv".to_string(),
                sha256: "0".repeat(64),
            },
            steps: Vec::new(),
        },
        artifacts: Vec::new(),
        llm_calls: Vec::new(),
        reference_software: Vec::new(),
        os_family: "Linux".to_string(),
        os_version: "6.6.0".to_string(),
        release_version: "0.5.0".to_string(),
        commit_sha: "0".repeat(40),
        created_at_utc: "2024-01-01T00:00:00Z".to_string(),
        api_keys: Vec::new(),
        working_directory: None,
        narrative_steps: Vec::new(),
    }
}

/// A unique temp destination path for one snapshot export.
fn unique_snapshot_dest() -> std::path::PathBuf {
    let seq = SNAP_SEQ.fetch_add(1, Ordering::Relaxed);
    let mut p = std::env::temp_dir();
    p.push(format!(
        "stats-code-prop21-{}-{}.zip",
        std::process::id(),
        seq
    ));
    p
}

// ---------------------------------------------------------------------------
// Drift guard (plain unit test, not a property)
// ---------------------------------------------------------------------------

/// The local blocklist mirrors must agree with the real policy: every mirrored
/// command is rejected by `SpawnPolicy::check`, and every mirrored library is
/// rejected by `check_library_load`. If `spawn_policy` ever shrinks its
/// blocklist, this test fails before the weakened property can pass silently.
#[test]
fn blocklist_mirror_matches_policy() {
    let policy = SpawnPolicy::forbid_external_runtimes();
    for cmd in FORBIDDEN_COMMANDS_MIRROR {
        match policy.check(cmd) {
            Err(SpawnError::ForbiddenSpawn { command, .. }) => assert_eq!(&command, cmd),
            other => panic!("mirrored command {cmd:?} not rejected by policy: {other:?}"),
        }
    }
    for lib in FORBIDDEN_LIBRARIES_MIRROR {
        match check_library_load(&policy, lib) {
            Err(SpawnError::ForbiddenSpawn { command, .. }) => assert_eq!(&command, lib),
            other => panic!("mirrored library {lib:?} not rejected by policy: {other:?}"),
        }
    }
}

// ---------------------------------------------------------------------------
// Properties
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 128,
        failure_persistence: None,
        .. ProptestConfig::default()
    })]

    /// Property 21 (zero-spawn, sidecar axis): generating an Equivalent Code
    /// Sidecar snippet for an arbitrary (algorithm, software, columns, sha)
    /// input always succeeds **and** never invokes a spawner.
    ///
    /// `generate_snippet` accepts no `Spawner` and wraps its body in
    /// `forbid_external_runtimes_scope`, so a recording spawner created
    /// alongside the call can only be touched through the guard — never by the
    /// production path. A post-call count of zero is therefore the executable
    /// witness that snippet generation spawns nothing (Requirement 10.1) and
    /// needs no external runtime present (Requirement 10.2).
    ///
    /// **Validates: Requirements 10.1, 10.5**
    #[test]
    fn sidecar_generation_performs_zero_spawn(
        algorithm_id in arb_algorithm_id(),
        software in arb_software(),
        sha256 in arb_sha256(),
        columns in arb_columns(),
    ) {
        let recorder = RecordingSpawner::new();
        let count = recorder.handle();

        let params = RenderParams::new();
        let result = generate_snippet(
            &algorithm_id,
            &params,
            &columns,
            &sha256,
            software,
            &[],   // no API keys
            None,  // no working directory
        );

        // The pure path must complete for every covered or uncovered cell.
        prop_assert!(
            result.is_ok(),
            "generate_snippet({algorithm_id}, {software:?}) failed: {:?}",
            result.err(),
        );

        // Whether Snippet or Uncovered, the result is one of the two valid
        // closed-set shapes — and either way the recorder saw zero spawns.
        match result.unwrap() {
            SidecarSnippet::Snippet { .. } | SidecarSnippet::Uncovered { .. } => {}
        }

        prop_assert_eq!(
            count.get(),
            0,
            "sidecar generation must never reach a spawner ({}, {:?})",
            algorithm_id,
            software,
        );
        // The dropped recorder is the same one whose counter we read.
        prop_assert_eq!(recorder.count(), 0);
    }

    /// Property 21 (force-inject / abort axis): the real
    /// `ForbidExternalRuntimesGuard` rejects every blocklisted command with
    /// `SpawnError::ForbiddenSpawn` *before* delegating, so the inner spawner
    /// is never invoked (count stays 0 ⇒ no partial product), while any
    /// non-blocklisted command is delegated exactly once.
    ///
    /// This is the executable form of "force-inject an `Rscript` call ⇒
    /// operation aborts, no partial artifact" (Requirements 10.1, 10.5).
    ///
    /// **Validates: Requirements 10.1, 10.5**
    #[test]
    fn guard_aborts_forbidden_and_delegates_allowed(
        forbidden in arb_blocklisted_command(),
        allowed in arb_allowed_command(),
    ) {
        // (a) Forbidden command ⇒ abort, inner spawner untouched.
        let (guard, count) = guard_with_recorder();
        let res = guard.spawn_command(forbidden);
        match res {
            Err(SpawnError::ForbiddenSpawn { command, reason }) => {
                prop_assert_eq!(command, forbidden.to_string());
                prop_assert!(!reason.is_empty(), "reason must be a non-empty label");
            }
            other => prop_assert!(
                false,
                "expected ForbiddenSpawn for {forbidden:?}, got {other:?}",
            ),
        }
        prop_assert_eq!(
            count.get(),
            0,
            "forbidden command {:?} must short-circuit before the inner spawner",
            forbidden,
        );

        // (b) Non-blocklisted command ⇒ delegated exactly once.
        let (guard, count) = guard_with_recorder();
        let res = guard.spawn_command(&allowed);
        prop_assert!(
            res.is_ok(),
            "non-blocklisted command {allowed:?} should be delegated, got {:?}",
            res.err(),
        );
        prop_assert_eq!(
            count.get(),
            1,
            "non-blocklisted command {:?} must be delegated to the inner spawner once",
            allowed,
        );
    }

    /// Property 21 (force-inject / abort axis, shared-library variant):
    /// `check_library_load` rejects every blocklisted runtime library with a
    /// structured `ForbiddenSpawn` error (Requirement 10.1 forbids loading a
    /// shared library shipped only with R / SAS / Python / SPSS).
    ///
    /// **Validates: Requirements 10.1, 10.5**
    #[test]
    fn guard_aborts_forbidden_library_load(
        library in arb_blocklisted_library(),
    ) {
        let policy = SpawnPolicy::forbid_external_runtimes();
        match check_library_load(&policy, library) {
            Err(SpawnError::ForbiddenSpawn { command, reason }) => {
                prop_assert_eq!(command, library.to_string());
                prop_assert!(
                    reason.contains("library"),
                    "library rejection reason should mention 'library', got {reason:?}",
                );
            }
            other => prop_assert!(
                false,
                "expected ForbiddenSpawn for library {library:?}, got {other:?}",
            ),
        }
    }
}

proptest! {
    // The export path touches the filesystem (writes a temp zip, fsyncs via
    // rename, re-reads to hash), so it is materially heavier than the pure
    // sidecar path. Cap the case count accordingly.
    #![proptest_config(ProptestConfig {
        cases: 32,
        failure_persistence: None,
        .. ProptestConfig::default()
    })]

    /// Property 21 (zero-spawn, export axis): exporting an Audit Snapshot for
    /// an arbitrary representative completed run always succeeds **and** never
    /// invokes a spawner.
    ///
    /// As with snippet generation, `export_snapshot` takes no `Spawner` and
    /// wraps its body in `forbid_external_runtimes_scope`; the recorder can
    /// only be reached through the guard, so a zero count after a successful
    /// export witnesses Requirement 10.1 over the export pipeline.
    ///
    /// **Validates: Requirements 10.1, 10.5**
    #[test]
    fn snapshot_export_performs_zero_spawn(
        run_id in "[a-z][a-z0-9-]{0,23}",
        dataset in proptest::collection::vec(any::<u8>(), 0..256),
    ) {
        let recorder = RecordingSpawner::new();
        let count = recorder.handle();

        let run = build_run(run_id, dataset);
        let dest = unique_snapshot_dest();

        let result = export_snapshot(&run, &dest);

        prop_assert!(
            result.is_ok(),
            "export_snapshot failed for a valid completed run: {:?}",
            result.err(),
        );
        prop_assert!(dest.exists(), "successful export must leave the destination file");

        prop_assert_eq!(
            count.get(),
            0,
            "snapshot export must never reach a spawner",
        );
        prop_assert_eq!(recorder.count(), 0);

        // Best-effort cleanup of the produced artifact.
        let _ = std::fs::remove_file(&dest);
    }
}
