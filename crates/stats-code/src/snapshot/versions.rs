//! `versions.json` builder for the Audit Snapshot.
//!
//! Implements task 6.3 of `parity-and-multilang-sidecar`. See design.md
//! ("Data Models — `versions.json`") and Requirements 7.4 / 9.2.
//!
//! `build_versions` is a **pure function**: it reads no clock, no host
//! environment, no random seeds. Every dynamic input flows in via arguments
//! so two calls with byte-identical inputs produce byte-identical output
//! (the byte-determinism contract carried into the snapshot exporter, task
//! 6.7).
//!
//! Privacy invariants enforced by Requirement 9.2:
//!
//! - `os_family` is documented as one of `"Windows" | "Linux" | "macOS"`.
//!   Validation lives in the caller (task 6.7); this module emits a
//!   `debug_assert!` so a wrong-family call fails loudly during development
//!   without panicking a release build over a string the caller already
//!   filtered.
//! - `os_version` is truncated to **at most 32 characters** on a UTF-8
//!   character boundary (so the field can never split a multi-byte glyph).
//!   When truncation occurs, `version_truncated` is set to `true` and the
//!   stored value is exactly the first 32 characters.
//! - `host_name`, `user_name`, and any user-profile-rooted absolute path
//!   are **never read** here — those values are simply not in the input
//!   set this function accepts.
//!
//! Determinism notes:
//!
//! - `reference_software` is stored verbatim in the order the caller passed
//!   it in. The caller (task 6.7) is expected to deduplicate and sort by
//!   `name` if it cares; this builder does not reorder so the snapshot can
//!   record "the order in which the run actually invoked them" if a future
//!   policy requires it. For the current schema we accept either order; the
//!   `runtime_dependencies` map is the byte-deterministic anchor.
//! - `runtime_dependencies` is stored as a `BTreeMap<String, String>` so
//!   serialization order is lexicographic by key — independent of the JSON
//!   parser's incidental ordering of the input string.
//!
//! _Requirements: 7.4, 9.2_

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Schema version of the `versions.json` payload. Bumped on any breaking
/// change to the field set; new readers must reject unknown values.
pub const SCHEMA_VERSION: u32 = 1;

/// Maximum length, in Unicode scalar values, of `os_version`. Values longer
/// than this are truncated on a UTF-8 character boundary and flagged via
/// `version_truncated = true` (Requirement 9.2: "OS version string of at
/// most 32 characters").
pub const OS_VERSION_MAX_CHARS: usize = 32;

/// One reference software entry inside `versions.json::reference_software`.
///
/// `name` is one of the four Reference Software identifiers from the
/// Algorithm Coverage Matrix (`"R" | "SAS" | "Python" | "SPSS"`). `version`
/// is the version string the caller observed when the run actually invoked
/// that software; the matrix's pinned-version metadata is *not* substituted
/// here because Requirement 7.4 specifies "the version of every Reference
/// Software whose snippet appears in the run's Equivalent Code Sidecars",
/// i.e. what was actually invoked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReferenceSoftwareVersion {
    pub name: String,
    pub version: String,
}

/// Top-level shape of `versions.json` inside an Audit Snapshot.
///
/// Field order matches `design.md` ("Data Models — `versions.json`") and
/// Requirement 7.4. Serialization preserves this order, giving byte-stable
/// JSON for byte-identical inputs (the determinism contract of
/// Requirement 7.1, threaded through this builder by task 6.7).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Versions {
    /// Always `1` for this revision. See [`SCHEMA_VERSION`].
    pub schema_version: u32,
    /// One of `"Windows" | "Linux" | "macOS"` (Requirement 9.2). Stored
    /// verbatim; the caller is responsible for ensuring the value is in
    /// the closed set, with `debug_assert!` providing a dev-time backstop.
    pub os_family: String,
    /// At most [`OS_VERSION_MAX_CHARS`] Unicode scalar values, truncated on
    /// a UTF-8 character boundary if the raw input was longer.
    pub os_version: String,
    /// `true` iff the raw OS version string was truncated to fit
    /// [`OS_VERSION_MAX_CHARS`].
    pub version_truncated: bool,
    /// Reference software actually invoked during the run. Empty when no
    /// Equivalent Code Sidecar was rendered or no reference snippet was
    /// executed (Requirement 7.4: "or an empty list when no Equivalent Code
    /// Sidecars were rendered in the run").
    pub reference_software: Vec<ReferenceSoftwareVersion>,
    /// Direct runtime dependency name → version. Stored as `BTreeMap` so
    /// JSON serialization is lexicographic by name, byte-stable across
    /// hosts (Requirement 7.4 "version of every direct runtime dependency
    /// loaded during the run's numeric computation steps").
    pub runtime_dependencies: BTreeMap<String, String>,
}

