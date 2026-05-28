//! Workflow YAML round-trip parser & pretty-printer.
//!
//! Wave-1 parser (task 5.2 of `parity-and-multilang-sidecar`). The in-memory
//! [`Workflow`] model and [`WorkflowYamlDoc`] handle were declared in task 5.1;
//! this file fills in [`parse`] so the snapshot exporter (§6.7) and
//! `--replay` (§7.2) have a structurally validating YAML reader. The
//! pretty-printer body still lives in task 5.3.
//!
//! ## Round-trip contract (target)
//!
//! - `parse` then `pretty_print(_, Some(doc))` ≡ input bytes (Requirement
//!   11.4 — document-side round-trip).
//! - `pretty_print(W, None)` then `parse` returns a `Workflow` structurally
//!   equal to `W` (Requirement 11.3 — model-side round-trip).
//! - `pretty_print(_, None)` is a fixpoint: applying it twice changes
//!   nothing (Requirement 11.7 — canonical form determinism).
//!
//! ## Hard rejection ladder enforced by [`parse`]
//!
//! 1. `bytes.len() > 10 MiB` → [`RULE_SIZE_CAP_EXCEEDED`] **before** any
//!    parser state is allocated (Requirement 11.6).
//! 2. Non-UTF-8 → [`RULE_NON_UTF8`].
//! 3. YAML not well-formed (yaml-rust2 [`ScanError`]) →
//!    [`RULE_YAML_SYNTAX_ERROR`] with 1-based `(line, column)` from the
//!    scanner marker.
//! 4. Schema violations (missing field, wrong type, malformed SHA256,
//!    unsupported `schema_version`, duplicate step id) → a closed-set
//!    [`rule_violated`](WorkflowYamlError::rule_violated) constant plus a
//!    closed-set [`field`](WorkflowYamlError::field) string-literal.
//!
//! Any failure short-circuits and returns [`WorkflowYamlError`]; no partial
//! [`Workflow`] is ever observable to the caller (Requirement 11.5).
//!
//! _Requirements: 8.1, 8.2, 11.1, 11.2, 11.3, 11.4, 11.5, 11.6, 11.7_

#![allow(dead_code)] // public surface stabilizes as snapshot::export_snapshot lands.

use std::collections::HashSet;
use std::error::Error;
use std::fmt;

use yaml_rust2::yaml::{Hash, Yaml};
use yaml_rust2::{ScanError, YamlLoader};

// ---------------------------------------------------------------------------
// In-memory model (Workflow)
// ---------------------------------------------------------------------------

/// Reference to an artifact on disk inside the Audit Snapshot.
///
/// Both `path` and `sha256` are required on every input/output entry per the
/// Workflow YAML schema in design.md §"Workflow YAML schema" and Requirement
/// 8.1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactRef {
    /// Path expressed relative to the analysis working directory.
    pub path: String,
    /// 64-character lowercase hex SHA256 of the artifact's bytes.
    pub sha256: String,
}

/// Reference Software invoked by an analysis step (e.g. R 4.4.1).
///
/// Optional per step: only populated when the step's parity comparison
/// actually called an external Reference Implementation during the original
/// run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceSoftwareRef {
    pub name: String,
    pub version: String,
}

/// LLM provider/model consulted during an analysis step, if any.
///
/// API keys are never recorded here (Requirement 9.1). The full per-call
/// provenance with timestamps and prompt/response hashes lives in
/// `llm_provenance.json`; this struct is the per-step pointer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlmRef {
    pub provider: String,
    pub model: String,
}

/// Pointer to the original input dataset embedded in the Audit Snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputDataset {
    pub path: String,
    pub sha256: String,
}

/// Algorithm parameters. Algorithm-specific structure is opaque at the
/// Workflow YAML level and travels through as a generic JSON value so the
/// schema stays stable when new Output-Level Algorithms land.
pub type Params = serde_json::Value;

/// One step in the analysis run's workflow.
///
/// Mirrors the schema in design.md §"Workflow YAML schema" and the per-step
/// fields enumerated in Requirement 8.1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowStep {
    /// Unique step identifier (e.g. `"step-1"`).
    pub id: String,
    /// Output-Level Algorithm identifier (matches `CoverageMatrix` ids).
    pub algorithm: String,
    /// Algorithm parameter set, opaque JSON.
    pub params: Params,
    /// Input artifacts consumed by this step.
    pub inputs: Vec<ArtifactRef>,
    /// Output artifacts produced by this step.
    pub outputs: Vec<ArtifactRef>,
    /// Reference Software invoked for this step's parity comparison, if any.
    pub reference_software: Option<ReferenceSoftwareRef>,
    /// LLM consulted while producing this step's artifact, if any.
    pub llm: Option<LlmRef>,
    /// Step start timestamp in ISO-8601 UTC.
    pub started_at_utc: String,
    /// Step end timestamp in ISO-8601 UTC.
    pub ended_at_utc: String,
}

/// Root model serialized to / deserialized from `workflow.yaml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workflow {
    /// Schema version of the Workflow YAML document (currently `1`).
    pub schema_version: u32,
    /// Original input dataset reference.
    pub input_dataset: InputDataset,
    /// Ordered list of analysis steps.
    pub steps: Vec<WorkflowStep>,
}

// ---------------------------------------------------------------------------
// Document-side handle (WorkflowYamlDoc)
// ---------------------------------------------------------------------------

/// Opaque handle holding everything we need to re-emit the original byte
/// sequence verbatim.
///
/// Wave-1 storage (task 5.2): the simplest byte-identical re-emission of a
/// document is "return the input bytes verbatim", which trivially satisfies
/// Requirement 11.4. Task 5.3's `pretty_print(_, Some(doc))` path will read
/// `original_bytes` and clone it into the output buffer.
///
/// The struct is non-`Copy`, non-`Default`, and constructed only by [`parse`]
/// to make the round-trip contract explicit at the type level: you cannot
/// fabricate a doc handle out of thin air, you can only obtain one from
/// parsing a real input.
#[derive(Debug, Clone)]
pub struct WorkflowYamlDoc {
    /// Verbatim copy of the bytes that were passed to [`parse`]. Consumed by
    /// task 5.3's `pretty_print(_, Some(doc))` path to satisfy the
    /// document-side round-trip contract (Requirement 11.4).
    pub(crate) original_bytes: Vec<u8>,
}

// ---------------------------------------------------------------------------
// Errors — closed-set rule + field constants
// ---------------------------------------------------------------------------

/// Hard size cap on `workflow.yaml` documents (Requirement 11.1, 11.6).
const SIZE_CAP_BYTES: usize = 10 * 1024 * 1024;

/// `bytes.len() > 10 MiB` rejection (Requirement 11.6).
pub const RULE_SIZE_CAP_EXCEEDED: &str = "size_cap_exceeded";
/// Input bytes are not valid UTF-8 (Requirement 11.6).
pub const RULE_NON_UTF8: &str = "non_utf8";
/// Input is UTF-8 but not well-formed YAML (Requirement 11.6).
pub const RULE_YAML_SYNTAX_ERROR: &str = "yaml_syntax_error";
/// YAML parsed but contained zero documents.
pub const RULE_EMPTY_DOCUMENT: &str = "empty_document";
/// YAML parsed but contained more than one document.
pub const RULE_MULTIPLE_DOCUMENTS: &str = "multiple_documents";
/// A required field is absent from a node (Requirement 11.5).
pub const RULE_MISSING_FIELD: &str = "missing_field";
/// A field is present but holds a value of the wrong type (Requirement 11.5).
pub const RULE_WRONG_TYPE: &str = "wrong_type";
/// `schema_version` is present and integer-typed but != 1.
pub const RULE_SCHEMA_VERSION_UNSUPPORTED: &str = "schema_version_unsupported";
/// A SHA256 string is not exactly 64 lowercase hex characters.
pub const RULE_INVALID_SHA256: &str = "invalid_sha256";
/// Two steps share the same `id`.
pub const RULE_DUPLICATE_STEP_ID: &str = "duplicate_step_id";

