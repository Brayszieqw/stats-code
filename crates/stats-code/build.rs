//! Build script for `stats-code`.
//!
//! Three independent responsibilities live here, kept in named functions so
//! sibling tasks can extend this file additively without disturbing the
//! existing checks:
//!
//! 1. `check_sidecar_templates()` — Feature: parity-and-multilang-sidecar,
//!    Requirement 2.2 (task 2.1). Parses `src/coverage_matrix/matrix.toml`
//!    and asserts that every (algorithm × software) cell whose `coverage`
//!    value ∈ {`live`, `recorded`, `sidecar_only`} has a matching template
//!    placeholder at `src/sidecar/templates/<software_lower>/<id>.tmpl.txt`,
//!    AND that every `coverage = "none"` cell has *no* such file (per
//!    Requirement 2.4 the uncovered sentinel is structural, not a template).
//!    Runs unconditionally (template presence is a source-code invariant,
//!    independent of the dev-vite vs prod web pipeline).
//!
//! 2. `inject_release_version_and_mirror()` — Feature:
//!    parity-and-multilang-sidecar, Requirements 6.1, 6.2, 7.4 (task 1.3).
//!    Reads the on-disk skeleton `src/coverage_matrix/matrix.toml`, replaces
//!    its `release_version = "0.0.0-build-injected"` placeholder with the
//!    live `CARGO_PKG_VERSION`, writes the injected variant to
//!    `OUT_DIR/matrix.toml` (consumed by `coverage_matrix/mod.rs` via
//!    `include_str!(concat!(env!("OUT_DIR"), "/matrix.toml"))`), and mirrors
//!    the same bytes to `validation/coverage_matrix.toml` so the pytest
//!    parity suite never drifts from the Rust binary.
//!
//! 3. `emit_runtime_deps()` — Feature: parity-and-multilang-sidecar,
//!    Requirement 7.4 (task 1.3). Walks the workspace `Cargo.lock` snapshot,
//!    projects `stats-code`'s direct dependencies (name + version), and
//!    writes a deterministic JSON dump to `OUT_DIR/runtime_deps.json` for
//!    the snapshot exporter (task 6.3) to embed under
//!    `versions.json::runtime_dependencies`.
//!
//! 4. `emit_release_version()` — Feature: parity-and-multilang-sidecar,
//!    Requirements 1.7, 7.3, 7.4 (task 15.3). Writes the live
//!    `CARGO_PKG_VERSION` to `OUT_DIR/release_version.txt` as a raw,
//!    newline-free UTF-8 string so the runtime can `include_str!` it as a
//!    `&'static str` constant. The file is written without a trailing
//!    newline so the embedded constant is byte-equal to the version string
//!    itself (no `.trim_end()` needed in a `const` context).
//!
//! 5. `check_web_dist()` — Feature: single-command-launcher, Requirement 6.4.
//!    In prod (`dev-vite` feature off) the build fails if `web/dist/` or its
//!    `index.html` is missing.

