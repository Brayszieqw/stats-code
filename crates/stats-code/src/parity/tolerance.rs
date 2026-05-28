//! `tolerance_config.yaml` loader for the parity validation pipeline.
//!
//! Wave-1 (task 9.3 of `parity-and-multilang-sidecar`). This module owns the
//! one and only reader for the per-algorithm Parity Threshold pairs that the
//! CI Parity Suite (`crates/stats-code/validation/run_validation.py`) and the
//! `parity` Internal Subcommand (task 9.1 / 9.2, follow-up wave) consume.
//!
//! ## Schema
//!
//! ```yaml
//! version: 1
//! defaults:
//!   non_iterative: { absolute: 1.0e-9, relative: 1.0e-6 }
//!   iterative:     { absolute: 1.0e-7, relative: 1.0e-4 }
//! algorithms:
//!   cox:      { absolute: 1.0e-7, relative: 1.0e-4 }
//!   logistic: { absolute: 1.0e-7, relative: 1.0e-4 }
//! ```
//!
//! - `version` MUST be `1` (anything else is rejected — schema evolution
//!   travels through an explicit version bump).
//! - `defaults.non_iterative` and `defaults.iterative` are both required.
//! - Every entry under `algorithms` is `{ absolute, relative }` with finite
//!   non-negative `f64` values. Integer literals are accepted and promoted
//!   to `f64`.
//!
//! ## Defaults (Requirement 12.2 / 12.3)
//!
//! When no config file has been loaded — i.e. the caller is using
//! [`ToleranceConfig::default()`] — [`ToleranceConfig::default_for`] returns
//! the spec-mandated constants:
//!
//! | classification        | absolute | relative |
//! |-----------------------|----------|----------|
//! | non-Iterative Algorithm | `1e-9`   | `1e-6`   |
//! | Iterative Algorithm     | `1e-7`   | `1e-4`   |
//!
//! When a file *has* been loaded, `default_for` consults the loaded
//! `defaults_iterative` / `defaults_non_iterative` first, so a future
//! tolerance-config edit can change the fallback without a code change.
//!
//! ## Strict mode (Requirement 12.6)
//!
//! [`ToleranceConfig::require_for_algorithm`] returns
//! [`ToleranceConfigError::MissingAlgorithm`] when the requested algorithm
//! id is absent from `algorithms`. The `parity` Internal Subcommand
//! dispatcher (task 9.2) maps that into exit code `4`, satisfying the
//! contract that "file exists but lacks an entry for an algorithm processed
//! by the suite ⇒ abort, no Parity Validation Report".
//!
//! _Requirements: 12.1, 12.2, 12.3, 12.6_

#![allow(dead_code)] // public surface stabilizes as parity::run_local lands.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use yaml_rust2::yaml::{Hash, Yaml};
use yaml_rust2::YamlLoader;

// ---------------------------------------------------------------------------
// Spec-mandated default constants (Requirement 12.2, 12.3)
// ---------------------------------------------------------------------------

/// Default Parity Threshold for non-Iterative Output-Level Algorithms
/// (Requirement 12.2): relative `1e-6`, absolute `1e-9`.
pub const DEFAULT_NON_ITERATIVE: Tolerance = Tolerance {
    absolute: 1e-9,
    relative: 1e-6,
};

/// Default Parity Threshold for Iterative Output-Level Algorithms
/// (Requirement 12.3): relative `1e-4`, absolute `1e-7`.
pub const DEFAULT_ITERATIVE: Tolerance = Tolerance {
    absolute: 1e-7,
    relative: 1e-4,
};

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// A single Parity Threshold pair `(absolute, relative)`.
///
/// Both fields are non-negative finite `f64`s; the loader rejects negatives
/// and non-finite values on parse, so any [`Tolerance`] handed back by the
/// public API of this module is well-formed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tolerance {
    /// Absolute tolerance: `|stats_engine - reference|` is allowed up to
    /// this magnitude before the absolute-difference check trips.
    pub absolute: f64,
    /// Relative tolerance: only meaningful when `|reference| > absolute`.
    pub relative: f64,
}