/// Build the `versions.json` payload for a snapshot.
///
/// Pure function: every dynamic input flows in via arguments.
///
/// # Inputs
///
/// - `os_family`: one of `"Windows" | "Linux" | "macOS"`. The caller
///   (task 6.7) is responsible for normalizing this; a wrong value triggers
///   a `debug_assert!` for early dev-time detection but is otherwise stored
///   verbatim so a release build does not panic over a host-detection edge
///   case the caller already screened.
/// - `raw_os_version`: the host OS version string. Truncated to at most
///   [`OS_VERSION_MAX_CHARS`] Unicode scalar values on a UTF-8 character
///   boundary; `version_truncated` is set to reflect whether truncation
///   actually happened.
/// - `reference_software`: every Reference Software actually invoked
///   during the run, in the order the caller chose. Empty slice yields an
///   empty `Vec` per Requirement 7.4.
/// - `runtime_deps_json`: the bytes of `$OUT_DIR/runtime_deps.json` as
///   produced by `build.rs::emit_runtime_deps`. The format is a flat JSON
///   object mapping package name to version string; parsing this back into
///   a `BTreeMap` round-trips through `serde_json` deterministically.
///
/// # Panics
///
/// Panics with a clear, actionable message if `runtime_deps_json` does not
/// parse as a JSON object of `String → String`. This is documented as a
/// build-time invariant: `build.rs` produces the file via a hand-rolled
/// deterministic JSON writer, so a parse failure here means either the
/// build script is broken or a caller passed unrelated bytes — both
/// programmer errors that should fail loudly.
///
/// _Requirements: 7.4, 9.2_
#[must_use]
pub fn build_versions(
    os_family: &str,
    raw_os_version: &str,
    reference_software: &[ReferenceSoftwareVersion],
    runtime_deps_json: &str,
) -> Versions {
    debug_assert!(
        matches!(os_family, "Windows" | "Linux" | "macOS"),
        "os_family must be one of \"Windows\", \"Linux\", \"macOS\"; got {os_family:?}. \
         Caller (task 6.7) is responsible for validation; this debug-only assert \
         catches misuse during development without panicking release builds."
    );

    let (os_version, version_truncated) = truncate_os_version(raw_os_version);

    let runtime_dependencies: BTreeMap<String, String> =
        serde_json::from_str(runtime_deps_json).unwrap_or_else(|e| {
            panic!(
                "snapshot::versions::build_versions: failed to parse \
                 runtime_deps_json as a JSON object of String→String: {e}. \
                 The build script (`build.rs::emit_runtime_deps`) is the \
                 single authoritative producer of this file and emits a \
                 deterministic flat map. A parse failure here means either \
                 the build script regressed or a caller passed unrelated \
                 bytes (Requirement 7.4)."
            )
        });

    Versions {
        schema_version: SCHEMA_VERSION,
        os_family: os_family.to_owned(),
        os_version,
        version_truncated,
        reference_software: reference_software.to_vec(),
        runtime_dependencies,
    }
}