use std::path::{Path, PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_DEV_VITE");
    println!("cargo:rerun-if-env-changed=CARGO_PKG_VERSION");

    let manifest_dir = PathBuf::from(
        std::env::var_os("CARGO_MANIFEST_DIR")
            .expect("CARGO_MANIFEST_DIR must be set by Cargo"),
    );

    let out_dir = PathBuf::from(
        std::env::var_os("OUT_DIR").expect("OUT_DIR must be set by Cargo for build scripts"),
    );

    // Run the sidecar template presence guard on every build, regardless of
    // dev-vite vs prod, so that the (matrix × templates) invariant cannot
    // silently drift. This must come *before* the dev-vite early return below
    // so dev builds also catch missing / stray templates.
    check_sidecar_templates(&manifest_dir);

    // Build-time injection of `release_version` into the matrix consumed by
    // the runtime loader, plus mirror to `validation/` for pytest. Must run
    // in both prod and dev-vite so `coverage_matrix/mod.rs::MATRIX_TOML`
    // (which `include_str!`'s `OUT_DIR/matrix.toml`) always finds the file.
    let _release_version = inject_release_version_and_mirror(&manifest_dir, &out_dir);

    // Build-time snapshot of direct runtime dependencies for the Audit
    // Snapshot Exporter (Requirement 7.4). Same rationale: must run in both
    // prod and dev-vite so the snapshot module's `include_str!` always
    // resolves.
    emit_runtime_deps(&manifest_dir, &out_dir);

    // Build-time emit of `release_version.txt` for the runtime
    // `RELEASE_VERSION` constant (Requirements 1.7, 7.3, 7.4 / task 15.3).
    // Same rationale as the runtime_deps emit: runs in both prod and
    // dev-vite so the lib's `include_str!` always resolves. Independent of
    // the matrix injection above so a future change that drops the
    // matrix-mirror step still leaves the standalone version artifact in
    // place.
    emit_release_version(&out_dir);

    // dev-vite mode skips the web/dist check (Requirement 6.4 only applies to
    // prod; in dev mode the launcher spawns Vite as the web source).
    if std::env::var_os("CARGO_FEATURE_DEV_VITE").is_some() {
        return;
    }

    check_web_dist(&manifest_dir);
}

// ---------------------------------------------------------------------------
// Section 1: sidecar template presence guard (Requirement 2.2 / task 2.1)
// ---------------------------------------------------------------------------

/// The four Reference Software identifiers from the Algorithm Coverage Matrix
/// schema, paired with their on-disk lowercase template directory names.
/// Order is fixed for stable, deterministic build-time error messages.
const SOFTWARE_KEYS: &[(&str, &str)] = &[
    ("R", "r"),
    ("SAS", "sas"),
    ("Python", "python"),
    ("SPSS", "spss"),
];

fn check_sidecar_templates(manifest_dir: &Path) {
    let matrix_path = manifest_dir
        .join("src")
        .join("coverage_matrix")
        .join("matrix.toml");
    let templates_root = manifest_dir
        .join("src")
        .join("sidecar")
        .join("templates");

    // Re-run when either the matrix or any template file changes.
    println!("cargo:rerun-if-changed={}", matrix_path.display());
    println!("cargo:rerun-if-changed={}", templates_root.display());

    let matrix_text = std::fs::read_to_string(&matrix_path).unwrap_or_else(|e| {
        panic!(
            "stats-code build: failed to read coverage matrix at {}: {} \
             (Requirement 2.2 — sidecar template presence guard)",
            matrix_path.display(),
            e
        );
    });

    let matrix: toml::Value = toml::from_str(&matrix_text).unwrap_or_else(|e| {
        panic!(
            "stats-code build: failed to parse coverage matrix at {}: {} \
             (Requirement 2.2)",
            matrix_path.display(),
            e
        );
    });

    let algorithms = matrix
        .get("algorithm")
        .and_then(|v| v.as_array())
        .unwrap_or_else(|| {
            panic!(
                "stats-code build: coverage matrix at {} is missing the \
                 `[[algorithm]]` array (Requirement 2.2)",
                matrix_path.display()
            );
        });

    let mut errors: Vec<String> = Vec::new();

    for (idx, entry) in algorithms.iter().enumerate() {
        let id = entry
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or_else(|| {
                panic!(
                    "stats-code build: coverage matrix entry #{idx} is \
                     missing required `id` string field (Requirement 2.2)"
                );
            });

        let coverage = entry
            .get("coverage")
            .and_then(|v| v.as_table())
            .unwrap_or_else(|| {
                panic!(
                    "stats-code build: coverage matrix entry `{id}` is \
                     missing required `[algorithm.coverage]` table \
                     (Requirement 2.2)"
                );
            });

        for (sw_key, sw_dir) in SOFTWARE_KEYS {
            let cov_value = coverage
                .get(*sw_key)
                .and_then(|v| v.as_str())
                .unwrap_or_else(|| {
                    panic!(
                        "stats-code build: coverage matrix entry `{id}` is \
                         missing required `coverage.{sw_key}` string field \
                         (Requirement 2.2)"
                    );
                });

            let tmpl_path = templates_root
                .join(sw_dir)
                .join(format!("{id}.tmpl.txt"));

            match cov_value {
                "live" | "recorded" | "sidecar_only" => {
                    if !tmpl_path.is_file() {
                        errors.push(format!(
                            "missing template: cell ({id}, {sw_key}) has \
                             coverage = \"{cov_value}\" but no placeholder \
                             exists at {} (Requirement 2.2)",
                            tmpl_path.display()
                        ));
                    }
                }
                "none" => {
                    if tmpl_path.exists() {
                        errors.push(format!(
                            "stray template: cell ({id}, {sw_key}) has \
                             coverage = \"none\" but a template file exists \
                             at {} — `none` cells emit a structured \
                             Uncovered sentinel, not a snippet \
                             (Requirement 2.4). Delete the file.",
                            tmpl_path.display()
                        ));
                    }
                }
                other => {
                    errors.push(format!(
                        "invalid coverage value: cell ({id}, {sw_key}) has \
                         coverage = \"{other}\"; expected one of \
                         {{live, recorded, sidecar_only, none}} \
                         (Requirement 6.2)"
                    ));
                }
            }
        }
    }

    if !errors.is_empty() {
        for err in &errors {
            println!("cargo:warning={err}");
        }
        panic!(
            "stats-code build: sidecar template presence guard found \
             {} violation(s); see warnings above. \
             Matrix: {} | Templates root: {}",
            errors.len(),
            matrix_path.display(),
            templates_root.display()
        );
    }
}