/// In-memory representation of `tolerance_config.yaml`.
///
/// Constructed either by [`load_from_path`] (validating loader) or by
/// [`ToleranceConfig::default`] (spec-default fallback used when the
/// caller has no file). Both paths produce the *same* shape, so the rest
/// of the parity pipeline is oblivious to whether the config was loaded
/// from disk or synthesized from defaults.
#[derive(Debug, Clone, PartialEq)]
pub struct ToleranceConfig {
    /// Default for non-Iterative algorithms (Requirement 12.2).
    pub defaults_non_iterative: Tolerance,
    /// Default for Iterative algorithms (Requirement 12.3).
    pub defaults_iterative: Tolerance,
    /// Per-algorithm overrides keyed by algorithm id (the same id used in
    /// the Algorithm Coverage Matrix). `BTreeMap` keeps the iteration
    /// order deterministic for downstream report generation.
    pub per_algorithm: BTreeMap<String, Tolerance>,
}

impl Default for ToleranceConfig {
    /// Spec-mandated default config used when no `tolerance_config.yaml`
    /// has been loaded (Requirement 12.2, 12.3).
    fn default() -> Self {
        Self {
            defaults_non_iterative: DEFAULT_NON_ITERATIVE,
            defaults_iterative: DEFAULT_ITERATIVE,
            per_algorithm: BTreeMap::new(),
        }
    }
}

impl ToleranceConfig {
    /// Resolve the active Parity Threshold for a given algorithm id.
    ///
    /// Lookup order:
    /// 1. Explicit `algorithms.<id>` entry from the loaded config.
    /// 2. `defaults_iterative` if `iterative == true`, otherwise
    ///    `defaults_non_iterative`.
    ///
    /// This is the soft-mode reader used by report rendering and by the
    /// generic threshold predicate. The strict-mode reader is
    /// [`Self::require_for_algorithm`].
    #[must_use] 
    pub fn default_for(&self, algorithm_id: &str, iterative: bool) -> Tolerance {
        if let Some(t) = self.per_algorithm.get(algorithm_id) {
            return *t;
        }
        if iterative {
            self.defaults_iterative
        } else {
            self.defaults_non_iterative
        }
    }

    /// Strict reader: return [`ToleranceConfigError::MissingAlgorithm`]
    /// when the algorithm id is absent from `algorithms`.
    ///
    /// The `parity` Internal Subcommand maps this error into exit code
    /// `4` (Requirement 12.6: "file exists but lacks an entry for an
    /// algorithm processed by the suite ⇒ abort with the missing
    /// algorithm in the error message, no Parity Validation Report").
    pub fn require_for_algorithm(
        &self,
        algorithm_id: &str,
    ) -> Result<Tolerance, ToleranceConfigError> {
        self.per_algorithm.get(algorithm_id).copied().ok_or_else(|| {
            ToleranceConfigError::MissingAlgorithm {
                algorithm: algorithm_id.to_owned(),
            }
        })
    }
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Closed set of failure modes for [`load_from_path`] and the strict-mode
/// reader [`ToleranceConfig::require_for_algorithm`].
///
/// Every variant is structured (no formatted-string-only payloads) so the
/// `parity` Internal Subcommand dispatcher (task 9.2) can map them onto
/// the spec-defined exit codes (`4` for missing/invalid config,
/// per design.md exit-code table) without parsing message text.
#[derive(Debug, thiserror::Error)]
pub enum ToleranceConfigError {
    /// IO failure reading the file (missing, permission denied, etc.).
    #[error("io error reading {path}: {source}")]
    Io {
        /// Path the loader attempted to read.
        path: String,
        /// Underlying [`std::io::Error`].
        #[source]
        source: std::io::Error,
    },

    /// Input bytes are not valid UTF-8.
    #[error("tolerance_config.yaml: not valid UTF-8")]
    NotUtf8,

    /// Input is UTF-8 but not well-formed YAML, or has zero / multiple
    /// documents. The string carries the underlying scanner / structural
    /// description for log readability.
    #[error("tolerance_config.yaml yaml parse error: {0}")]
    Yaml(String),