/// Truncate `raw` to at most [`OS_VERSION_MAX_CHARS`] Unicode scalar values
/// on a UTF-8 character boundary, returning the truncated string and a
/// `truncated` flag.
///
/// Uses `char_indices().nth(OS_VERSION_MAX_CHARS)` so:
/// - if `raw` has ≤ 32 characters, the iterator yields `None` and we keep
///   the original string with `truncated = false`;
/// - if `raw` has ≥ 33 characters, the iterator yields the byte index
///   where the 33rd character starts, and we slice `raw` up to that index,
///   producing exactly 32 characters with `truncated = true`.
///
/// This guarantees the boundary slice is always valid UTF-8 — `char_indices`
/// only ever yields valid character start positions.
fn truncate_os_version(raw: &str) -> (String, bool) {
    match raw.char_indices().nth(OS_VERSION_MAX_CHARS) {
        Some((cutoff, _)) => (raw[..cutoff].to_owned(), true),
        None => (raw.to_owned(), false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal valid runtime_deps JSON for tests that don't exercise that
    /// surface specifically.
    const SAMPLE_DEPS_JSON: &str = r#"{"axum":"0.7.5","stats-code":"0.5.0"}"#;

    fn sample_refs() -> Vec<ReferenceSoftwareVersion> {
        vec![
            ReferenceSoftwareVersion {
                name: "R".to_owned(),
                version: "4.4.1".to_owned(),
            },
            ReferenceSoftwareVersion {
                name: "Python".to_owned(),
                version: "3.11.9".to_owned(),
            },
        ]
    }

    #[test]
    fn happy_path_windows() {
        let v = build_versions("Windows", "10.0.22631", &sample_refs(), SAMPLE_DEPS_JSON);
        assert_eq!(v.schema_version, 1);
        assert_eq!(v.os_family, "Windows");
        assert_eq!(v.os_version, "10.0.22631");
        assert!(!v.version_truncated);
        assert_eq!(v.reference_software, sample_refs());
        assert_eq!(v.runtime_dependencies.get("axum").map(String::as_str), Some("0.7.5"));
        assert_eq!(
            v.runtime_dependencies.get("stats-code").map(String::as_str),
            Some("0.5.0")
        );
    }

    #[test]
    fn happy_path_linux() {
        let v = build_versions("Linux", "6.6.0", &[], SAMPLE_DEPS_JSON);
        assert_eq!(v.os_family, "Linux");
        assert_eq!(v.os_version, "6.6.0");
        assert!(!v.version_truncated);
    }

    #[test]
    fn happy_path_macos() {
        let v = build_versions("macOS", "14.5", &[], SAMPLE_DEPS_JSON);
        assert_eq!(v.os_family, "macOS");
        assert_eq!(v.os_version, "14.5");
        assert!(!v.version_truncated);
    }

    #[test]
    fn os_version_exactly_32_chars_is_not_truncated() {
        // 32 ASCII characters, exactly at the limit.
        let raw = "a".repeat(OS_VERSION_MAX_CHARS);
        assert_eq!(raw.chars().count(), OS_VERSION_MAX_CHARS);

        let v = build_versions("Linux", &raw, &[], SAMPLE_DEPS_JSON);
        assert_eq!(v.os_version, raw);
        assert!(
            !v.version_truncated,
            "exactly {OS_VERSION_MAX_CHARS} chars must not trigger truncation"
        );
        assert_eq!(v.os_version.chars().count(), OS_VERSION_MAX_CHARS);
    }

    #[test]
    fn os_version_33_chars_is_truncated() {
        // 33 ASCII characters → must truncate to 32 with the flag set.
        let raw = "b".repeat(OS_VERSION_MAX_CHARS + 1);
        assert_eq!(raw.chars().count(), OS_VERSION_MAX_CHARS + 1);

        let v = build_versions("Linux", &raw, &[], SAMPLE_DEPS_JSON);
        assert_eq!(v.os_version.chars().count(), OS_VERSION_MAX_CHARS);
        assert_eq!(v.os_version, "b".repeat(OS_VERSION_MAX_CHARS));
        assert!(v.version_truncated);
    }

    #[test]
    fn os_version_long_input_truncated_to_32() {
        let raw = "Windows 10.0.22631.4317 Build 22631 with Some Additional Tag";
        assert!(raw.chars().count() > OS_VERSION_MAX_CHARS);

        let v = build_versions("Windows", raw, &[], SAMPLE_DEPS_JSON);
        assert_eq!(v.os_version.chars().count(), OS_VERSION_MAX_CHARS);
        assert!(v.version_truncated);
        // The first 32 ASCII characters should be retained byte-for-byte.
        assert_eq!(v.os_version, raw.chars().take(OS_VERSION_MAX_CHARS).collect::<String>());
    }

    #[test]
    fn os_version_truncation_is_utf8_boundary_safe() {
        // 30 ASCII chars + "中文" (3 bytes each in UTF-8). 32 characters
        // total, but if we had naively sliced at byte 32 we'd land mid-
        // glyph. Here we expect: at the 32-char limit, no truncation.
        let raw = format!("{}中文", "a".repeat(30));
        assert_eq!(raw.chars().count(), 32);
        let v = build_versions("Linux", &raw, &[], SAMPLE_DEPS_JSON);
        assert!(!v.version_truncated);
        assert_eq!(v.os_version, raw);

        // Now push to 33 chars by adding one more ASCII; truncation must
        // produce exactly 32 valid chars (no panic, no broken UTF-8).
        let raw2 = format!("{}中文a", "a".repeat(30));
        assert_eq!(raw2.chars().count(), 33);
        let v2 = build_versions("Linux", &raw2, &[], SAMPLE_DEPS_JSON);
        assert!(v2.version_truncated);
        assert_eq!(v2.os_version.chars().count(), 32);
        // The truncated string must itself be valid UTF-8 — implied by it
        // being a `String`, but we also check it ends with the second
        // multi-byte glyph intact.
        assert!(v2.os_version.ends_with("中文"));
    }

    #[test]
    fn empty_reference_software_yields_empty_vec() {
        let v = build_versions("Windows", "10.0.22631", &[], SAMPLE_DEPS_JSON);
        assert!(v.reference_software.is_empty());
    }

    #[test]
    fn reference_software_preserves_caller_order() {
        let refs = vec![
            ReferenceSoftwareVersion {
                name: "Python".to_owned(),
                version: "3.11.9".to_owned(),
            },
            ReferenceSoftwareVersion {
                name: "R".to_owned(),
                version: "4.4.1".to_owned(),
            },
        ];
        let v = build_versions("Linux", "6.6.0", &refs, SAMPLE_DEPS_JSON);
        assert_eq!(v.reference_software, refs);
        // Specifically verify Python comes first (we did NOT sort).
        assert_eq!(v.reference_software[0].name, "Python");
    }

    #[test]
    fn runtime_deps_keys_are_sorted_in_btreemap() {
        // Pass JSON in non-sorted order; the BTreeMap must end up sorted.
        let json = r#"{"zeta":"1.0.0","alpha":"0.1.0","mu":"2.0.0","beta":"0.2.0"}"#;
        let v = build_versions("Linux", "6.6.0", &[], json);

        let keys: Vec<&str> = v.runtime_dependencies.keys().map(String::as_str).collect();
        assert_eq!(keys, vec!["alpha", "beta", "mu", "zeta"]);
    }

    #[test]
    fn runtime_deps_round_trips_through_serde_json() {
        let v = build_versions(
            "Windows",
            "10.0.22631",
            &sample_refs(),
            r#"{"axum":"0.7.5","yaml-rust2":"0.11.0"}"#,
        );

        let json = serde_json::to_vec(&v).expect("Versions serializes");
        let parsed: Versions = serde_json::from_slice(&json).expect("Versions round-trips");
        assert_eq!(parsed, v);
    }

    #[test]
    fn json_field_order_matches_struct_declaration() {
        let v = build_versions("Windows", "10.0.22631", &sample_refs(), SAMPLE_DEPS_JSON);
        let json = serde_json::to_string(&v).unwrap();
        let pos = |needle: &str| {
            json.find(needle)
                .unwrap_or_else(|| panic!("missing field {needle} in {json}"))
        };
        let order = [
            pos("\"schema_version\""),
            pos("\"os_family\""),
            pos("\"os_version\""),
            pos("\"version_truncated\""),
            pos("\"reference_software\""),
            pos("\"runtime_dependencies\""),
        ];
        let mut sorted = order;
        sorted.sort_unstable();
        assert_eq!(
            order, sorted,
            "JSON fields must appear in struct declaration order; got {json}"
        );
    }

    #[test]
    fn determinism_byte_identical_json_for_same_inputs() {
        let refs = sample_refs();
        let json_in = r#"{"axum":"0.7.5","yaml-rust2":"0.11.0","serde":"1.0.0"}"#;

        let v1 = build_versions("Windows", "10.0.22631", &refs, json_in);
        let v2 = build_versions("Windows", "10.0.22631", &refs, json_in);
        assert_eq!(v1, v2);

        let j1 = serde_json::to_vec(&v1).unwrap();
        let j2 = serde_json::to_vec(&v2).unwrap();
        assert_eq!(j1, j2, "byte-identical inputs must produce byte-identical JSON");
    }

    #[test]
    fn determinism_runtime_deps_serialization_independent_of_input_order() {
        // Two different JSON spellings of the same logical map must produce
        // the same `Versions` value and the same serialized JSON.
        let a = r#"{"alpha":"1.0","beta":"2.0","gamma":"3.0"}"#;
        let b = r#"{"gamma":"3.0","alpha":"1.0","beta":"2.0"}"#;

        let va = build_versions("Linux", "6.6.0", &[], a);
        let vb = build_versions("Linux", "6.6.0", &[], b);
        assert_eq!(va, vb);

        let ja = serde_json::to_vec(&va).unwrap();
        let jb = serde_json::to_vec(&vb).unwrap();
        assert_eq!(
            ja, jb,
            "BTreeMap serialization must be order-independent on input"
        );
    }

    #[test]
    fn empty_runtime_deps_object_yields_empty_map() {
        let v = build_versions("Windows", "10.0.22631", &[], "{}");
        assert!(v.runtime_dependencies.is_empty());
    }

    #[test]
    #[should_panic(expected = "failed to parse runtime_deps_json")]
    fn malformed_runtime_deps_json_panics_with_clear_message() {
        let _ = build_versions("Windows", "10.0.22631", &[], "not json");
    }

    #[test]
    #[should_panic(expected = "failed to parse runtime_deps_json")]
    fn non_object_runtime_deps_panics() {
        // A JSON array is valid JSON but not the expected map shape.
        let _ = build_versions("Windows", "10.0.22631", &[], r#"["a","b"]"#);
    }

    #[test]
    #[should_panic(expected = "failed to parse runtime_deps_json")]
    fn nested_object_runtime_deps_panics() {
        // String→String map is the contract; a nested value is rejected.
        let _ = build_versions(
            "Windows",
            "10.0.22631",
            &[],
            r#"{"axum":{"version":"0.7.5"}}"#,
        );
    }
}