// ---------------------------------------------------------------------------
// Section 2: matrix.toml release_version injection + validation/ mirror
// (Feature: parity-and-multilang-sidecar, Requirements 6.1, 6.2, 7.4 / task 1.3)
// ---------------------------------------------------------------------------

/// Exact placeholder literal expected to appear once in
/// `src/coverage_matrix/matrix.toml`. Spelled identically to the file so a
/// future commit that accidentally edits it without a matching update here
/// fails the build with a clear, actionable error.
const RELEASE_VERSION_PLACEHOLDER: &str = r#"release_version = "0.0.0-build-injected""#;

/// Inject the live `CARGO_PKG_VERSION` into the matrix and emit two copies:
///
/// - `$OUT_DIR/matrix.toml` — consumed at compile time by
///   `coverage_matrix/mod.rs` via
///   `include_str!(concat!(env!("OUT_DIR"), "/matrix.toml"))`.
/// - `<crate>/validation/coverage_matrix.toml` — read by the pytest parity
///   suite. Mirroring at build time guarantees Rust and Python share one
///   version-stamped artifact (Requirement 6.1 single source of truth).
///
/// Returns the live release version string for the caller's convenience.
///
/// All I/O is fail-loud: a missing placeholder, a missing source file, or a
/// failed write to either destination panics with a message naming the
/// exact path involved, so a CI run on a read-only checkout fails
/// immediately rather than producing a silently-stale binary.
///
/// Line-ending policy: the source file may be checked out as CRLF on
/// Windows, but every artifact this function writes is normalized to LF
/// (UTF-8, no BOM) so that Rust binaries built on Windows and on Linux
/// embed byte-identical text — a prerequisite for the byte-deterministic
/// snapshot artifacts of Requirement 2.1 and the report-rendering code that
/// will reuse this matrix.
fn inject_release_version_and_mirror(manifest_dir: &Path, out_dir: &Path) -> String {
    let matrix_src = manifest_dir
        .join("src")
        .join("coverage_matrix")
        .join("matrix.toml");
    let matrix_out = out_dir.join("matrix.toml");
    let validation_mirror = manifest_dir
        .join("validation")
        .join("coverage_matrix.toml");

    // Re-run when the source matrix or the workspace package version
    // metadata changes. `Cargo.toml` is the canonical source of
    // `CARGO_PKG_VERSION`; the workspace root `Cargo.toml` carries the
    // shared `[workspace.package].version` so include both.
    println!("cargo:rerun-if-changed={}", matrix_src.display());
    println!("cargo:rerun-if-changed={}", manifest_dir.join("Cargo.toml").display());
    println!(
        "cargo:rerun-if-changed={}",
        manifest_dir.join("..").join("..").join("Cargo.toml").display()
    );
    // Mirror is also tracked so a manual edit (which would be wrong) at
    // least triggers a rebuild that overwrites the drift.
    println!("cargo:rerun-if-changed={}", validation_mirror.display());

    let release_version = std::env::var("CARGO_PKG_VERSION")
        .expect("CARGO_PKG_VERSION must be set by Cargo during build script execution");

    let raw = std::fs::read(&matrix_src).unwrap_or_else(|e| {
        panic!(
            "stats-code build: failed to read coverage matrix skeleton at {}: {} \
             (Requirement 6.1, task 1.3)",
            matrix_src.display(),
            e
        );
    });

    // Normalize CRLF -> LF without touching anything else (no BOM stripping,
    // no trailing-whitespace edits) so the diff against the source is
    // exactly the placeholder substitution.
    let normalized = normalize_crlf_to_lf(&raw);

    // UTF-8 validation — the matrix file is hand-edited and any stray
    // encoding is a bug we want to surface here, not at runtime parse.
    let text = std::str::from_utf8(&normalized).unwrap_or_else(|e| {
        panic!(
            "stats-code build: coverage matrix at {} is not valid UTF-8: {} \
             (Requirement 6.1, task 1.3)",
            matrix_src.display(),
            e
        );
    });

    // Single, exact-string replacement. The source file is the single source
    // of truth (Requirement 6.1) so by construction the placeholder appears
    // exactly once. A miss means someone edited the placeholder without
    // updating this constant — fail loud with a self-documenting message.
    if !text.contains(RELEASE_VERSION_PLACEHOLDER) {
        panic!(
            "stats-code build: coverage matrix at {} no longer contains the \
             expected placeholder line `{}`. Either restore the placeholder \
             in the on-disk skeleton or update RELEASE_VERSION_PLACEHOLDER \
             in build.rs to match the new literal. (Requirements 6.1, 7.4)",
            matrix_src.display(),
            RELEASE_VERSION_PLACEHOLDER,
        );
    }

    let injected_line = format!(r#"release_version = "{release_version}""#);
    let injected = text.replacen(RELEASE_VERSION_PLACEHOLDER, &injected_line, 1);

    write_lf_utf8(&matrix_out, injected.as_bytes()).unwrap_or_else(|e| {
        panic!(
            "stats-code build: failed to write injected matrix to {}: {} \
             (Requirement 6.1, task 1.3)",
            matrix_out.display(),
            e
        );
    });

    write_lf_utf8(&validation_mirror, injected.as_bytes()).unwrap_or_else(|e| {
        panic!(
            "stats-code build: failed to mirror injected matrix to {}: {} \
             — this file is the pytest parity suite's view of the matrix \
             (Requirement 6.1). A read-only checkout will fail here; that is \
             intentional so CI catches the drift immediately.",
            validation_mirror.display(),
            e
        );
    });

    release_version
}

/// Convert `\r\n` to `\n` in-place (returning a new `Vec<u8>`). Stand-alone
/// `\r` bytes are preserved (none should occur in the matrix, but we don't
/// silently rewrite them).
fn normalize_crlf_to_lf(input: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(input.len());
    let mut i = 0;
    while i < input.len() {
        if input[i] == b'\r' && i + 1 < input.len() && input[i + 1] == b'\n' {
            out.push(b'\n');
            i += 2;
        } else {
            out.push(input[i]);
            i += 1;
        }
    }
    out
}

/// Write `bytes` to `path`, creating parent directories as needed. Bytes
/// are written verbatim — callers that want LF-only output must normalize
/// before calling.
fn write_lf_utf8(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, bytes)
}