    /// `version` field is missing, the wrong type, or not equal to `1`.
    #[error("tolerance_config.yaml: unsupported version (must be 1)")]
    UnsupportedVersion,

    /// A required top-level key is absent (`version`, `defaults`,
    /// `defaults.non_iterative`, `defaults.iterative`).
    #[error("tolerance_config.yaml: missing required field {field:?}")]
    MissingField {
        /// Dot-separated field path, drawn from a closed set:
        /// `"version"`, `"defaults"`, `"defaults.non_iterative"`,
        /// `"defaults.iterative"`.
        field: &'static str,
    },

    /// An entry under `defaults.*` or `algorithms.*` is malformed (wrong
    /// type, missing `absolute` / `relative`, negative, or non-finite).
    #[error("tolerance_config.yaml: invalid tolerance entry for {algorithm:?}: {reason}")]
    InvalidEntry {
        /// Algorithm id, or `"defaults.non_iterative"` /
        /// `"defaults.iterative"` for default rows.
        algorithm: String,
        /// Closed-set reason string: see [`REASON_*`](self) below.
        reason: &'static str,
    },

    /// `algorithms.<id>` is absent and the caller used the strict reader
    /// [`ToleranceConfig::require_for_algorithm`] (Requirement 12.6).
    #[error("tolerance_config.yaml: missing entry for algorithm {algorithm:?}")]
    MissingAlgorithm {
        /// Algorithm id that the strict reader could not resolve.
        algorithm: String,
    },
}

/// Closed-set reason strings for [`ToleranceConfigError::InvalidEntry`].
/// They are pinned to `&'static str` so callers can match on identity.
pub const REASON_NOT_A_MAPPING: &str = "entry is not a mapping";
pub const REASON_MISSING_ABSOLUTE: &str = "missing field `absolute`";
pub const REASON_MISSING_RELATIVE: &str = "missing field `relative`";
pub const REASON_ABSOLUTE_NOT_NUMERIC: &str = "field `absolute` is not numeric";
pub const REASON_RELATIVE_NOT_NUMERIC: &str = "field `relative` is not numeric";
pub const REASON_ABSOLUTE_NEGATIVE: &str = "field `absolute` is negative";
pub const REASON_RELATIVE_NEGATIVE: &str = "field `relative` is negative";
pub const REASON_ABSOLUTE_NON_FINITE: &str = "field `absolute` is not finite";
pub const REASON_RELATIVE_NON_FINITE: &str = "field `relative` is not finite";

// ---------------------------------------------------------------------------
// Public loader
// ---------------------------------------------------------------------------

/// Load a [`ToleranceConfig`] from a YAML file on disk.
///
/// Returns:
///
/// - `Ok(config)` when every gate below passes and the config is fully
///   validated.
/// - `Err(ToleranceConfigError::*)` otherwise. The error variants are a
///   closed set, no partial config is ever returned, and the caller can
///   map the variant onto exit code `4` (Requirement 12.6) without
///   parsing message text.
///
/// Validation gates, applied in order:
///
/// 1. File read: missing / permission errors surface as
///    [`ToleranceConfigError::Io`].
/// 2. UTF-8 validity: invalid byte sequences ⇒
///    [`ToleranceConfigError::NotUtf8`].
/// 3. YAML well-formedness: any [`yaml_rust2::ScanError`], or a doc count
///    other than one, surfaces as [`ToleranceConfigError::Yaml`].
/// 4. Schema:
///    - `version` is integer-typed and equal to `1`.
///    - `defaults.non_iterative` and `defaults.iterative` are both present
///      and well-formed.
///    - Each `algorithms.<id>` entry, if any, is a mapping with finite
///      non-negative `absolute` and `relative` values.
///
/// _Requirements: 12.1, 12.2, 12.3, 12.6_
pub fn load_from_path(path: &Path) -> Result<ToleranceConfig, ToleranceConfigError> {
    // Gate 1 — read the file. Wrap the IO error with the path so the
    // caller can identify which config was attempted.
    let bytes = fs::read(path).map_err(|source| ToleranceConfigError::Io {
        path: path_to_display(path),
        source,
    })?;

    // Gate 2 — UTF-8 validity. The scanner accepts &str, so we have to
    // promote here anyway.
    let text = std::str::from_utf8(&bytes).map_err(|_| ToleranceConfigError::NotUtf8)?;

    // Gate 3 — YAML well-formedness + exactly one document.
    let docs = YamlLoader::load_from_str(text)
        .map_err(|e| ToleranceConfigError::Yaml(format!("{e}")))?;
    if docs.is_empty() {
        return Err(ToleranceConfigError::Yaml(
            "document is empty (expected one mapping)".to_owned(),
        ));
    }
    if docs.len() > 1 {
        return Err(ToleranceConfigError::Yaml(
            "expected a single YAML document".to_owned(),
        ));
    }

    // Gate 4 — schema validation.
    let root = match &docs[0] {
        Yaml::Hash(h) => h,
        _ => {
            return Err(ToleranceConfigError::Yaml(
                "top-level document is not a mapping".to_owned(),
            ));
        }
    };

    // version must be present, integer-typed, and equal to 1.
    let version_node = lookup(root, "version").ok_or(ToleranceConfigError::MissingField {
        field: "version",
    })?;
    match version_node {
        Yaml::Integer(1) => {}
        Yaml::Integer(_) | Yaml::Real(_) | Yaml::String(_) => {
            return Err(ToleranceConfigError::UnsupportedVersion);
        }
        _ => return Err(ToleranceConfigError::UnsupportedVersion),
    }

    // defaults.non_iterative / defaults.iterative — both required.
    let defaults_node = lookup(root, "defaults").ok_or(ToleranceConfigError::MissingField {
        field: "defaults",
    })?;
    let defaults_hash = match defaults_node {
        Yaml::Hash(h) => h,
        _ => {
            return Err(ToleranceConfigError::Yaml(
                "`defaults` is not a mapping".to_owned(),
            ));
        }
    };

    let non_iterative_node = lookup(defaults_hash, "non_iterative").ok_or(
        ToleranceConfigError::MissingField {
            field: "defaults.non_iterative",
        },
    )?;
    let iterative_node = lookup(defaults_hash, "iterative").ok_or(
        ToleranceConfigError::MissingField {
            field: "defaults.iterative",
        },
    )?;

    let defaults_non_iterative =
        parse_tolerance_entry(non_iterative_node, "defaults.non_iterative")?;
    let defaults_iterative = parse_tolerance_entry(iterative_node, "defaults.iterative")?;

    // algorithms — optional. Empty / missing is valid (the defaults
    // alone are enough for a soft-mode lookup).
    let mut per_algorithm: BTreeMap<String, Tolerance> = BTreeMap::new();
    if let Some(algorithms_node) = lookup(root, "algorithms") {
        match algorithms_node {
            Yaml::Hash(h) => {
                for (k, v) in h {
                    let id = match k {
                        Yaml::String(s) => s.clone(),
                        _ => {
                            return Err(ToleranceConfigError::Yaml(
                                "`algorithms` keys must be strings".to_owned(),
                            ));
                        }
                    };
                    let entry = parse_tolerance_entry(v, &id)?;
                    per_algorithm.insert(id, entry);
                }
            }
            Yaml::Null => {
                // Explicit `algorithms: null` ⇒ treat as empty map.
            }
            _ => {
                return Err(ToleranceConfigError::Yaml(
                    "`algorithms` is not a mapping".to_owned(),
                ));
            }
        }
    }

    Ok(ToleranceConfig {
        defaults_non_iterative,
        defaults_iterative,
        per_algorithm,
    })
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn lookup<'a>(hash: &'a Hash, key: &str) -> Option<&'a Yaml> {
    hash.get(&Yaml::String(key.to_owned()))
}

