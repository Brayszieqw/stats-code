//! Property tests for the Workflow YAML round-trip module.
//!
//! Properties 13–16 from the parity-and-multilang-sidecar spec (tasks 5.4–5.7).
//!
//! - P13: Document-side round-trip preserves bytes.
//! - P14: Model-side round-trip preserves structure.
//! - P15: Canonical pretty-print is deterministic.
//! - P16: Malformed input is rejected cleanly with structured errors.

use proptest::prelude::*;

use stats_code::snapshot::workflow_yaml::{
    parse, pretty_print, ArtifactRef, InputDataset, LlmRef, ReferenceSoftwareRef, Workflow,
    WorkflowStep, WorkflowYamlError,
};

// ---------------------------------------------------------------------------
// Strategies
// ---------------------------------------------------------------------------

/// Generate a valid 64-character lowercase hex SHA256 string.
fn arb_sha256() -> impl Strategy<Value = String> {
    "[0-9a-f]{64}"
}

/// Generate a non-empty identifier-like string safe for YAML plain scalars.
/// Avoids characters that would require quoting or could be misinterpreted.
fn arb_plain_id() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9_]{0,15}"
}

/// Generate a path-like string (no quoting issues).
fn arb_path() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9_/.-]{0,30}"
}

/// Generate an ISO-8601 UTC timestamp string.
fn arb_timestamp() -> impl Strategy<Value = String> {
    (2020u32..2030, 1u32..13, 1u32..29, 0u32..24, 0u32..60, 0u32..60).prop_map(
        |(y, m, d, h, min, s)| {
            format!("{y:04}-{m:02}-{d:02}T{h:02}:{min:02}:{s:02}Z")
        },
    )
}

/// Generate an arbitrary `ArtifactRef`.
fn arb_artifact_ref() -> impl Strategy<Value = ArtifactRef> {
    (arb_path(), arb_sha256()).prop_map(|(path, sha256)| ArtifactRef { path, sha256 })
}

/// Generate an optional `ReferenceSoftwareRef`.
fn arb_reference_software() -> impl Strategy<Value = Option<ReferenceSoftwareRef>> {
    prop_oneof![
        3 => Just(None),
        1 => (arb_plain_id(), "[0-9]{1,2}\\.[0-9]{1,2}\\.[0-9]{1,2}")
            .prop_map(|(name, version)| Some(ReferenceSoftwareRef { name, version })),
    ]
}

/// Generate an optional `LlmRef`.
fn arb_llm() -> impl Strategy<Value = Option<LlmRef>> {
    prop_oneof![
        3 => Just(None),
        1 => (arb_plain_id(), arb_plain_id())
            .prop_map(|(provider, model)| Some(LlmRef { provider, model })),
    ]
}

/// Generate a simple `serde_json::Value` suitable for `params`.
/// We restrict to structures that round-trip cleanly through YAML:
/// - Maps with string keys and scalar/array/map values
/// - No NaN/Inf floats (not representable in JSON)
fn arb_params() -> impl Strategy<Value = serde_json::Value> {
    let leaf = prop_oneof![
        Just(serde_json::Value::Null),
        any::<bool>().prop_map(serde_json::Value::Bool),
        (-1000i64..1000).prop_map(|n| serde_json::Value::Number(n.into())),
        arb_plain_id().prop_map(serde_json::Value::String),
    ];

    leaf.prop_recursive(2, 16, 4, |inner| {
        prop_oneof![
            // Array of values
            prop::collection::vec(inner.clone(), 0..4)
                .prop_map(serde_json::Value::Array),
            // Object with sorted string keys
            prop::collection::btree_map(arb_plain_id(), inner, 0..4)
                .prop_map(|m| serde_json::Value::Object(m.into_iter().collect())),
        ]
    })
}