// ---------------------------------------------------------------------------
// Section 3: runtime_deps.json snapshot from Cargo.lock
// (Feature: parity-and-multilang-sidecar, Requirement 7.4 / task 1.3)
// ---------------------------------------------------------------------------

/// Walk the workspace `Cargo.lock` and emit a deterministic JSON dump of
/// `stats-code`'s direct runtime dependencies (name → version) to
/// `$OUT_DIR/runtime_deps.json`.
///
/// Consumed at compile time by the snapshot exporter (task 6.3) under
/// `versions.json::runtime_dependencies` so a snapshot can name the exact
/// libraries that produced its numeric outputs (Requirement 7.4).
///
/// Implementation choice: we hand-roll the JSON writer rather than pull in
/// `serde_json` as a build-dep, because the output is a flat string-string
/// map of well-known package names and semver versions — no escaping
/// hazards beyond the standard JSON string rules, and the saved build-time
/// dependency keeps the lockfile lean.
///
/// Determinism: keys are emitted in `BTreeMap` (lexicographic) order with
/// two-space indentation, LF newlines, and a trailing newline. Two
/// invocations on different hosts with the same `Cargo.lock` produce
/// byte-identical output.
fn emit_runtime_deps(manifest_dir: &Path, out_dir: &Path) {
    let lockfile = manifest_dir
        .join("..")
        .join("..")
        .join("Cargo.lock");
    let runtime_deps_out = out_dir.join("runtime_deps.json");

    println!("cargo:rerun-if-changed={}", lockfile.display());

    let lock_text = std::fs::read_to_string(&lockfile).unwrap_or_else(|e| {
        panic!(
            "stats-code build: failed to read workspace Cargo.lock at {}: {} \
             (Requirement 7.4 — runtime_deps.json snapshot). The build script \
             expects to run from a workspace member with `Cargo.lock` two \
             directories up.",
            lockfile.display(),
            e
        );
    });

    let lock: toml::Value = toml::from_str(&lock_text).unwrap_or_else(|e| {
        panic!(
            "stats-code build: failed to parse workspace Cargo.lock at {}: {} \
             (Requirement 7.4)",
            lockfile.display(),
            e
        );
    });

    let packages = lock
        .get("package")
        .and_then(|v| v.as_array())
        .unwrap_or_else(|| {
            panic!(
                "stats-code build: workspace Cargo.lock at {} is missing the \
                 `[[package]]` array (Requirement 7.4)",
                lockfile.display()
            );
        });

    // Build a lookup from package name → list of (version, _) pairs so we
    // can resolve dep entries that omit a version qualifier (the common
    // case when a name is unique in the workspace's resolved graph).
    let mut name_to_versions: std::collections::BTreeMap<&str, Vec<&str>> =
        std::collections::BTreeMap::new();
    for pkg in packages {
        let name = pkg
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or_else(|| {
                panic!(
                    "stats-code build: Cargo.lock package entry missing `name` \
                     field (Requirement 7.4)"
                )
            });
        let version = pkg
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or_else(|| {
                panic!(
                    "stats-code build: Cargo.lock package `{name}` is missing \
                     `version` field (Requirement 7.4)"
                )
            });
        name_to_versions.entry(name).or_default().push(version);
    }

    // Find the stats-code package's direct deps. Match on the
    // `CARGO_PKG_NAME` env var so this script remains correct if the crate
    // is renamed.
    let pkg_name = std::env::var("CARGO_PKG_NAME")
        .expect("CARGO_PKG_NAME must be set by Cargo during build script execution");
    let pkg_version = std::env::var("CARGO_PKG_VERSION")
        .expect("CARGO_PKG_VERSION must be set by Cargo during build script execution");

    let stats_code = packages
        .iter()
        .find(|p| {
            p.get("name").and_then(|v| v.as_str()) == Some(pkg_name.as_str())
                && p.get("version").and_then(|v| v.as_str()) == Some(pkg_version.as_str())
        })
        .unwrap_or_else(|| {
            panic!(
                "stats-code build: workspace Cargo.lock at {} does not contain \
                 a `[[package]]` entry for {pkg_name} {pkg_version} \
                 (Requirement 7.4)",
                lockfile.display(),
            )
        });

    let deps = stats_code
        .get("dependencies")
        .and_then(|v| v.as_array())
        .map(|a| a.as_slice())
        .unwrap_or(&[]);

    let mut runtime_deps: std::collections::BTreeMap<String, String> =
        std::collections::BTreeMap::new();

    // Always include the crate itself so the snapshot has at least one
    // entry even if `Cargo.lock` somehow lists no deps.
    runtime_deps.insert(pkg_name.clone(), pkg_version.clone());

    for dep in deps {
        let raw = dep.as_str().unwrap_or_else(|| {
            panic!(
                "stats-code build: Cargo.lock dependency entry for {pkg_name} \
                 is not a string (Requirement 7.4)"
            )
        });

        // Cargo.lock dependency strings are one of:
        //   "name"
        //   "name version"
        //   "name version (source-spec)"
        // The version qualifier appears when more than one version of the
        // same crate is in the resolved graph (e.g. `windows-sys 0.59.0`).
        let mut parts = raw.splitn(3, ' ');
        let name = parts
            .next()
            .expect("splitn always yields at least one element");
        let version_token = parts.next();

        let version = if let Some(v) = version_token {
            // Strip any registry-source suffix in case it ever shows up in
            // the second token. The standard Cargo.lock format keeps source
            // in a third token, but we tolerate both shapes.
            v.split_whitespace().next().unwrap_or(v).to_string()
        } else {
            // Resolve via the unique-name lookup. If multiple versions
            // exist with no qualifier, that is a malformed Cargo.lock for
            // our purposes — fail loud rather than silently pick one.
            let versions = name_to_versions.get(name).unwrap_or_else(|| {
                panic!(
                    "stats-code build: dependency `{name}` of {pkg_name} not \
                     found in Cargo.lock packages (Requirement 7.4)"
                )
            });
            if versions.len() != 1 {
                panic!(
                    "stats-code build: dependency `{name}` of {pkg_name} is \
                     ambiguous in Cargo.lock (versions: {versions:?}); \
                     Cargo should have written a version qualifier on the \
                     dependency line (Requirement 7.4)"
                );
            }
            versions[0].to_string()
        };

        runtime_deps.insert(name.to_string(), version);
    }

    let json = serialize_string_map_pretty(&runtime_deps);
    write_lf_utf8(&runtime_deps_out, json.as_bytes()).unwrap_or_else(|e| {
        panic!(
            "stats-code build: failed to write {}: {} (Requirement 7.4)",
            runtime_deps_out.display(),
            e
        );
    });
}

