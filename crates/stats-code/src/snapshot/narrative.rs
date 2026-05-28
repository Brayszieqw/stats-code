//! `narrative.md` (STROBE/CONSORT-style) builder for the Audit Snapshot.
//!
//! Implements task 6.5 of `parity-and-multilang-sidecar`. See design.md
//! "`narrative.md`" / Property 20 ("Narrative citations resolve") and
//! Requirement 8.5 (every cited artifact path must resolve to a file present
//! in the snapshot's file index).
//!
//! `build_narrative` is a **pure function**: no clock, no environment, no
//! I/O. The list of available artifact paths is supplied by the caller (the
//! snapshot exporter, task 6.7) as a `BTreeSet<String>`, which gives
//! deterministic ordering and O(log n) `contains` lookups.
//!
//! ## Output format
//!
//! Plain UTF-8 markdown. All line endings are `\n` (no `\r`). The output
//! ends with a single trailing `\n`.
//!
//! - Heading: `# Audit Snapshot Narrative\n`, followed by a blank line
//!   (`\n`) when at least one step is present.
//! - For each step (in the order supplied by the caller):
//!   - `## Step {id}: {display_name}\n\n`
//!   - ``Algorithm `{algorithm}` with parameters: {params_summary}.\n\n``
//!   - One bullet per `KeyMetric`:
//!     `- {label}: {value} [{artifact_path}#{json_pointer}]\n`
//! - Steps are separated by a single blank line. The last step is followed
//!   by the bullet's trailing `\n` only — no trailing blank line at EOF —
//!   which is what the task's "end with single trailing LF" means.
//! - When `steps` is empty the output is exactly the literal string
//!   `# Audit Snapshot Narrative\n` (single trailing LF, no blank line).
//!
//! _Requirements: 8.5_

use std::collections::BTreeSet;
use std::fmt::Write;

/// One step's contribution to the narrative.
///
/// `key_metrics` carries the numeric values the prose cites; every metric's
/// `artifact_path` must exist in the snapshot's file index that gets passed
/// to [`build_narrative`], otherwise the exporter aborts before any byte is
/// written (Requirement 8.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NarrativeStep {
    /// Step identifier, e.g. `"step-1"`. Reused as the `<step_id>` segment
    /// in the citation paths under `artifacts/<step_id>/`.
    pub id: String,
    /// Output-Level Algorithm identifier this step ran, e.g. `"tableone"`.
    pub algorithm: String,
    /// Human-readable display name shown in the `## Step` heading.
    pub display_name: String,
    /// Single-line summary of the parameter set, e.g. `"by=treatment"`.
    pub params_summary: String,
    /// Ordered key metrics; rendered as bullets in the order supplied.
    pub key_metrics: Vec<KeyMetric>,
}

/// One numeric value cited in the narrative.
///
/// Renders as `- {label}: {value} [{artifact_path}#{json_pointer}]\n`.
/// `artifact_path` must resolve in the snapshot's file index (Requirement
/// 8.5); `json_pointer` is the RFC 6901 fragment into that artifact, e.g.
/// `"estimate"` or `"ci/0"`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyMetric {
    /// Human-readable label, e.g. `"Effect estimate"`.
    pub label: String,
    /// Pre-rendered value (caller is responsible for numeric formatting).
    pub value: String,
    /// Path inside the snapshot, e.g. `"artifacts/step-1/result.json"`.
    pub artifact_path: String,
    /// JSON Pointer fragment, e.g. `"estimate"` or `"ci/0"`.
    pub json_pointer: String,
}

/// Errors returned by [`build_narrative`].
///
/// The narrative builder validates citations against the file index before
/// emitting any output; on mismatch it returns an error and produces no
/// partial string (Requirement 8.5).
#[derive(Debug, thiserror::Error)]
pub enum NarrativeError {
    /// A `KeyMetric` cited an `artifact_path` that is not present in the
    /// supplied `artifacts_index`.
    #[error("narrative cites unknown artifact path: {path}")]
    UnknownArtifact {
        /// The offending path, copied from the citing `KeyMetric`.
        path: String,
    },
}