/// Generate an arbitrary `WorkflowStep`.
fn arb_step(step_index: usize) -> impl Strategy<Value = WorkflowStep> {
    (
        arb_plain_id(),
        arb_params(),
        prop::collection::vec(arb_artifact_ref(), 0..3),
        prop::collection::vec(arb_artifact_ref(), 0..3),
        arb_reference_software(),
        arb_llm(),
        arb_timestamp(),
        arb_timestamp(),
    )
        .prop_map(
            move |(algorithm, params, inputs, outputs, reference_software, llm, started, ended)| {
                WorkflowStep {
                    // Ensure unique step ids by incorporating the index.
                    id: format!("step-{step_index}"),
                    algorithm,
                    params,
                    inputs,
                    outputs,
                    reference_software,
                    llm,
                    started_at_utc: started,
                    ended_at_utc: ended,
                }
            },
        )
}

/// Generate an arbitrary valid `Workflow` (0–3 steps).
fn arb_workflow() -> impl Strategy<Value = Workflow> {
    (arb_path(), arb_sha256(), 0usize..4).prop_flat_map(|(path, sha256, num_steps)| {
        let steps_strategy: Vec<_> = (0..num_steps).map(arb_step).collect();
        (Just(path), Just(sha256), steps_strategy).prop_map(|(path, sha256, steps)| Workflow {
            schema_version: 1,
            input_dataset: InputDataset { path, sha256 },
            steps,
        })
    })
}

/// Generate a valid YAML document by constructing a Workflow and
/// pretty-printing it in canonical mode. This guarantees the document
/// is parseable and exercises the document-side round-trip.
fn arb_valid_workflow_yaml_doc() -> impl Strategy<Value = Vec<u8>> {
    arb_workflow().prop_map(|w| pretty_print(&w, None))
}

/// Category of malformed input for Property 16.
#[derive(Debug, Clone)]
enum MalformedCategory {
    /// Input exceeds 10 MiB size cap.
    Oversize,
    /// Input contains non-UTF-8 bytes.
    NonUtf8,
    /// Valid UTF-8 but invalid YAML syntax.
    BadYamlSyntax,
    /// Valid YAML but violates the Workflow schema.
    SchemaViolation,
}