/// Hand-rolled deterministic JSON pretty-printer for a `BTreeMap<String,
/// String>`. Output shape:
///
/// ```text
/// {
///   "name1": "version1",
///   "name2": "version2"
/// }
/// ```
///
/// LF newlines, two-space indent, trailing newline, sorted keys (the
/// `BTreeMap` already enforces sort order). Strings are escaped per the
/// JSON spec; package names and semver strings used in `Cargo.lock` only
/// need `\\` and `\"` handling in practice, but the escaper covers
/// control characters too for safety against future inputs.
fn serialize_string_map_pretty(map: &std::collections::BTreeMap<String, String>) -> String {
    if map.is_empty() {
        return "{}\n".to_string();
    }
    let mut out = String::new();
    out.push_str("{\n");
    let last_idx = map.len() - 1;
    for (i, (k, v)) in map.iter().enumerate() {
        out.push_str("  ");
        out.push_str(&json_escape_string(k));
        out.push_str(": ");
        out.push_str(&json_escape_string(v));
        if i != last_idx {
            out.push(',');
        }
        out.push('\n');
    }
    out.push_str("}\n");
    out
}

/// Escape `s` per RFC 8259 string rules and wrap in double quotes.
fn json_escape_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

// ---------------------------------------------------------------------------
// Section 4: release_version.txt emit for runtime `RELEASE_VERSION` constant
// (Feature: parity-and-multilang-sidecar, Requirements 1.7, 7.3, 7.4 / task 15.3)
// ---------------------------------------------------------------------------