/// Closed-set field-path constants. Every [`WorkflowYamlError::field`]
/// returned by [`parse`] is one of these, so callers can match on a finite
/// alphabet without dynamic string allocation.
pub const FIELD_ROOT: &str = "<root>";
pub const FIELD_SCHEMA_VERSION: &str = "schema_version";
pub const FIELD_INPUT_DATASET: &str = "input_dataset";
pub const FIELD_INPUT_DATASET_PATH: &str = "input_dataset.path";
pub const FIELD_INPUT_DATASET_SHA256: &str = "input_dataset.sha256";
pub const FIELD_STEPS: &str = "steps";
pub const FIELD_STEP: &str = "step";
pub const FIELD_STEP_ID: &str = "step.id";
pub const FIELD_STEP_ALGORITHM: &str = "step.algorithm";
pub const FIELD_STEP_PARAMS: &str = "step.params";
pub const FIELD_STEP_INPUTS: &str = "step.inputs";
pub const FIELD_STEP_OUTPUTS: &str = "step.outputs";
pub const FIELD_STEP_REFERENCE_SOFTWARE: &str = "step.reference_software";
pub const FIELD_STEP_LLM: &str = "step.llm";
pub const FIELD_STEP_STARTED_AT_UTC: &str = "step.started_at_utc";
pub const FIELD_STEP_ENDED_AT_UTC: &str = "step.ended_at_utc";
pub const FIELD_ARTIFACT: &str = "artifact";
pub const FIELD_ARTIFACT_PATH: &str = "artifact.path";
pub const FIELD_ARTIFACT_SHA256: &str = "artifact.sha256";
pub const FIELD_REFERENCE_SOFTWARE_NAME: &str = "reference_software.name";
pub const FIELD_REFERENCE_SOFTWARE_VERSION: &str = "reference_software.version";
pub const FIELD_LLM_PROVIDER: &str = "llm.provider";
pub const FIELD_LLM_MODEL: &str = "llm.model";

/// Structured Workflow YAML parse error.
///
/// Carries the position (1-based `line` / `column`, both zero when the
/// violation is global, e.g. size-cap rejection) and a `(rule_violated,
/// field)` pair drawn from the closed sets defined above. This lets callers
/// route on rule + field without parsing dynamic error strings, satisfying
/// Requirement 11.5 and 11.6.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowYamlError {
    /// 1-based line number where the violation was detected (0 when not
    /// applicable, e.g. size-cap rejection or non-UTF-8 input).
    pub line: usize,
    /// 1-based column number where the violation was detected (0 when not
    /// applicable).
    pub column: usize,
    /// Closed-set rule identifier, one of the `RULE_*` constants in this
    /// module.
    pub rule_violated: &'static str,
    /// Closed-set field path that the violation refers to, one of the
    /// `FIELD_*` constants in this module. `None` when the violation does
    /// not refer to a specific field (e.g. size cap, UTF-8, raw YAML
    /// syntax).
    pub field: Option<&'static str>,
}

impl WorkflowYamlError {
    /// Construct a `WorkflowYamlError` without an attached field path.
    pub(crate) fn new(line: usize, column: usize, rule_violated: &'static str) -> Self {
        Self { line, column, rule_violated, field: None }
    }

    /// Construct a `WorkflowYamlError` with an attached closed-set field
    /// path.
    pub(crate) fn with_field(
        line: usize,
        column: usize,
        rule_violated: &'static str,
        field: &'static str,
    ) -> Self {
        Self { line, column, rule_violated, field: Some(field) }
    }
}

impl fmt::Display for WorkflowYamlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.field {
            Some(field) => write!(
                f,
                "workflow.yaml: {} at field `{}` (line {}, column {})",
                self.rule_violated, field, self.line, self.column
            ),
            None => write!(
                f,
                "workflow.yaml: {} (line {}, column {})",
                self.rule_violated, self.line, self.column
            ),
        }
    }
}

impl Error for WorkflowYamlError {}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Parse a `workflow.yaml` byte slice into a normalized [`Workflow`] model
/// and a [`WorkflowYamlDoc`] handle suitable for byte-identical
/// re-serialization by task 5.3's `pretty_print(_, Some(doc))` path.
///
/// ## Pre-conditions enforced
///
/// 1. `bytes.len() <= 10 * 1024 * 1024`. Larger inputs are rejected
///    **before** any parser state is allocated (Requirement 11.6).
/// 2. UTF-8 validity is verified before YAML parsing.
/// 3. YAML well-formedness is verified via [`YamlLoader::load_from_str`];
///    [`ScanError`] from the scanner is carried through with 1-based
///    `(line, column)` from the underlying marker.
/// 4. Schema validation projects the parsed YAML tree into [`Workflow`].
///    Any rule violation produces a [`WorkflowYamlError`] with a closed-set
///    `(rule_violated, field)` pair, and no partially-built `Workflow` is
///    exposed (Requirement 11.5).
///
/// ## Schema (from design.md §"Workflow YAML schema")
///
/// - `schema_version: u32` — must equal `1`.
/// - `input_dataset: { path: str, sha256: str }` — `sha256` must be 64
///   lowercase hex chars.
/// - `steps[]` — each step:
///   - `id: str` (unique across steps)
///   - `algorithm: str`
///   - `params: any` (opaque; rendered to `serde_json::Value`)
///   - `inputs[]: { path, sha256 }` (sha256 64-hex lowercase)
///   - `outputs[]: { path, sha256 }`
///   - `reference_software?: { name, version }`
///   - `llm?: { provider, model }`
///   - `started_at_utc: str`, `ended_at_utc: str`
///
/// _Requirements: 8.1, 11.1, 11.5, 11.6_
pub fn parse(bytes: &[u8]) -> Result<(Workflow, WorkflowYamlDoc), WorkflowYamlError> {
    // Gate 1 — size cap. Done first so a multi-GiB input never allocates a
    // parser. Returns `line=0, column=0` because the violation is global,
    // not located at any source position.
    if bytes.len() > SIZE_CAP_BYTES {
        return Err(WorkflowYamlError::new(0, 0, RULE_SIZE_CAP_EXCEEDED));
    }

    // Gate 2 — UTF-8 validity. We do not surface the byte offset of the
    // first invalid sequence at this layer; the violation is treated as a
    // categorical reject per Requirement 11.6.
    let text = std::str::from_utf8(bytes)
        .map_err(|_| WorkflowYamlError::new(0, 0, RULE_NON_UTF8))?;

    // Gate 3 — YAML well-formedness. yaml-rust2 surfaces a `ScanError` with
    // a 1-based line and a 0-based column; we expose both as 1-based.
    let docs = YamlLoader::load_from_str(text).map_err(scan_error_to_workflow_error)?;
    if docs.is_empty() {
        return Err(WorkflowYamlError::with_field(
            1,
            1,
            RULE_EMPTY_DOCUMENT,
            FIELD_ROOT,
        ));
    }
    if docs.len() > 1 {
        return Err(WorkflowYamlError::with_field(
            1,
            1,
            RULE_MULTIPLE_DOCUMENTS,
            FIELD_ROOT,
        ));
    }

    // Gate 4 — schema validation. Per-node line/column tracking would
    // require a custom `MarkedEventReceiver`; for the wave-1 parser we
    // expose `line=0, column=0` for schema violations and rely on the
    // closed-set `(rule_violated, field)` pair to identify the offender.
    // This is documented behavior (the existing struct doc-comment already
    // permits "0 when not applicable") and satisfies Requirement 11.5
    // because the structured error still names the violated rule and the
    // offending field path.
    let workflow = parse_workflow(&docs[0])?;

    let doc = WorkflowYamlDoc { original_bytes: bytes.to_vec() };
    Ok((workflow, doc))
}