/// Generate malformed input bytes that should be rejected by `parse`.
fn arb_malformed_input() -> impl Strategy<Value = (Vec<u8>, MalformedCategory)> {
    prop_oneof![
        // (a) Oversize: >10 MiB. We generate exactly 10 MiB + 1..256 bytes.
        (1usize..256).prop_map(|extra| {
            let size = 10 * 1024 * 1024 + extra;
            // Fill with valid YAML-ish content to ensure the size gate is
            // what rejects it, not UTF-8 or syntax.
            let mut bytes = Vec::with_capacity(size);
            bytes.extend_from_slice(b"# padding\n");
            bytes.resize(size, b'x');
            (bytes, MalformedCategory::Oversize)
        }),
        // (b) Non-UTF-8: inject invalid byte sequences.
        //
        // `0xFF` (and `0xFE`) can never appear in *any* position of a valid
        // UTF-8 sequence, so prefixing the random tail with `0xFF` guarantees
        // the whole buffer is invalid UTF-8 — even when the random bytes in
        // `0x80..=0xFD` would themselves have formed a valid multi-byte
        // sequence (e.g. `0xC2 0xA0` decodes to U+00A0). Without this anchor
        // the generator could emit accidentally-valid UTF-8 that is merely
        // bad YAML, which `parse` correctly reports as `yaml_syntax_error`
        // rather than `non_utf8`, producing a spurious category mismatch.
        prop::collection::vec(0x80u8..=0xFD, 0..8).prop_map(|tail| {
            let mut bytes = b"schema_version: 1\n".to_vec();
            bytes.push(0xFF);
            bytes.extend_from_slice(&tail);
            (bytes, MalformedCategory::NonUtf8)
        }),
        // (c) Bad YAML syntax: valid UTF-8 but not well-formed YAML.
        prop_oneof![
            Just(b"[unclosed\n".to_vec()),
            Just(b":\n  - :\n    - : [\n".to_vec()),
            Just(b"---\n{key: [}\n".to_vec()),
            Just(b"---\n*undefined_alias\n".to_vec()),
            Just(b"key: &anchor\n  <<: *missing\n  bad: [\n".to_vec()),
        ].prop_map(|bytes| (bytes, MalformedCategory::BadYamlSyntax)),
        // (d) Schema violations: valid YAML but missing/wrong fields.
        prop_oneof![
            // Missing schema_version
            Just(b"input_dataset:\n  path: data.csv\n  sha256: 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\nsteps: []\n".to_vec()),
            // Wrong type for schema_version (string instead of int)
            Just(b"schema_version: \"1\"\ninput_dataset:\n  path: data.csv\n  sha256: 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\nsteps: []\n".to_vec()),
            // Unsupported schema_version
            Just(b"schema_version: 99\ninput_dataset:\n  path: data.csv\n  sha256: 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\nsteps: []\n".to_vec()),
            // Invalid SHA256 (uppercase)
            Just(b"schema_version: 1\ninput_dataset:\n  path: data.csv\n  sha256: 0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF\nsteps: []\n".to_vec()),
            // Invalid SHA256 (too short)
            Just(b"schema_version: 1\ninput_dataset:\n  path: data.csv\n  sha256: abcd\nsteps: []\n".to_vec()),
            // Missing input_dataset
            Just(b"schema_version: 1\nsteps: []\n".to_vec()),
            // Missing steps
            Just(b"schema_version: 1\ninput_dataset:\n  path: data.csv\n  sha256: 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\n".to_vec()),
            // Duplicate step ids
            Just(b"schema_version: 1\ninput_dataset:\n  path: data.csv\n  sha256: 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\nsteps:\n  - id: dup\n    algorithm: x\n    params: {}\n    inputs: []\n    outputs:\n      - path: a.json\n        sha256: 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\n    started_at_utc: \"2024-01-01T00:00:00Z\"\n    ended_at_utc: \"2024-01-01T00:00:01Z\"\n  - id: dup\n    algorithm: y\n    params: {}\n    inputs: []\n    outputs:\n      - path: b.json\n        sha256: 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\n    started_at_utc: \"2024-01-01T00:00:02Z\"\n    ended_at_utc: \"2024-01-01T00:00:03Z\"\n".to_vec()),
            // Root is a scalar, not a mapping
            Just(b"just a plain string\n".to_vec()),
        ].prop_map(|bytes| (bytes, MalformedCategory::SchemaViolation)),
    ]
}