/// Build the `narrative.md` UTF-8 markdown body for a snapshot.
///
/// Every numeric value the prose states is followed by an inline citation
/// `[<artifact_path>#<json_pointer>]`. The function validates that every
/// cited path exists in `artifacts_index` *before* emitting any output: on
/// the first miss it returns [`NarrativeError::UnknownArtifact`] with the
/// offending path and produces no partial string (Requirement 8.5).
///
/// Determinism: byte-identical inputs produce a byte-identical `String`,
/// because step ordering is preserved verbatim and the only data-dependent
/// branches are the `contains` check and the bullet count.
///
/// _Requirements: 8.5_
pub fn build_narrative(
    steps: &[NarrativeStep],
    artifacts_index: &BTreeSet<String>,
) -> Result<String, NarrativeError> {
    // Validate every citation up-front so a violation aborts before any
    // byte of markdown is materialised.
    for step in steps {
        for metric in &step.key_metrics {
            if !artifacts_index.contains(&metric.artifact_path) {
                return Err(NarrativeError::UnknownArtifact {
                    path: metric.artifact_path.clone(),
                });
            }
        }
    }

    let mut out = String::new();
    out.push_str("# Audit Snapshot Narrative\n");

    // Empty case: exactly the heading + single trailing LF, no blank line.
    if steps.is_empty() {
        return Ok(out);
    }

    // Blank line between heading and first step.
    out.push('\n');

    for (i, step) in steps.iter().enumerate() {
        if i > 0 {
            // Blank-line separator between consecutive steps. Equivalent to
            // "each step has a trailing blank line, except the last", which
            // keeps the file ending at a single trailing LF.
            out.push('\n');
        }

        // Heading for this step.
        let _ = writeln!(out, "## Step {}: {}", step.id, step.display_name);
        // Blank line after the `##` heading.
        out.push('\n');

        // Algorithm + parameters sentence.
        let _ = writeln!(
            out,
            "Algorithm `{}` with parameters: {}.",
            step.algorithm, step.params_summary
        );
        // Blank line before the bullet list.
        out.push('\n');

        // Bullet list of key metrics, preserving caller-supplied order.
        for metric in &step.key_metrics {
            let _ = writeln!(
                out,
                "- {}: {} [{}#{}]",
                metric.label, metric.value, metric.artifact_path, metric.json_pointer
            );
        }
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn idx(paths: &[&str]) -> BTreeSet<String> {
        paths.iter().map(|p| (*p).to_owned()).collect()
    }

    fn sample_step(
        id: &str,
        algorithm: &str,
        display_name: &str,
        params: &str,
        metrics: &[(&str, &str, &str, &str)],
    ) -> NarrativeStep {
        NarrativeStep {
            id: id.to_owned(),
            algorithm: algorithm.to_owned(),
            display_name: display_name.to_owned(),
            params_summary: params.to_owned(),
            key_metrics: metrics
                .iter()
                .map(|(l, v, p, j)| KeyMetric {
                    label: (*l).to_owned(),
                    value: (*v).to_owned(),
                    artifact_path: (*p).to_owned(),
                    json_pointer: (*j).to_owned(),
                })
                .collect(),
        }
    }

    #[test]
    fn happy_path_two_steps_two_metrics_each() {
        let steps = vec![
            sample_step(
                "step-1",
                "tableone",
                "Baseline characteristics",
                "by=treatment",
                &[
                    ("N total", "120", "artifacts/step-1/result.json", "n"),
                    (
                        "Mean age",
                        "45.6",
                        "artifacts/step-1/result.json",
                        "mean_age",
                    ),
                ],
            ),
            sample_step(
                "step-2",
                "ttest",
                "Two-sample t-test",
                "y=age, group=treatment",
                &[
                    (
                        "Effect estimate",
                        "1.234",
                        "artifacts/step-2/result.json",
                        "estimate",
                    ),
                    ("p-value", "0.045", "artifacts/step-2/result.json", "p_value"),
                ],
            ),
        ];
        let index = idx(&[
            "artifacts/step-1/result.json",
            "artifacts/step-2/result.json",
        ]);

        let out = build_narrative(&steps, &index).expect("happy path builds narrative");

        assert!(
            out.starts_with("# Audit Snapshot Narrative\n"),
            "missing top-level heading: {out:?}"
        );
        assert!(
            out.contains("## Step step-1: Baseline characteristics"),
            "missing step-1 heading: {out:?}"
        );
        assert!(
            out.contains("## Step step-2: Two-sample t-test"),
            "missing step-2 heading: {out:?}"
        );
        // All four citations present.
        assert!(out.contains("[artifacts/step-1/result.json#n]"));
        assert!(out.contains("[artifacts/step-1/result.json#mean_age]"));
        assert!(out.contains("[artifacts/step-2/result.json#estimate]"));
        assert!(out.contains("[artifacts/step-2/result.json#p_value]"));
        // Algorithm sentences with backticks and trailing period.
        assert!(out.contains("Algorithm `tableone` with parameters: by=treatment."));
        assert!(out.contains("Algorithm `ttest` with parameters: y=age, group=treatment."));
        // Bullets render with the exact prefix.
        assert!(out.contains("\n- N total: 120 [artifacts/step-1/result.json#n]\n"));
    }

    #[test]
    fn unknown_artifact_returns_structured_error_with_path() {
        let steps = vec![sample_step(
            "step-7",
            "anova",
            "One-way ANOVA",
            "y=score, group=arm",
            &[
                ("F", "3.21", "artifacts/step-7/result.json", "f_stat"),
                // This one is missing from the index.
                ("p", "0.04", "artifacts/step-7/MISSING.json", "p_value"),
            ],
        )];
        let index = idx(&["artifacts/step-7/result.json"]);

        let err = build_narrative(&steps, &index).expect_err("must reject unknown artifact");
        match err {
            NarrativeError::UnknownArtifact { path } => {
                assert_eq!(path, "artifacts/step-7/MISSING.json");
            }
        }
    }

    #[test]
    fn empty_steps_produces_only_heading_with_single_trailing_lf() {
        let index = BTreeSet::new();
        let out = build_narrative(&[], &index).expect("empty steps is allowed");
        assert_eq!(
            out, "# Audit Snapshot Narrative\n",
            "empty narrative must be exactly heading + single trailing LF"
        );
    }

    #[test]
    fn output_ends_with_single_trailing_lf() {
        let steps = vec![sample_step(
            "step-1",
            "tableone",
            "Baseline",
            "by=treatment",
            &[("N", "10", "artifacts/step-1/result.json", "n")],
        )];
        let index = idx(&["artifacts/step-1/result.json"]);
        let out = build_narrative(&steps, &index).unwrap();
        assert!(out.ends_with('\n'), "must end with LF");
        assert!(
            !out.ends_with("\n\n"),
            "must end with a single trailing LF, not a blank line: {out:?}"
        );
    }

    #[test]
    fn output_contains_no_carriage_return() {
        let steps = vec![
            sample_step(
                "step-1",
                "tableone",
                "Baseline",
                "by=treatment",
                &[("N", "10", "artifacts/step-1/result.json", "n")],
            ),
            sample_step(
                "step-2",
                "ttest",
                "T-test",
                "y=age",
                &[(
                    "Estimate",
                    "1.0",
                    "artifacts/step-2/result.json",
                    "estimate",
                )],
            ),
        ];
        let index = idx(&[
            "artifacts/step-1/result.json",
            "artifacts/step-2/result.json",
        ]);
        let out = build_narrative(&steps, &index).unwrap();
        assert!(
            !out.contains('\r'),
            "narrative must use LF-only line endings, found CR in {out:?}"
        );
    }

    #[test]
    fn determinism_same_inputs_produce_byte_identical_output() {
        let make = || {
            vec![
                sample_step(
                    "step-1",
                    "tableone",
                    "Baseline",
                    "by=treatment",
                    &[
                        ("N", "120", "artifacts/step-1/result.json", "n"),
                        ("Mean", "45.6", "artifacts/step-1/result.json", "mean"),
                    ],
                ),
                sample_step(
                    "step-2",
                    "ttest",
                    "T-test",
                    "y=age",
                    &[(
                        "Estimate",
                        "1.234",
                        "artifacts/step-2/result.json",
                        "estimate",
                    )],
                ),
            ]
        };
        let s1 = make();
        let s2 = make();
        let index = idx(&[
            "artifacts/step-1/result.json",
            "artifacts/step-2/result.json",
        ]);

        let a = build_narrative(&s1, &index).unwrap();
        let b = build_narrative(&s2, &index).unwrap();
        assert_eq!(a.as_bytes(), b.as_bytes(), "determinism violated");
    }

    #[test]
    fn metrics_render_in_caller_supplied_order() {
        // Reverse-alphabetical labels prove we don't sort.
        let steps = vec![sample_step(
            "step-1",
            "tableone",
            "Baseline",
            "by=treatment",
            &[
                ("zeta", "1", "artifacts/step-1/result.json", "z"),
                ("alpha", "2", "artifacts/step-1/result.json", "a"),
            ],
        )];
        let index = idx(&["artifacts/step-1/result.json"]);
        let out = build_narrative(&steps, &index).unwrap();
        let zpos = out.find("- zeta: 1").expect("zeta bullet present");
        let apos = out.find("- alpha: 2").expect("alpha bullet present");
        assert!(zpos < apos, "metrics must appear in caller order");
    }

    #[test]
    fn first_unknown_artifact_short_circuits_validation() {
        // The second step also has an unknown citation, but the first step's
        // miss must be the one reported (validation walks in order).
        let steps = vec![
            sample_step(
                "step-1",
                "tableone",
                "Baseline",
                "by=arm",
                &[("N", "10", "artifacts/step-1/MISSING_A.json", "n")],
            ),
            sample_step(
                "step-2",
                "ttest",
                "T-test",
                "y=age",
                &[("E", "1.0", "artifacts/step-2/MISSING_B.json", "estimate")],
            ),
        ];
        let index = idx(&[]);
        let err = build_narrative(&steps, &index).expect_err("must error");
        match err {
            NarrativeError::UnknownArtifact { path } => {
                assert_eq!(path, "artifacts/step-1/MISSING_A.json");
            }
        }
    }
}