/// Emit `$OUT_DIR/release_version.txt` containing the live
/// `CARGO_PKG_VERSION` as raw UTF-8 with no trailing newline and no BOM.
///
/// The runtime exposes the file via
/// `pub const RELEASE_VERSION: &str = include_str!(concat!(env!("OUT_DIR"),
/// "/release_version.txt"));` (see `lib.rs`). Because `include_str!`
/// returns the file's bytes verbatim and there is no `.trim_end()` in a
/// `const` context, we deliberately omit the trailing newline so the
/// embedded constant equals the version string byte-for-byte. Two builds
/// of the same source tree on different hosts produce a byte-identical
/// artifact (no clock, no host state — the only input is
/// `CARGO_PKG_VERSION`, which Cargo derives from `Cargo.toml`).
///
/// Consumers:
///
/// - `lib.rs::RELEASE_VERSION` — surfaced for the SPA's
///   `<SidecarFooter>` (Requirement 1.7) and the Audit Snapshot's
///   `manifest.json::stats_code_release_version` (Requirement 7.3).
/// - `snapshot::manifest::build_manifest` — receives this value via its
///   `release_version` parameter (Requirement 7.3).
fn emit_release_version(out_dir: &Path) {
    let release_version = std::env::var("CARGO_PKG_VERSION")
        .expect("CARGO_PKG_VERSION must be set by Cargo during build script execution");

    let dest = out_dir.join("release_version.txt");
    write_lf_utf8(&dest, release_version.as_bytes()).unwrap_or_else(|e| {
        panic!(
            "stats-code build: failed to write {}: {} \
             (Requirements 1.7, 7.3, 7.4 — task 15.3)",
            dest.display(),
            e
        );
    });
}

// ---------------------------------------------------------------------------
// Section 5: web/dist prod check (single-command-launcher, Requirement 6.4)
// ---------------------------------------------------------------------------

fn check_web_dist(manifest_dir: &Path) {
    let web_dist = manifest_dir
        .join("..")
        .join("..")
        .join("web")
        .join("dist");
    let index_html = web_dist.join("index.html");

    println!("cargo:rerun-if-changed={}", web_dist.display());
    println!("cargo:rerun-if-changed={}", index_html.display());

    if !web_dist.is_dir() {
        println!(
            "cargo:warning=web/dist directory not found at {}",
            web_dist.display()
        );
        panic!(
            "stats-code prod build requires `web/dist/` to exist (Requirement 6.4). \
             Run `npm run build` in `web/` first, or build with `--features dev-vite` \
             for the dev workflow. Expected path: {}",
            web_dist.display()
        );
    }

    if !index_html.is_file() {
        println!(
            "cargo:warning=web/dist/index.html missing at {}",
            index_html.display()
        );
        panic!(
            "stats-code prod build requires `web/dist/index.html` (Requirement 6.4). \
             The directory exists but the entry point is missing — re-run `npm run build` \
             in `web/`. Expected path: {}",
            index_html.display()
        );
    }
}