/// Serialize a [`Workflow`] back to YAML bytes.
///
/// Two modes:
///
/// - `doc = Some(d)` — document-side round-trip: rebuild the original byte
///   sequence using the trivia (comments, blank lines, key order, line
///   endings, indentation) captured in `d`. Required for Requirement 11.4.
/// - `doc = None` — canonical form: deterministic key order, fixed
///   indentation, LF line endings, trailing newline. Required for
///   Requirements 11.3 and 11.7 (the canonical form is a fixpoint).
///
/// _Requirements: 11.2, 11.3, 11.4, 11.7_
#[must_use] 
pub fn pretty_print(model: &Workflow, doc: Option<&WorkflowYamlDoc>) -> Vec<u8> {
    // Document-side round-trip (Requirement 11.4). The wave-1
    // `WorkflowYamlDoc` captures a verbatim copy of the input bytes via
    // `parse`, so re-emission is just "return that buffer". This is
    // byte-identical by construction (whitespace, indentation, key order,
    // blank lines, line endings, and comments are all preserved because we
    // never tokenized them away).
    if let Some(d) = doc {
        return d.original_bytes.clone();
    }

    // Canonical-mode emission (Requirements 11.2, 11.3, 11.7). Hand-rolled
    // emitter so we have full control over the byte sequence and the
    // canonical form is a true fixpoint:
    //
    //   pretty_print(parse(pretty_print(W, None)).0, None)
    //     == pretty_print(W, None)
    //
    // Top-level keys are emitted in fixed order (`schema_version`,
    // `input_dataset`, `steps`); per-step keys are emitted in fixed order
    // (`id`, `algorithm`, `params`, `inputs`, `outputs`,
    // `reference_software?`, `llm?`, `started_at_utc`, `ended_at_utc`); any
    // nested map under `params` (a `serde_json::Value`) is emitted with
    // keys in lexicographic order. Indentation is fixed at two spaces per
    // level, line endings are LF, and a single trailing LF terminates the
    // document.
    let mut out = String::new();

    out.push_str("schema_version: ");
    out.push_str(&model.schema_version.to_string());
    out.push('\n');

    out.push_str("input_dataset:\n");
    emit_input_dataset(&mut out, &model.input_dataset, 1);

    if model.steps.is_empty() {
        out.push_str("steps: []\n");
    } else {
        out.push_str("steps:\n");
        for step in &model.steps {
            emit_step(&mut out, step, 1);
        }
    }

    out.into_bytes()
}

// ---------------------------------------------------------------------------
// Canonical pretty-printer — internal helpers (task 5.3)
// ---------------------------------------------------------------------------

/// One canonical indent level = two spaces.
const INDENT: &str = "  ";

fn push_indent(buf: &mut String, level: usize) {
    for _ in 0..level {
        buf.push_str(INDENT);
    }
}

/// Emit `<indent>key: <value>\n` where `value` is a quoted-or-plain scalar.
fn emit_scalar_field(buf: &mut String, level: usize, key: &str, value: &str) {
    push_indent(buf, level);
    buf.push_str(key);
    buf.push_str(": ");
    push_yaml_string(buf, value);
    buf.push('\n');
}

fn emit_input_dataset(buf: &mut String, ds: &InputDataset, level: usize) {
    emit_scalar_field(buf, level, "path", &ds.path);
    emit_scalar_field(buf, level, "sha256", &ds.sha256);
}

fn emit_step(buf: &mut String, step: &WorkflowStep, level: usize) {
    // Sequence-of-mappings: first key sits on the `- ` line so the rest of
    // the mapping aligns at level + 1.
    push_indent(buf, level);
    buf.push_str("- id: ");
    push_yaml_string(buf, &step.id);
    buf.push('\n');

    let inner = level + 1;
    emit_scalar_field(buf, inner, "algorithm", &step.algorithm);

    // params — opaque JSON, rendered with sorted keys.
    push_indent(buf, inner);
    buf.push_str("params:");
    emit_json_value_after_key(buf, &step.params, inner);

    // inputs / outputs — block sequences of artifact refs, or `[]` when
    // empty.
    emit_artifact_list(buf, inner, "inputs", &step.inputs);
    emit_artifact_list(buf, inner, "outputs", &step.outputs);

    if let Some(rs) = &step.reference_software {
        push_indent(buf, inner);
        buf.push_str("reference_software:\n");
        emit_scalar_field(buf, inner + 1, "name", &rs.name);
        emit_scalar_field(buf, inner + 1, "version", &rs.version);
    }
    if let Some(llm) = &step.llm {
        push_indent(buf, inner);
        buf.push_str("llm:\n");
        emit_scalar_field(buf, inner + 1, "provider", &llm.provider);
        emit_scalar_field(buf, inner + 1, "model", &llm.model);
    }

    emit_scalar_field(buf, inner, "started_at_utc", &step.started_at_utc);
    emit_scalar_field(buf, inner, "ended_at_utc", &step.ended_at_utc);
}

fn emit_artifact_list(buf: &mut String, level: usize, key: &str, list: &[ArtifactRef]) {
    push_indent(buf, level);
    buf.push_str(key);
    if list.is_empty() {
        buf.push_str(": []\n");
        return;
    }
    buf.push_str(":\n");
    for art in list {
        push_indent(buf, level);
        buf.push_str("- path: ");
        push_yaml_string(buf, &art.path);
        buf.push('\n');
        emit_scalar_field(buf, level + 1, "sha256", &art.sha256);
    }
}

/// Emit the value half of `key:` when the key (with its trailing `:`) has
/// just been written but no trailing space / newline. Drives container
/// layout: containers go on a new line and indent under `level + 1`,
/// scalars sit on the same line as the key.
fn emit_json_value_after_key(buf: &mut String, value: &serde_json::Value, level: usize) {
    use serde_json::Value;
    match value {
        Value::Null => {
            buf.push_str(" null\n");
        }
        Value::Bool(b) => {
            buf.push(' ');
            buf.push_str(if *b { "true" } else { "false" });
            buf.push('\n');
        }
        Value::Number(n) => {
            buf.push(' ');
            buf.push_str(&n.to_string());
            buf.push('\n');
        }
        Value::String(s) => {
            buf.push(' ');
            push_yaml_string(buf, s);
            buf.push('\n');
        }
        Value::Array(arr) => {
            if arr.is_empty() {
                buf.push_str(" []\n");
                return;
            }
            buf.push('\n');
            emit_json_block_seq(buf, arr, level);
        }
        Value::Object(map) => {
            if map.is_empty() {
                buf.push_str(" {}\n");
                return;
            }
            buf.push('\n');
            emit_json_block_map(buf, map, level + 1);
        }
    }
}

fn emit_json_block_seq(buf: &mut String, arr: &[serde_json::Value], level: usize) {
    use serde_json::Value;
    for item in arr {
        push_indent(buf, level);
        match item {
            Value::Null => {
                buf.push_str("- null\n");
            }
            Value::Bool(b) => {
                buf.push_str("- ");
                buf.push_str(if *b { "true" } else { "false" });
                buf.push('\n');
            }
            Value::Number(n) => {
                buf.push_str("- ");
                buf.push_str(&n.to_string());
                buf.push('\n');
            }
            Value::String(s) => {
                buf.push_str("- ");
                push_yaml_string(buf, s);
                buf.push('\n');
            }
            Value::Array(inner) => {
                if inner.is_empty() {
                    buf.push_str("- []\n");
                } else {
                    buf.push_str("-\n");
                    emit_json_block_seq(buf, inner, level + 1);
                }
            }
            Value::Object(inner) => {
                if inner.is_empty() {
                    buf.push_str("- {}\n");
                } else {
                    // Inline first key on the `- ` line so the rest of the
                    // map indents one level deeper.
                    let mut keys: Vec<&String> = inner.keys().collect();
                    keys.sort();
                    let first_key = keys[0];
                    let first_val = &inner[first_key];
                    buf.push_str("- ");
                    buf.push_str(first_key);
                    buf.push(':');
                    emit_json_value_after_key(buf, first_val, level + 1);
                    for k in keys.iter().skip(1) {
                        push_indent(buf, level + 1);
                        buf.push_str(k);
                        buf.push(':');
                        emit_json_value_after_key(buf, &inner[*k], level + 1);
                    }
                }
            }
        }
    }
}