/// Parse a single `{ absolute: f64, relative: f64 }` mapping, attributing
/// any failure to `algorithm` (an algorithm id, or the literal
/// `"defaults.non_iterative"` / `"defaults.iterative"` for default rows).
fn parse_tolerance_entry(
    node: &Yaml,
    algorithm: &str,
) -> Result<Tolerance, ToleranceConfigError> {
    let h = match node {
        Yaml::Hash(h) => h,
        _ => {
            return Err(ToleranceConfigError::InvalidEntry {
                algorithm: algorithm.to_owned(),
                reason: REASON_NOT_A_MAPPING,
            });
        }
    };

    let abs_node = lookup(h, "absolute").ok_or_else(|| ToleranceConfigError::InvalidEntry {
        algorithm: algorithm.to_owned(),
        reason: REASON_MISSING_ABSOLUTE,
    })?;
    let rel_node = lookup(h, "relative").ok_or_else(|| ToleranceConfigError::InvalidEntry {
        algorithm: algorithm.to_owned(),
        reason: REASON_MISSING_RELATIVE,
    })?;

    let absolute = yaml_to_f64(abs_node).ok_or_else(|| ToleranceConfigError::InvalidEntry {
        algorithm: algorithm.to_owned(),
        reason: REASON_ABSOLUTE_NOT_NUMERIC,
    })?;
    let relative = yaml_to_f64(rel_node).ok_or_else(|| ToleranceConfigError::InvalidEntry {
        algorithm: algorithm.to_owned(),
        reason: REASON_RELATIVE_NOT_NUMERIC,
    })?;

    if !absolute.is_finite() {
        return Err(ToleranceConfigError::InvalidEntry {
            algorithm: algorithm.to_owned(),
            reason: REASON_ABSOLUTE_NON_FINITE,
        });
    }
    if !relative.is_finite() {
        return Err(ToleranceConfigError::InvalidEntry {
            algorithm: algorithm.to_owned(),
            reason: REASON_RELATIVE_NON_FINITE,
        });
    }
    if absolute < 0.0 {
        return Err(ToleranceConfigError::InvalidEntry {
            algorithm: algorithm.to_owned(),
            reason: REASON_ABSOLUTE_NEGATIVE,
        });
    }
    if relative < 0.0 {
        return Err(ToleranceConfigError::InvalidEntry {
            algorithm: algorithm.to_owned(),
            reason: REASON_RELATIVE_NEGATIVE,
        });
    }

    Ok(Tolerance { absolute, relative })
}