// ---------------------------------------------------------------------------
// Properties
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig { cases: 64, failure_persistence: None, .. ProptestConfig::default() })]

    /// **Property 13: Workflow YAML document-side round-trip preserves bytes**
    ///
    /// **Validates: Requirements 8.2, 11.4**
    ///
    /// For any valid YAML document D produced by canonical pretty-print:
    /// `pretty_print(parse(D).0, Some(parse(D).1)) == D`
    #[test]
    fn workflow_yaml_document_side_round_trip(doc_bytes in arb_valid_workflow_yaml_doc()) {
        let (workflow, yaml_doc) = parse(&doc_bytes)
            .expect("canonical pretty-print output must always be parseable");
        let re_emitted = pretty_print(&workflow, Some(&yaml_doc));
        prop_assert_eq!(
            &re_emitted, &doc_bytes,
            "document-side round-trip must preserve bytes exactly"
        );
    }

    /// **Property 14: Workflow YAML model-side round-trip preserves structure**
    ///
    /// **Validates: Requirements 11.3, 8.1**
    ///
    /// For any arbitrary Workflow W:
    /// `parse(pretty_print(W, None)).0` is structurally equal to W.
    #[test]
    fn workflow_yaml_model_side_round_trip(workflow in arb_workflow()) {
        let canonical_bytes = pretty_print(&workflow, None);
        let (parsed, _) = parse(&canonical_bytes).unwrap_or_else(|e| {
            panic!(
                "canonical pretty-print of a valid Workflow must be parseable, got error: {e}"
            );
        });
        prop_assert_eq!(
            &parsed, &workflow,
            "model-side round-trip must preserve Workflow structure"
        );
    }

    /// **Property 15: Workflow YAML canonical pretty-print is deterministic**
    ///
    /// **Validates: Requirements 11.7**
    ///
    /// For any Workflow W, `pretty_print(W.clone(), None) == pretty_print(W, None)`.
    /// Two structurally equal workflows produce identical bytes.
    #[test]
    fn workflow_yaml_canonical_deterministic(workflow in arb_workflow()) {
        let first = pretty_print(&workflow, None);
        let second = pretty_print(&workflow.clone(), None);
        prop_assert_eq!(
            &first, &second,
            "canonical pretty-print must be deterministic: same Workflow → same bytes"
        );

        // Additionally verify it's a fixpoint: pretty_print(parse(output).0, None) == output
        let (reparsed, _) = parse(&first)
            .expect("canonical output must be parseable");
        let third = pretty_print(&reparsed, None);
        prop_assert_eq!(
            &third, &first,
            "canonical pretty-print must be a fixpoint"
        );
    }

    /// **Property 16: Workflow YAML parser rejects malformed input cleanly**
    ///
    /// **Validates: Requirements 11.5, 11.6**
    ///
    /// For any malformed input (oversize, non-UTF-8, bad YAML syntax, or
    /// schema violation), `parse` returns `Err(WorkflowYamlError)` with:
    /// - A non-empty `rule_violated` field.
    /// - For schema violations: `field` is populated.
    /// - No partial `Workflow` is accessible (the `Err` variant carries no model).
    #[test]
    fn workflow_yaml_rejects_malformed_input((input_bytes, category) in arb_malformed_input()) {
        let result = parse(&input_bytes);
        prop_assert!(
            result.is_err(),
            "malformed input ({:?}) must be rejected, but parse returned Ok",
            category
        );
        let err: WorkflowYamlError = result.unwrap_err();

        // rule_violated must be non-empty.
        prop_assert!(
            !err.rule_violated.is_empty(),
            "WorkflowYamlError.rule_violated must be non-empty"
        );

        // Verify the error category matches expectations.
        match category {
            MalformedCategory::Oversize => {
                prop_assert_eq!(
                    err.rule_violated, "size_cap_exceeded",
                    "oversize input must trigger size_cap_exceeded rule"
                );
            }
            MalformedCategory::NonUtf8 => {
                prop_assert_eq!(
                    err.rule_violated, "non_utf8",
                    "non-UTF-8 input must trigger non_utf8 rule"
                );
            }
            MalformedCategory::BadYamlSyntax => {
                prop_assert_eq!(
                    err.rule_violated, "yaml_syntax_error",
                    "bad YAML syntax must trigger yaml_syntax_error rule"
                );
                // YAML syntax errors should have line/column >= 1.
                prop_assert!(
                    err.line >= 1,
                    "YAML syntax error must report line >= 1, got {}",
                    err.line
                );
                prop_assert!(
                    err.column >= 1,
                    "YAML syntax error must report column >= 1, got {}",
                    err.column
                );
            }
            MalformedCategory::SchemaViolation => {
                // Schema violations use one of the RULE_* constants that are
                // NOT size_cap_exceeded, non_utf8, or yaml_syntax_error.
                prop_assert!(
                    err.rule_violated != "size_cap_exceeded"
                        && err.rule_violated != "non_utf8"
                        && err.rule_violated != "yaml_syntax_error",
                    "schema violation must not use pre-schema rule, got: {}",
                    err.rule_violated
                );
                // Schema violations must have a field path populated.
                prop_assert!(
                    err.field.is_some(),
                    "schema violation must populate the `field` path, rule={}",
                    err.rule_violated
                );
            }
        }
    }
}