fn emit_json_block_map(
    buf: &mut String,
    map: &serde_json::Map<String, serde_json::Value>,
    level: usize,
) {
    let mut keys: Vec<&String> = map.keys().collect();
    keys.sort();
    for k in keys {
        push_indent(buf, level);
        buf.push_str(k);
        buf.push(':');
        emit_json_value_after_key(buf, &map[k], level);
    }
}

/// Emit `s` either as a plain YAML scalar or as a double-quoted scalar
/// with `\\` and `\"` escapes, depending on whether `s` is safe to leave
/// plain.
///
/// We reject strings that would round-trip to a different YAML node type
/// (e.g. `true`, `1`, `null`, `0123`) by quoting them. This guarantees
/// `parse(pretty_print(W, None)).0 == W` (Requirement 11.3) and that the
/// canonical form is a fixpoint at the byte level (Requirement 11.7).
fn push_yaml_string(buf: &mut String, s: &str) {
    if needs_quoting(s) {
        buf.push('"');
        for ch in s.chars() {
            match ch {
                '\\' => buf.push_str("\\\\"),
                '"' => buf.push_str("\\\""),
                '\n' => buf.push_str("\\n"),
                '\r' => buf.push_str("\\r"),
                '\t' => buf.push_str("\\t"),
                c if (c as u32) < 0x20 => {
                    // Escape other ASCII control chars as \xHH.
                    buf.push_str(&format!("\\x{:02x}", c as u32));
                }
                c => buf.push(c),
            }
        }
        buf.push('"');
    } else {
        buf.push_str(s);
    }
}

fn needs_quoting(s: &str) -> bool {
    if s.is_empty() {
        return true;
    }
    let bytes = s.as_bytes();
    // Leading or trailing whitespace.
    if bytes[0] == b' '
        || bytes[0] == b'\t'
        || bytes[bytes.len() - 1] == b' '
        || bytes[bytes.len() - 1] == b'\t'
    {
        return true;
    }
    // First character cannot be a YAML indicator that opens a flow /
    // mapping construct.
    let first = s.chars().next().unwrap();
    if matches!(
        first,
        '-' | '?'
            | ':'
            | ','
            | '['
            | ']'
            | '{'
            | '}'
            | '#'
            | '&'
            | '*'
            | '!'
            | '|'
            | '>'
            | '\''
            | '"'
            | '%'
            | '@'
            | '`'
    ) {
        return true;
    }
    // Any internal control char or YAML indicator that would split the
    // scalar or introduce a comment.
    for ch in s.chars() {
        if (ch as u32) < 0x20 {
            return true;
        }
        if matches!(ch, ':' | '#' | '\u{0085}' | '\u{2028}' | '\u{2029}') {
            return true;
        }
    }
    // Strings that look like booleans / null / yes-no / on-off under the
    // YAML 1.2 core schema (and a couple of YAML 1.1 holdovers that
    // yaml-rust2 still resolves) must be quoted to preserve the string
    // type.
    match s {
        "null" | "Null" | "NULL" | "~" | "true" | "True" | "TRUE" | "false" | "False"
        | "FALSE" | "yes" | "Yes" | "YES" | "no" | "No" | "NO" | "on" | "On" | "ON"
        | "off" | "Off" | "OFF" => return true,
        _ => {}
    }
    if looks_numeric(s) {
        return true;
    }
    false
}