/// Coerce a [`Yaml`] scalar to `f64`. Accepts integer literals (promoted
/// via `as f64`) and real literals (parsed from the canonical string
/// representation yaml-rust2 uses for floats).
fn yaml_to_f64(node: &Yaml) -> Option<f64> {
    match node {
        Yaml::Integer(n) => Some(*n as f64),
        Yaml::Real(s) => s.parse::<f64>().ok(),
        _ => None,
    }
}

/// Lossy `Path` rendering for error messages. Lossy is acceptable here:
/// the path appears in a human-readable error string only, never in any
/// snapshot or report artifact.
fn path_to_display(path: &Path) -> String {
    let pb: PathBuf = path.to_path_buf();
    pb.display().to_string()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    /// Helper: write `body` to a fresh temp file and return its path.
    fn write_temp_yaml(body: &str) -> NamedTempFile {
        let mut tmp = NamedTempFile::new().expect("create temp file");
        tmp.write_all(body.as_bytes()).expect("write yaml body");
        tmp.flush().expect("flush yaml body");
        tmp
    }

    const HAPPY_PATH_YAML: &str = "\
version: 1
defaults:
  non_iterative: { absolute: 1.0e-9, relative: 1.0e-6 }
  iterative:     { absolute: 1.0e-7, relative: 1.0e-4 }
algorithms:
  cox:      { absolute: 1.0e-7, relative: 1.0e-4 }
  logistic: { absolute: 1.0e-7, relative: 1.0e-4 }
";

    #[test]
    fn happy_path_parses_full_schema() {
        let tmp = write_temp_yaml(HAPPY_PATH_YAML);
        let cfg = load_from_path(tmp.path()).expect("happy path should parse");

        assert_eq!(cfg.defaults_non_iterative.absolute, 1e-9);
        assert_eq!(cfg.defaults_non_iterative.relative, 1e-6);
        assert_eq!(cfg.defaults_iterative.absolute, 1e-7);
        assert_eq!(cfg.defaults_iterative.relative, 1e-4);

        let cox = cfg.per_algorithm.get("cox").copied().expect("cox entry");
        assert_eq!(cox.absolute, 1e-7);
        assert_eq!(cox.relative, 1e-4);

        let logistic = cfg
            .per_algorithm
            .get("logistic")
            .copied()
            .expect("logistic entry");
        assert_eq!(logistic.absolute, 1e-7);
        assert_eq!(logistic.relative, 1e-4);
    }

    #[test]
    fn defaults_match_spec_constants() {
        let cfg = ToleranceConfig::default();
        assert_eq!(cfg.defaults_non_iterative, DEFAULT_NON_ITERATIVE);
        assert_eq!(cfg.defaults_iterative, DEFAULT_ITERATIVE);
        assert!(cfg.per_algorithm.is_empty());

        // Spec values, double-checked here so a future edit to the
        // constants does not silently change the defaults.
        assert_eq!(DEFAULT_NON_ITERATIVE.absolute, 1e-9);
        assert_eq!(DEFAULT_NON_ITERATIVE.relative, 1e-6);
        assert_eq!(DEFAULT_ITERATIVE.absolute, 1e-7);
        assert_eq!(DEFAULT_ITERATIVE.relative, 1e-4);
    }

    #[test]
    fn default_for_uses_loaded_defaults_for_missing_algorithm() {
        let tmp = write_temp_yaml(HAPPY_PATH_YAML);
        let cfg = load_from_path(tmp.path()).expect("happy path");

        // unknown algorithm, non-iterative ⇒ loaded non_iterative defaults
        let t = cfg.default_for("unknown_algo", false);
        assert_eq!(t, cfg.defaults_non_iterative);

        // unknown algorithm, iterative ⇒ loaded iterative defaults
        let t = cfg.default_for("unknown_algo", true);
        assert_eq!(t, cfg.defaults_iterative);
    }

    #[test]
    fn default_for_picks_explicit_entry_when_present() {
        let tmp = write_temp_yaml(HAPPY_PATH_YAML);
        let cfg = load_from_path(tmp.path()).expect("happy path");

        // `iterative` flag is irrelevant once an explicit entry exists —
        // explicit overrides win.
        let t1 = cfg.default_for("cox", true);
        let t2 = cfg.default_for("cox", false);
        assert_eq!(t1, t2);
        assert_eq!(t1.absolute, 1e-7);
        assert_eq!(t1.relative, 1e-4);
    }

    #[test]
    fn default_default_for_uses_spec_constants() {
        // Round-trip the documented "no config loaded" path: the
        // synthesized default must hand out spec constants.
        let cfg = ToleranceConfig::default();
        assert_eq!(cfg.default_for("anything", false), DEFAULT_NON_ITERATIVE);
        assert_eq!(cfg.default_for("anything", true), DEFAULT_ITERATIVE);
    }

    #[test]
    fn require_for_algorithm_returns_missing_when_absent() {
        let tmp = write_temp_yaml(HAPPY_PATH_YAML);
        let cfg = load_from_path(tmp.path()).expect("happy path");

        match cfg.require_for_algorithm("not_in_file") {
            Err(ToleranceConfigError::MissingAlgorithm { algorithm }) => {
                assert_eq!(algorithm, "not_in_file");
            }
            other => panic!(
                "expected MissingAlgorithm, got {:?}",
                other.map(|_| "Ok(_)")
            ),
        }
    }

    #[test]
    fn require_for_algorithm_returns_entry_when_present() {
        let tmp = write_temp_yaml(HAPPY_PATH_YAML);
        let cfg = load_from_path(tmp.path()).expect("happy path");

        let cox = cfg.require_for_algorithm("cox").expect("cox should exist");
        assert_eq!(cox.absolute, 1e-7);
        assert_eq!(cox.relative, 1e-4);
    }

    #[test]
    fn missing_version_field_is_rejected() {
        let yaml = "\
defaults:
  non_iterative: { absolute: 1.0e-9, relative: 1.0e-6 }
  iterative:     { absolute: 1.0e-7, relative: 1.0e-4 }
";
        let tmp = write_temp_yaml(yaml);
        match load_from_path(tmp.path()) {
            Err(ToleranceConfigError::MissingField { field }) => {
                assert_eq!(field, "version");
            }
            other => panic!("expected MissingField version, got {:?}", other.is_ok()),
        }
    }

    #[test]
    fn wrong_version_value_is_rejected() {
        let yaml = "\
version: 2
defaults:
  non_iterative: { absolute: 1.0e-9, relative: 1.0e-6 }
  iterative:     { absolute: 1.0e-7, relative: 1.0e-4 }
";
        let tmp = write_temp_yaml(yaml);
        assert!(matches!(
            load_from_path(tmp.path()),
            Err(ToleranceConfigError::UnsupportedVersion)
        ));
    }

    #[test]
    fn missing_defaults_non_iterative_is_rejected() {
        let yaml = "\
version: 1
defaults:
  iterative: { absolute: 1.0e-7, relative: 1.0e-4 }
";
        let tmp = write_temp_yaml(yaml);
        match load_from_path(tmp.path()) {
            Err(ToleranceConfigError::MissingField { field }) => {
                assert_eq!(field, "defaults.non_iterative");
            }
            other => panic!(
                "expected MissingField defaults.non_iterative, got {:?}",
                other.is_ok()
            ),
        }
    }

    #[test]
    fn missing_defaults_iterative_is_rejected() {
        let yaml = "\
version: 1
defaults:
  non_iterative: { absolute: 1.0e-9, relative: 1.0e-6 }
";
        let tmp = write_temp_yaml(yaml);
        match load_from_path(tmp.path()) {
            Err(ToleranceConfigError::MissingField { field }) => {
                assert_eq!(field, "defaults.iterative");
            }
            other => panic!(
                "expected MissingField defaults.iterative, got {:?}",
                other.is_ok()
            ),
        }
    }

    #[test]
    fn missing_defaults_root_is_rejected() {
        let yaml = "\
version: 1
algorithms:
  cox: { absolute: 1.0e-7, relative: 1.0e-4 }
";
        let tmp = write_temp_yaml(yaml);
        match load_from_path(tmp.path()) {
            Err(ToleranceConfigError::MissingField { field }) => {
                assert_eq!(field, "defaults");
            }
            other => panic!("expected MissingField defaults, got {:?}", other.is_ok()),
        }
    }

    #[test]
    fn string_instead_of_float_is_rejected() {
        let yaml = "\
version: 1
defaults:
  non_iterative: { absolute: \"oops\", relative: 1.0e-6 }
  iterative:     { absolute: 1.0e-7, relative: 1.0e-4 }
";
        let tmp = write_temp_yaml(yaml);
        match load_from_path(tmp.path()) {
            Err(ToleranceConfigError::InvalidEntry { algorithm, reason }) => {
                assert_eq!(algorithm, "defaults.non_iterative");
                assert_eq!(reason, REASON_ABSOLUTE_NOT_NUMERIC);
            }
            other => panic!("expected InvalidEntry, got {:?}", other.is_ok()),
        }
    }

    #[test]
    fn negative_absolute_is_rejected() {
        let yaml = "\
version: 1
defaults:
  non_iterative: { absolute: -1.0e-9, relative: 1.0e-6 }
  iterative:     { absolute: 1.0e-7, relative: 1.0e-4 }
";
        let tmp = write_temp_yaml(yaml);
        match load_from_path(tmp.path()) {
            Err(ToleranceConfigError::InvalidEntry { algorithm, reason }) => {
                assert_eq!(algorithm, "defaults.non_iterative");
                assert_eq!(reason, REASON_ABSOLUTE_NEGATIVE);
            }
            other => panic!("expected InvalidEntry, got {:?}", other.is_ok()),
        }
    }

    #[test]
    fn negative_relative_is_rejected_for_algorithm_entry() {
        let yaml = "\
version: 1
defaults:
  non_iterative: { absolute: 1.0e-9, relative: 1.0e-6 }
  iterative:     { absolute: 1.0e-7, relative: 1.0e-4 }
algorithms:
  cox: { absolute: 1.0e-7, relative: -1.0e-4 }
";
        let tmp = write_temp_yaml(yaml);
        match load_from_path(tmp.path()) {
            Err(ToleranceConfigError::InvalidEntry { algorithm, reason }) => {
                assert_eq!(algorithm, "cox");
                assert_eq!(reason, REASON_RELATIVE_NEGATIVE);
            }
            other => panic!("expected InvalidEntry, got {:?}", other.is_ok()),
        }
    }

    #[test]
    fn missing_relative_field_is_rejected() {
        let yaml = "\
version: 1
defaults:
  non_iterative: { absolute: 1.0e-9 }
  iterative:     { absolute: 1.0e-7, relative: 1.0e-4 }
";
        let tmp = write_temp_yaml(yaml);
        match load_from_path(tmp.path()) {
            Err(ToleranceConfigError::InvalidEntry { algorithm, reason }) => {
                assert_eq!(algorithm, "defaults.non_iterative");
                assert_eq!(reason, REASON_MISSING_RELATIVE);
            }
            other => panic!("expected InvalidEntry, got {:?}", other.is_ok()),
        }
    }

    #[test]
    fn integer_literals_are_accepted_and_promoted() {
        let yaml = "\
version: 1
defaults:
  non_iterative: { absolute: 0, relative: 0 }
  iterative:     { absolute: 0, relative: 0 }
algorithms:
  exact: { absolute: 0, relative: 0 }
";
        let tmp = write_temp_yaml(yaml);
        let cfg = load_from_path(tmp.path()).expect("integer literals should parse");
        assert_eq!(cfg.defaults_non_iterative.absolute, 0.0);
        assert_eq!(cfg.defaults_iterative.relative, 0.0);
        let exact = cfg.per_algorithm.get("exact").copied().expect("exact");
        assert_eq!(exact.absolute, 0.0);
        assert_eq!(exact.relative, 0.0);
    }

    #[test]
    fn empty_algorithms_section_is_accepted() {
        let yaml = "\
version: 1
defaults:
  non_iterative: { absolute: 1.0e-9, relative: 1.0e-6 }
  iterative:     { absolute: 1.0e-7, relative: 1.0e-4 }
algorithms: {}
";
        let tmp = write_temp_yaml(yaml);
        let cfg = load_from_path(tmp.path()).expect("empty algorithms should parse");
        assert!(cfg.per_algorithm.is_empty());
    }

    #[test]
    fn missing_algorithms_section_is_accepted() {
        let yaml = "\
version: 1
defaults:
  non_iterative: { absolute: 1.0e-9, relative: 1.0e-6 }
  iterative:     { absolute: 1.0e-7, relative: 1.0e-4 }
";
        let tmp = write_temp_yaml(yaml);
        let cfg = load_from_path(tmp.path()).expect("missing algorithms should parse");
        assert!(cfg.per_algorithm.is_empty());
    }

    #[test]
    fn io_error_when_file_does_not_exist() {
        let bogus = std::path::Path::new(
            "this-path-must-not-exist-tolerance-config-xyz-9f3.yaml",
        );
        match load_from_path(bogus) {
            Err(ToleranceConfigError::Io { path, source: _ }) => {
                assert!(path.contains("tolerance-config-xyz-9f3"));
            }
            other => panic!("expected Io error, got {:?}", other.is_ok()),
        }
    }

    #[test]
    fn malformed_yaml_is_rejected() {
        // Unbalanced bracket — yaml-rust2 surfaces this as a ScanError.
        let yaml = "version: 1\ndefaults: { non_iterative: { absolute: 1.0e-9, relative: 1.0e-6";
        let tmp = write_temp_yaml(yaml);
        assert!(matches!(
            load_from_path(tmp.path()),
            Err(ToleranceConfigError::Yaml(_))
        ));
    }

    #[test]
    fn non_utf8_bytes_are_rejected() {
        let mut tmp = NamedTempFile::new().expect("create temp file");
        // 0xFF is not a valid UTF-8 start byte.
        tmp.write_all(&[0xFF, 0xFE, 0xFD]).expect("write bytes");
        tmp.flush().expect("flush");
        assert!(matches!(
            load_from_path(tmp.path()),
            Err(ToleranceConfigError::NotUtf8)
        ));
    }
}