/// Heuristic: does `s` look like a YAML 1.2 core-schema number? We treat
/// anything that successfully parses as `i64`, `u64`, `f64`, or matches a
/// hex / octal / binary integer prefix pattern as numeric. False
/// positives are safe (we just quote unnecessarily); false negatives
/// would break the round-trip contract.
fn looks_numeric(s: &str) -> bool {
    if s.parse::<i64>().is_ok() || s.parse::<u64>().is_ok() {
        return true;
    }
    if s.parse::<f64>().is_ok() {
        return true;
    }
    if let Some(rest) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        if !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_hexdigit()) {
            return true;
        }
    }
    if let Some(rest) = s.strip_prefix("0o").or_else(|| s.strip_prefix("0O")) {
        if !rest.is_empty() && rest.bytes().all(|b| matches!(b, b'0'..=b'7')) {
            return true;
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn scan_error_to_workflow_error(e: ScanError) -> WorkflowYamlError {
    let m = e.marker();
    // yaml-rust2's `Marker::line()` is 1-based; `Marker::col()` is 0-based.
    // Surface both as 1-based here so callers don't have to know that quirk.
    WorkflowYamlError::new(m.line(), m.col() + 1, RULE_YAML_SYNTAX_ERROR)
}

fn parse_workflow(root: &Yaml) -> Result<Workflow, WorkflowYamlError> {
    let root_hash = root
        .as_hash()
        .ok_or_else(|| WorkflowYamlError::with_field(0, 0, RULE_WRONG_TYPE, FIELD_ROOT))?;

    // schema_version
    let schema_version_node = lookup(root_hash, "schema_version").ok_or_else(|| {
        WorkflowYamlError::with_field(0, 0, RULE_MISSING_FIELD, FIELD_SCHEMA_VERSION)
    })?;
    let raw_schema_version = match schema_version_node {
        Yaml::Integer(n) => *n,
        _ => {
            return Err(WorkflowYamlError::with_field(
                0,
                0,
                RULE_WRONG_TYPE,
                FIELD_SCHEMA_VERSION,
            ));
        }
    };
    if raw_schema_version != 1 {
        return Err(WorkflowYamlError::with_field(
            0,
            0,
            RULE_SCHEMA_VERSION_UNSUPPORTED,
            FIELD_SCHEMA_VERSION,
        ));
    }

    // input_dataset
    let input_dataset_node = lookup(root_hash, "input_dataset").ok_or_else(|| {
        WorkflowYamlError::with_field(0, 0, RULE_MISSING_FIELD, FIELD_INPUT_DATASET)
    })?;
    let input_dataset = parse_input_dataset(input_dataset_node)?;

    // steps
    let steps_node = lookup(root_hash, "steps")
        .ok_or_else(|| WorkflowYamlError::with_field(0, 0, RULE_MISSING_FIELD, FIELD_STEPS))?;
    let steps_array = steps_node
        .as_vec()
        .ok_or_else(|| WorkflowYamlError::with_field(0, 0, RULE_WRONG_TYPE, FIELD_STEPS))?;
    let mut steps = Vec::with_capacity(steps_array.len());
    let mut seen_ids: HashSet<String> = HashSet::with_capacity(steps_array.len());
    for step_node in steps_array {
        let step = parse_step(step_node)?;
        if !seen_ids.insert(step.id.clone()) {
            return Err(WorkflowYamlError::with_field(
                0,
                0,
                RULE_DUPLICATE_STEP_ID,
                FIELD_STEP_ID,
            ));
        }
        steps.push(step);
    }

    Ok(Workflow {
        schema_version: 1,
        input_dataset,
        steps,
    })
}

fn parse_input_dataset(node: &Yaml) -> Result<InputDataset, WorkflowYamlError> {
    let h = node
        .as_hash()
        .ok_or_else(|| WorkflowYamlError::with_field(0, 0, RULE_WRONG_TYPE, FIELD_INPUT_DATASET))?;
    let path = require_string(h, "path", FIELD_INPUT_DATASET_PATH)?;
    let sha256 = require_string(h, "sha256", FIELD_INPUT_DATASET_SHA256)?;
    if !is_valid_sha256(&sha256) {
        return Err(WorkflowYamlError::with_field(
            0,
            0,
            RULE_INVALID_SHA256,
            FIELD_INPUT_DATASET_SHA256,
        ));
    }
    Ok(InputDataset { path, sha256 })
}

fn parse_step(node: &Yaml) -> Result<WorkflowStep, WorkflowYamlError> {
    let h = node
        .as_hash()
        .ok_or_else(|| WorkflowYamlError::with_field(0, 0, RULE_WRONG_TYPE, FIELD_STEP))?;

    let id = require_string(h, "id", FIELD_STEP_ID)?;
    let algorithm = require_string(h, "algorithm", FIELD_STEP_ALGORITHM)?;

    let params_node = lookup(h, "params")
        .ok_or_else(|| WorkflowYamlError::with_field(0, 0, RULE_MISSING_FIELD, FIELD_STEP_PARAMS))?;
    let params = yaml_to_json(params_node)
        .ok_or_else(|| WorkflowYamlError::with_field(0, 0, RULE_WRONG_TYPE, FIELD_STEP_PARAMS))?;

    let inputs_node = lookup(h, "inputs")
        .ok_or_else(|| WorkflowYamlError::with_field(0, 0, RULE_MISSING_FIELD, FIELD_STEP_INPUTS))?;
    let inputs_arr = inputs_node
        .as_vec()
        .ok_or_else(|| WorkflowYamlError::with_field(0, 0, RULE_WRONG_TYPE, FIELD_STEP_INPUTS))?;
    let mut inputs = Vec::with_capacity(inputs_arr.len());
    for n in inputs_arr {
        inputs.push(parse_artifact_ref(n)?);
    }

    let outputs_node = lookup(h, "outputs").ok_or_else(|| {
        WorkflowYamlError::with_field(0, 0, RULE_MISSING_FIELD, FIELD_STEP_OUTPUTS)
    })?;
    let outputs_arr = outputs_node
        .as_vec()
        .ok_or_else(|| WorkflowYamlError::with_field(0, 0, RULE_WRONG_TYPE, FIELD_STEP_OUTPUTS))?;
    let mut outputs = Vec::with_capacity(outputs_arr.len());
    for n in outputs_arr {
        outputs.push(parse_artifact_ref(n)?);
    }

    // reference_software is optional. Treat both "absent" and "explicit null"
    // as None (idiomatic YAML).
    let reference_software = match lookup(h, "reference_software") {
        None | Some(Yaml::Null) => None,
        Some(n) => Some(parse_reference_software(n)?),
    };
    let llm = match lookup(h, "llm") {
        None | Some(Yaml::Null) => None,
        Some(n) => Some(parse_llm(n)?),
    };

    let started_at_utc = require_string(h, "started_at_utc", FIELD_STEP_STARTED_AT_UTC)?;
    let ended_at_utc = require_string(h, "ended_at_utc", FIELD_STEP_ENDED_AT_UTC)?;

    Ok(WorkflowStep {
        id,
        algorithm,
        params,
        inputs,
        outputs,
        reference_software,
        llm,
        started_at_utc,
        ended_at_utc,
    })
}

fn parse_artifact_ref(node: &Yaml) -> Result<ArtifactRef, WorkflowYamlError> {
    let h = node
        .as_hash()
        .ok_or_else(|| WorkflowYamlError::with_field(0, 0, RULE_WRONG_TYPE, FIELD_ARTIFACT))?;
    let path = require_string(h, "path", FIELD_ARTIFACT_PATH)?;
    let sha256 = require_string(h, "sha256", FIELD_ARTIFACT_SHA256)?;
    if !is_valid_sha256(&sha256) {
        return Err(WorkflowYamlError::with_field(
            0,
            0,
            RULE_INVALID_SHA256,
            FIELD_ARTIFACT_SHA256,
        ));
    }
    Ok(ArtifactRef { path, sha256 })
}

fn parse_reference_software(node: &Yaml) -> Result<ReferenceSoftwareRef, WorkflowYamlError> {
    let h = node.as_hash().ok_or_else(|| {
        WorkflowYamlError::with_field(0, 0, RULE_WRONG_TYPE, FIELD_STEP_REFERENCE_SOFTWARE)
    })?;
    let name = require_string(h, "name", FIELD_REFERENCE_SOFTWARE_NAME)?;
    let version = require_string(h, "version", FIELD_REFERENCE_SOFTWARE_VERSION)?;
    Ok(ReferenceSoftwareRef { name, version })
}

fn parse_llm(node: &Yaml) -> Result<LlmRef, WorkflowYamlError> {
    let h = node
        .as_hash()
        .ok_or_else(|| WorkflowYamlError::with_field(0, 0, RULE_WRONG_TYPE, FIELD_STEP_LLM))?;
    let provider = require_string(h, "provider", FIELD_LLM_PROVIDER)?;
    let model = require_string(h, "model", FIELD_LLM_MODEL)?;
    Ok(LlmRef { provider, model })
}

fn lookup<'a>(hash: &'a Hash, key: &str) -> Option<&'a Yaml> {
    hash.get(&Yaml::String(key.to_owned()))
}

fn require_string(
    hash: &Hash,
    key: &str,
    field: &'static str,
) -> Result<String, WorkflowYamlError> {
    let node = lookup(hash, key)
        .ok_or_else(|| WorkflowYamlError::with_field(0, 0, RULE_MISSING_FIELD, field))?;
    match node.as_str() {
        Some(s) => Ok(s.to_owned()),
        None => Err(WorkflowYamlError::with_field(0, 0, RULE_WRONG_TYPE, field)),
    }
}

/// 64-character lowercase hex check.
fn is_valid_sha256(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Project a YAML node onto a `serde_json::Value`. Returns `None` if the
/// node contains an alias or a non-string-keyed mapping (these are
/// structurally unrepresentable as JSON and we surface them as
/// `RULE_WRONG_TYPE` at the calling site).
fn yaml_to_json(y: &Yaml) -> Option<serde_json::Value> {
    use serde_json::Value;
    match y {
        Yaml::Null => Some(Value::Null),
        Yaml::Boolean(b) => Some(Value::Bool(*b)),
        Yaml::Integer(n) => Some(Value::Number((*n).into())),
        Yaml::Real(s) => match s.parse::<f64>() {
            Ok(f) => match serde_json::Number::from_f64(f) {
                Some(n) => Some(Value::Number(n)),
                // Non-finite (NaN / ±inf) → preserve the original string token.
                None => Some(Value::String(s.clone())),
            },
            Err(_) => Some(Value::String(s.clone())),
        },
        Yaml::String(s) => Some(Value::String(s.clone())),
        Yaml::Array(arr) => {
            let mut out = Vec::with_capacity(arr.len());
            for item in arr {
                out.push(yaml_to_json(item)?);
            }
            Some(Value::Array(out))
        }
        Yaml::Hash(h) => {
            let mut out = serde_json::Map::with_capacity(h.len());
            for (k, v) in h {
                let key = k.as_str()?.to_owned();
                out.insert(key, yaml_to_json(v)?);
            }
            Some(Value::Object(out))
        }
        // Aliases and BadValue are not representable in our params JSON.
        Yaml::Alias(_) | Yaml::BadValue => None,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_SHA: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    const VALID_SHA_2: &str = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";

    fn minimal_yaml() -> String {
        format!(
            r#"schema_version: 1
input_dataset:
  path: "data.csv"
  sha256: "{sha}"
steps:
  - id: step-1
    algorithm: tableone
    params:
      by: "treatment"
      vars: ["age", "sex"]
    inputs:
      - path: "data.csv"
        sha256: "{sha}"
    outputs:
      - path: "artifacts/step-1/result.json"
        sha256: "{sha}"
    started_at_utc: "2024-01-01T00:00:00Z"
    ended_at_utc:   "2024-01-01T00:00:01Z"
"#,
            sha = VALID_SHA
        )
    }

    #[test]
    fn parse_happy_path_minimal() {
        let bytes = minimal_yaml().into_bytes();
        let (wf, _doc) = parse(&bytes).expect("minimal valid YAML must parse");
        assert_eq!(wf.schema_version, 1);
        assert_eq!(wf.input_dataset.path, "data.csv");
        assert_eq!(wf.input_dataset.sha256, VALID_SHA);
        assert_eq!(wf.steps.len(), 1);

        let s = &wf.steps[0];
        assert_eq!(s.id, "step-1");
        assert_eq!(s.algorithm, "tableone");
        assert_eq!(s.inputs.len(), 1);
        assert_eq!(s.outputs.len(), 1);
        assert_eq!(s.inputs[0].path, "data.csv");
        assert_eq!(s.outputs[0].path, "artifacts/step-1/result.json");
        assert!(s.reference_software.is_none());
        assert!(s.llm.is_none());
        assert_eq!(s.started_at_utc, "2024-01-01T00:00:00Z");
        assert_eq!(s.ended_at_utc, "2024-01-01T00:00:01Z");

        // params is a JSON object with by + vars.
        let params = &s.params;
        assert_eq!(params["by"], serde_json::json!("treatment"));
        assert_eq!(params["vars"], serde_json::json!(["age", "sex"]));
    }

    #[test]
    fn parse_multi_step_mixed_optionals() {
        let yaml = format!(
            r#"schema_version: 1
input_dataset:
  path: "data.csv"
  sha256: "{sha}"
steps:
  - id: step-1
    algorithm: tableone
    params: {{}}
    inputs: []
    outputs:
      - path: "artifacts/step-1/result.json"
        sha256: "{sha2}"
    reference_software:
      name: "R"
      version: "4.4.1"
    started_at_utc: "2024-01-01T00:00:00Z"
    ended_at_utc:   "2024-01-01T00:00:01Z"
  - id: step-2
    algorithm: cox
    params:
      alpha: 0.05
    inputs:
      - path: "data.csv"
        sha256: "{sha}"
    outputs:
      - path: "artifacts/step-2/result.json"
        sha256: "{sha2}"
    llm:
      provider: "deepseek"
      model: "deepseek-chat"
    started_at_utc: "2024-01-01T00:00:02Z"
    ended_at_utc:   "2024-01-01T00:00:03Z"
  - id: step-3
    algorithm: logistic
    params:
      penalty: "l2"
    inputs:
      - path: "artifacts/step-1/result.json"
        sha256: "{sha2}"
    outputs:
      - path: "artifacts/step-3/result.json"
        sha256: "{sha}"
    reference_software:
      name: "Python"
      version: "3.11.9"
    llm:
      provider: "openai"
      model: "gpt-4o"
    started_at_utc: "2024-01-01T00:00:04Z"
    ended_at_utc:   "2024-01-01T00:00:05Z"
"#,
            sha = VALID_SHA,
            sha2 = VALID_SHA_2
        );
        let (wf, _doc) = parse(yaml.as_bytes()).expect("multi-step YAML must parse");
        assert_eq!(wf.steps.len(), 3);
        assert_eq!(wf.steps[0].id, "step-1");
        assert_eq!(wf.steps[1].id, "step-2");
        assert_eq!(wf.steps[2].id, "step-3");

        // step-1: reference_software present, llm absent
        assert_eq!(
            wf.steps[0].reference_software.as_ref().unwrap().name,
            "R"
        );
        assert!(wf.steps[0].llm.is_none());

        // step-2: llm present, reference_software absent
        assert!(wf.steps[1].reference_software.is_none());
        assert_eq!(wf.steps[1].llm.as_ref().unwrap().provider, "deepseek");
        assert_eq!(wf.steps[1].params["alpha"], serde_json::json!(0.05));

        // step-3: both present
        assert_eq!(
            wf.steps[2].reference_software.as_ref().unwrap().name,
            "Python"
        );
        assert_eq!(wf.steps[2].llm.as_ref().unwrap().model, "gpt-4o");
    }

    #[test]
    fn parse_rejects_size_cap() {
        // 10 MiB + 1 byte. We don't even need to make it valid YAML — the
        // size gate fires before allocation.
        let bytes = vec![b'a'; SIZE_CAP_BYTES + 1];
        let err = parse(&bytes).expect_err("oversize input must be rejected");
        assert_eq!(err.rule_violated, RULE_SIZE_CAP_EXCEEDED);
        assert_eq!(err.line, 0);
        assert_eq!(err.column, 0);
        assert!(err.field.is_none());
    }

    #[test]
    fn parse_size_cap_boundary_accepts_exactly_10_mib() {
        // Exactly 10 MiB of valid YAML padded with comment lines should pass
        // the size gate (it may still fail on YAML or schema validation, but
        // not with `RULE_SIZE_CAP_EXCEEDED`). We pad valid YAML with a long
        // trailing comment until the byte count reaches exactly the cap.
        let mut bytes = minimal_yaml().into_bytes();
        // Build the comment payload so that the final length equals the cap.
        let header = b"\n# pad: ";
        let tail_byte = b'x';
        if bytes.len() < SIZE_CAP_BYTES {
            let need = SIZE_CAP_BYTES - bytes.len();
            if need >= header.len() {
                bytes.extend_from_slice(header);
                bytes.extend(std::iter::repeat(tail_byte).take(need - header.len()));
            } else {
                bytes.extend(std::iter::repeat(b' ').take(need));
            }
        }
        assert_eq!(bytes.len(), SIZE_CAP_BYTES);
        // Either parses or fails for a non-size-cap reason.
        match parse(&bytes) {
            Ok(_) => {}
            Err(e) => assert_ne!(e.rule_violated, RULE_SIZE_CAP_EXCEEDED),
        }
    }

    #[test]
    fn parse_rejects_non_utf8() {
        // 0xFF is not a legal UTF-8 byte.
        let bytes = vec![0xFFu8, 0xFE, 0xFD];
        let err = parse(&bytes).expect_err("non-UTF-8 input must be rejected");
        assert_eq!(err.rule_violated, RULE_NON_UTF8);
        assert!(err.field.is_none());
    }

    #[test]
    fn parse_rejects_yaml_syntax_error() {
        let bytes = b"[invalid\n";
        let err = parse(bytes).expect_err("malformed YAML must be rejected");
        assert_eq!(err.rule_violated, RULE_YAML_SYNTAX_ERROR);
        // The scanner should locate the error somewhere in the source; both
        // line and column are 1-based and must be at least 1.
        assert!(err.line >= 1, "expected line >= 1, got {}", err.line);
        assert!(err.column >= 1, "expected column >= 1, got {}", err.column);
    }

    #[test]
    fn parse_rejects_missing_schema_version() {
        let yaml = format!(
            r#"input_dataset:
  path: "data.csv"
  sha256: "{sha}"
steps: []
"#,
            sha = VALID_SHA
        );
        let err = parse(yaml.as_bytes()).expect_err("missing schema_version must be rejected");
        assert_eq!(err.rule_violated, RULE_MISSING_FIELD);
        assert_eq!(err.field, Some(FIELD_SCHEMA_VERSION));
    }

    #[test]
    fn parse_rejects_wrong_type_schema_version() {
        // Quoted "1" is a YAML string, not an integer.
        let yaml = format!(
            r#"schema_version: "1"
input_dataset:
  path: "data.csv"
  sha256: "{sha}"
steps: []
"#,
            sha = VALID_SHA
        );
        let err = parse(yaml.as_bytes())
            .expect_err("string-typed schema_version must be rejected");
        assert_eq!(err.rule_violated, RULE_WRONG_TYPE);
        assert_eq!(err.field, Some(FIELD_SCHEMA_VERSION));
    }

    #[test]
    fn parse_rejects_unsupported_schema_version() {
        let yaml = format!(
            r#"schema_version: 2
input_dataset:
  path: "data.csv"
  sha256: "{sha}"
steps: []
"#,
            sha = VALID_SHA
        );
        let err = parse(yaml.as_bytes())
            .expect_err("schema_version != 1 must be rejected");
        assert_eq!(err.rule_violated, RULE_SCHEMA_VERSION_UNSUPPORTED);
        assert_eq!(err.field, Some(FIELD_SCHEMA_VERSION));
    }

    #[test]
    fn parse_rejects_missing_input_dataset() {
        let yaml = "schema_version: 1\nsteps: []\n";
        let err = parse(yaml.as_bytes())
            .expect_err("missing input_dataset must be rejected");
        assert_eq!(err.rule_violated, RULE_MISSING_FIELD);
        assert_eq!(err.field, Some(FIELD_INPUT_DATASET));
    }

    #[test]
    fn parse_rejects_missing_steps() {
        let yaml = format!(
            r#"schema_version: 1
input_dataset:
  path: "data.csv"
  sha256: "{sha}"
"#,
            sha = VALID_SHA
        );
        let err = parse(yaml.as_bytes()).expect_err("missing steps must be rejected");
        assert_eq!(err.rule_violated, RULE_MISSING_FIELD);
        assert_eq!(err.field, Some(FIELD_STEPS));
    }

    #[test]
    fn parse_rejects_invalid_dataset_sha256() {
        // Uppercase hex is not allowed (we require lowercase only).
        let yaml = format!(
            r#"schema_version: 1
input_dataset:
  path: "data.csv"
  sha256: "{}"
steps: []
"#,
            "0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF"
        );
        let err = parse(yaml.as_bytes())
            .expect_err("uppercase-hex sha256 must be rejected");
        assert_eq!(err.rule_violated, RULE_INVALID_SHA256);
        assert_eq!(err.field, Some(FIELD_INPUT_DATASET_SHA256));
    }

    #[test]
    fn parse_rejects_invalid_artifact_sha256() {
        let yaml = format!(
            r#"schema_version: 1
input_dataset:
  path: "data.csv"
  sha256: "{sha}"
steps:
  - id: step-1
    algorithm: tableone
    params: {{}}
    inputs:
      - path: "data.csv"
        sha256: "not-a-real-hash"
    outputs:
      - path: "artifacts/step-1/result.json"
        sha256: "{sha}"
    started_at_utc: "2024-01-01T00:00:00Z"
    ended_at_utc:   "2024-01-01T00:00:01Z"
"#,
            sha = VALID_SHA
        );
        let err = parse(yaml.as_bytes())
            .expect_err("non-hex artifact sha256 must be rejected");
        assert_eq!(err.rule_violated, RULE_INVALID_SHA256);
        assert_eq!(err.field, Some(FIELD_ARTIFACT_SHA256));
    }

    #[test]
    fn parse_rejects_duplicate_step_ids() {
        let yaml = format!(
            r#"schema_version: 1
input_dataset:
  path: "data.csv"
  sha256: "{sha}"
steps:
  - id: step-1
    algorithm: tableone
    params: {{}}
    inputs: []
    outputs:
      - path: "a.json"
        sha256: "{sha}"
    started_at_utc: "2024-01-01T00:00:00Z"
    ended_at_utc:   "2024-01-01T00:00:01Z"
  - id: step-1
    algorithm: cox
    params: {{}}
    inputs: []
    outputs:
      - path: "b.json"
        sha256: "{sha}"
    started_at_utc: "2024-01-01T00:00:02Z"
    ended_at_utc:   "2024-01-01T00:00:03Z"
"#,
            sha = VALID_SHA
        );
        let err = parse(yaml.as_bytes())
            .expect_err("duplicate step ids must be rejected");
        assert_eq!(err.rule_violated, RULE_DUPLICATE_STEP_ID);
        assert_eq!(err.field, Some(FIELD_STEP_ID));
    }

    #[test]
    fn parse_doc_preserves_original_bytes() {
        let yaml = minimal_yaml();
        let bytes = yaml.into_bytes();
        let (_, doc) = parse(&bytes).expect("minimal yaml must parse");
        assert_eq!(
            doc.original_bytes, bytes,
            "WorkflowYamlDoc must hold a verbatim copy of the input bytes \
             so task 5.3's `pretty_print(_, Some(doc))` path can satisfy \
             Requirement 11.4 by returning that buffer as-is"
        );
    }

    #[test]
    fn error_display_includes_rule_and_field() {
        let e = WorkflowYamlError::with_field(
            12,
            34,
            RULE_MISSING_FIELD,
            FIELD_SCHEMA_VERSION,
        );
        let s = e.to_string();
        assert!(s.contains("missing_field"), "got: {s}");
        assert!(s.contains("schema_version"), "got: {s}");
        assert!(s.contains("12"), "got: {s}");
        assert!(s.contains("34"), "got: {s}");
    }

    #[test]
    fn error_implements_std_error() {
        // Smoke test: WorkflowYamlError must be usable with `?` and the
        // standard error-handling traits.
        fn _coerce(e: WorkflowYamlError) -> Box<dyn std::error::Error> {
            Box::new(e)
        }
    }

    #[test]
    fn is_valid_sha256_accepts_64_lowercase_hex() {
        assert!(is_valid_sha256(VALID_SHA));
        assert!(is_valid_sha256(VALID_SHA_2));
    }

    #[test]
    fn is_valid_sha256_rejects_uppercase() {
        assert!(!is_valid_sha256(
            "0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF"
        ));
    }

    #[test]
    fn is_valid_sha256_rejects_wrong_length() {
        assert!(!is_valid_sha256("0123"));
        assert!(!is_valid_sha256(&"a".repeat(63)));
        assert!(!is_valid_sha256(&"a".repeat(65)));
    }

    #[test]
    fn is_valid_sha256_rejects_non_hex_chars() {
        let mut s = String::from("0123456789abcdef");
        s = s.repeat(4);
        // Replace one char with a non-hex char.
        s.replace_range(0..1, "g");
        assert!(!is_valid_sha256(&s));
    }

    // -----------------------------------------------------------------
    // pretty_print — task 5.3
    // -----------------------------------------------------------------

    /// Build a sample `Workflow` exercising both optional members
    /// (reference_software, llm), nested `params` maps & arrays, empty
    /// `inputs`, and several scalar shapes that need quoting.
    fn sample_workflow() -> Workflow {
        Workflow {
            schema_version: 1,
            input_dataset: InputDataset {
                path: "data.csv".to_owned(),
                sha256: VALID_SHA.to_owned(),
            },
            steps: vec![
                WorkflowStep {
                    id: "step-1".to_owned(),
                    algorithm: "tableone".to_owned(),
                    params: serde_json::json!({
                        "by": "treatment",
                        "vars": ["age", "sex"],
                        "alpha": 0.05,
                        "include": true,
                    }),
                    inputs: vec![],
                    outputs: vec![ArtifactRef {
                        path: "artifacts/step-1/result.json".to_owned(),
                        sha256: VALID_SHA_2.to_owned(),
                    }],
                    reference_software: Some(ReferenceSoftwareRef {
                        name: "R".to_owned(),
                        version: "4.4.1".to_owned(),
                    }),
                    llm: None,
                    started_at_utc: "2024-01-01T00:00:00Z".to_owned(),
                    ended_at_utc: "2024-01-01T00:00:01Z".to_owned(),
                },
                WorkflowStep {
                    id: "step-2".to_owned(),
                    algorithm: "cox".to_owned(),
                    params: serde_json::json!({}),
                    inputs: vec![ArtifactRef {
                        path: "data.csv".to_owned(),
                        sha256: VALID_SHA.to_owned(),
                    }],
                    outputs: vec![ArtifactRef {
                        path: "artifacts/step-2/result.json".to_owned(),
                        sha256: VALID_SHA_2.to_owned(),
                    }],
                    reference_software: None,
                    llm: Some(LlmRef {
                        provider: "deepseek".to_owned(),
                        model: "deepseek-chat".to_owned(),
                    }),
                    started_at_utc: "2024-01-01T00:00:02Z".to_owned(),
                    ended_at_utc: "2024-01-01T00:00:03Z".to_owned(),
                },
            ],
        }
    }

    #[test]
    fn pretty_print_doc_mode_is_byte_identical() {
        // Requirement 11.4: `parse → pretty_print(_, Some(doc))` must
        // return bytes byte-identical to the original input.
        let bytes = minimal_yaml().into_bytes();
        let (wf, doc) = parse(&bytes).expect("minimal yaml must parse");
        let out = pretty_print(&wf, Some(&doc));
        assert_eq!(out, bytes);
    }

    #[test]
    fn pretty_print_doc_mode_preserves_comments_and_blank_lines() {
        // Doc-mode round-trip must preserve trivia (Requirement 11.4):
        // comments, blank lines, key order, indentation.
        let yaml = format!(
            "# leading comment\nschema_version: 1\n\n# block comment before input_dataset\ninput_dataset:\n  path: \"data.csv\"\n  sha256: \"{sha}\"  # trailing comment\n\nsteps:\n  - id: step-1\n    algorithm: tableone\n    params: {{}}\n    inputs: []\n    outputs:\n      - path: \"out.json\"\n        sha256: \"{sha}\"\n    started_at_utc: \"2024-01-01T00:00:00Z\"\n    ended_at_utc:   \"2024-01-01T00:00:01Z\"\n",
            sha = VALID_SHA
        );
        let bytes = yaml.into_bytes();
        let (wf, doc) = parse(&bytes).expect("yaml with comments must parse");
        let out = pretty_print(&wf, Some(&doc));
        assert_eq!(
            out, bytes,
            "doc-mode pretty_print must reproduce comments + blank lines + spacing verbatim"
        );
    }

    #[test]
    fn pretty_print_canonical_round_trips_through_parse() {
        // Requirement 11.3: `parse(pretty_print(W, None)).0` must be
        // structurally equal to `W`.
        let wf = sample_workflow();
        let canonical = pretty_print(&wf, None);
        let (parsed, _) = parse(&canonical).expect("canonical YAML must parse");
        assert_eq!(parsed, wf);
    }

    #[test]
    fn pretty_print_canonical_is_a_fixpoint() {
        // Requirement 11.7: applying canonical pretty-print twice (with a
        // round-trip parse in the middle, and again at the end) yields a
        // byte-identical output. Concretely:
        //
        //   pretty_print(parse(pretty_print(W, None)).0, None)
        //     == pretty_print(W, None)
        let wf = sample_workflow();
        let first = pretty_print(&wf, None);
        let (parsed, _) = parse(&first).expect("first canonical pass must parse");
        let second = pretty_print(&parsed, None);
        assert_eq!(
            second, first,
            "canonical pretty-print must be a fixpoint at the byte level"
        );
        // And one more round to be paranoid.
        let (parsed2, _) = parse(&second).unwrap();
        let third = pretty_print(&parsed2, None);
        assert_eq!(third, second);
    }

    #[test]
    fn pretty_print_canonical_is_lf_only_with_trailing_lf() {
        let wf = sample_workflow();
        let bytes = pretty_print(&wf, None);
        // No CR (\r) anywhere — canonical form is LF-only.
        assert!(
            !bytes.contains(&b'\r'),
            "canonical output must contain no CR bytes"
        );
        // Ends with exactly one LF and not a doubled blank line.
        assert_eq!(
            bytes.last(),
            Some(&b'\n'),
            "canonical output must end with a trailing LF"
        );
        assert_ne!(
            &bytes[bytes.len().saturating_sub(2)..],
            b"\n\n",
            "canonical output must end with a single LF, not a blank line"
        );
        // No trailing whitespace on any emitted line.
        for line in bytes.split(|b| *b == b'\n') {
            if line.is_empty() {
                continue;
            }
            let last = *line.last().unwrap();
            assert!(
                last != b' ' && last != b'\t',
                "canonical line must not have trailing whitespace: {:?}",
                std::str::from_utf8(line)
            );
        }
    }

    #[test]
    fn pretty_print_canonical_emits_empty_inputs_inline() {
        // Requirement: empty inputs/outputs `[]` should appear inline as
        // `inputs: []` (and `outputs: []`), not as a block sequence with
        // zero items.
        let wf = sample_workflow();
        let bytes = pretty_print(&wf, None);
        let text = std::str::from_utf8(&bytes).unwrap();
        assert!(
            text.contains("    inputs: []\n"),
            "expected inline empty inputs, got:\n{text}"
        );
    }

    #[test]
    fn pretty_print_canonical_quotes_ambiguous_string_params() {
        // params that contain a string like `"true"` or `"1"` must quote
        // them so the round-trip preserves the string type.
        let wf = Workflow {
            schema_version: 1,
            input_dataset: InputDataset {
                path: "data.csv".to_owned(),
                sha256: VALID_SHA.to_owned(),
            },
            steps: vec![WorkflowStep {
                id: "step-1".to_owned(),
                algorithm: "tableone".to_owned(),
                params: serde_json::json!({
                    "literal_true": "true",
                    "literal_one": "1",
                    "literal_null": "null",
                    "literal_yes": "yes",
                }),
                inputs: vec![],
                outputs: vec![ArtifactRef {
                    path: "out.json".to_owned(),
                    sha256: VALID_SHA.to_owned(),
                }],
                reference_software: None,
                llm: None,
                started_at_utc: "2024-01-01T00:00:00Z".to_owned(),
                ended_at_utc: "2024-01-01T00:00:01Z".to_owned(),
            }],
        };
        let bytes = pretty_print(&wf, None);
        let (back, _) = parse(&bytes).expect("ambiguous-string params must parse back");
        assert_eq!(back.steps[0].params, wf.steps[0].params);
    }

    #[test]
    fn pretty_print_canonical_emits_steps_in_input_order() {
        // Steps are an ordered collection; canonical form must keep their
        // original order rather than alphabetizing by id.
        let mut wf = sample_workflow();
        wf.steps.reverse();
        let bytes = pretty_print(&wf, None);
        let text = std::str::from_utf8(&bytes).unwrap();
        let pos_step1 = text
            .find("- id: step-1")
            .expect("step-1 must appear in canonical output");
        let pos_step2 = text
            .find("- id: step-2")
            .expect("step-2 must appear in canonical output");
        assert!(
            pos_step2 < pos_step1,
            "steps must keep input order; got step-1 at {pos_step1}, step-2 at {pos_step2}"
        );
    }

    #[test]
    fn needs_quoting_known_cases() {
        assert!(needs_quoting(""));
        assert!(needs_quoting(" leading"));
        assert!(needs_quoting("trailing "));
        assert!(needs_quoting("has: colon"));
        assert!(needs_quoting("has #hash"));
        assert!(needs_quoting("true"));
        assert!(needs_quoting("False"));
        assert!(needs_quoting("null"));
        assert!(needs_quoting("1"));
        assert!(needs_quoting("0123"));
        assert!(needs_quoting("3.14"));
        assert!(needs_quoting("-1"));
        assert!(needs_quoting("[bracket"));
        assert!(needs_quoting("{brace"));

        // Plain-safe strings.
        assert!(!needs_quoting("data.csv"));
        assert!(!needs_quoting("step-1")); // starts with letter, contains '-' but only after first char
        assert!(!needs_quoting("tableone"));
        assert!(!needs_quoting("artifacts/step-1/result.json"));
        assert!(!needs_quoting("R"));
        assert!(!needs_quoting("4.4.1")); // not a valid f64 (two dots)
    }
}
