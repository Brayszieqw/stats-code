//! `llm_provenance.json` builder for the Audit Snapshot.
//!
//! Implements task 6.4 of `parity-and-multilang-sidecar`. See design.md
//! "Data Models — `llm_provenance.json`" and Requirements 7.5 / 9.1 / 9.4.
//!
//! `build_llm_provenance` is a **pure function**: it reads no clock, no host
//! environment, no random seeds. The slice of `LlmCall` records flows in via
//! the argument list, so two calls with byte-identical inputs produce
//! byte-identical output (the determinism contract of Requirement 7.1,
//! threaded through this builder by task 6.7).
//!
//! # Privacy contract (Requirements 9.1 and 9.4)
//!
//! The struct exposed by this module **never embeds an LLM API key**. The
//! API-key check is owned by the redaction layer
//! ([`crate::redact::redact_pure`]) and runs on every string field before
//! the call records reach this builder, before this builder's output is
//! written to disk by `snapshot::zip_writer::write_deterministic_zip`, and
//! before any other surface in the snapshot pipeline.
//!
//! The two SHA256 fields (`prompt_sha256`, `response_sha256`) are 32-byte
//! digests rendered as 64-character lowercase hexadecimal strings — never
//! the plaintext prompt or response. The hashes are produced by the LLM
//! call site at the time the request is issued and stored on the run; this
//! builder only forwards them.
//!
//! _Requirements: 7.5, 9.1, 9.4_

use serde::{Deserialize, Serialize};

/// Schema version of the `llm_provenance.json` payload. Bumped on any
/// breaking change to the field set; new readers must reject unknown values.
pub const SCHEMA_VERSION: u32 = 1;

/// A single LLM provider call recorded for the snapshot.
///
/// Field order matches `design.md` ("Data Models — `llm_provenance.json`")
/// and Requirement 7.5. Serialization uses the field order declared here,
/// which gives byte-stable JSON output for identical inputs.
///
/// # Field invariants
///
/// - `provider`, `model`: free-form identifiers (e.g. `"deepseek"`,
///   `"deepseek-chat"`). Already redaction-checked by the caller.
/// - `request_at_utc`: ISO-8601 UTC string (e.g. `"2024-01-01T12:34:56Z"`).
///   Stored verbatim; the builder does not validate the format.
/// - `prompt_sha256`, `response_sha256`: 64-character lowercase hexadecimal
///   strings (32 raw bytes per Requirement 7.5). Never plaintext.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LlmCall {
    /// LLM provider name (e.g. `"deepseek"`, `"openai"`).
    pub provider: String,
    /// Model identifier (e.g. `"deepseek-chat"`).
    pub model: String,
    /// ISO-8601 UTC timestamp of the request.
    pub request_at_utc: String,
    /// 64-character lowercase hexadecimal SHA256 of the prompt bytes.
    pub prompt_sha256: String,
    /// 64-character lowercase hexadecimal SHA256 of the response bytes.
    pub response_sha256: String,
}

/// Top-level shape of `llm_provenance.json` inside an Audit Snapshot.
///
/// Field order matches `design.md` ("Data Models — `llm_provenance.json`")
/// and Requirement 7.5. Serialization uses the field order declared here,
/// which gives byte-stable JSON output for identical inputs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LlmProvenance {
    /// Always `1` for this revision. See [`SCHEMA_VERSION`].
    pub schema_version: u32,
    /// One entry per LLM call made during the run. Empty `Vec` when the run
    /// made zero LLM calls (Requirement 7.5: "WHEN no LLM call was made
    /// during the run, THE `llm_provenance.json` SHALL contain an empty
    /// list").
    pub calls: Vec<LlmCall>,
}

/// Build the `llm_provenance.json` payload for a snapshot.
///
/// Pure function: wraps `calls` into [`LlmProvenance`] with
/// `schema_version = 1`. Order of `calls` is preserved as given by the
/// caller; the builder does not sort.
///
/// # Empty-case contract
///
/// When `calls` is empty, the returned [`LlmProvenance::calls`] is an empty
/// `Vec` (length `0`), **not** an absent field. This is the byte-stable
/// representation of "no LLM calls" required by Requirement 7.5 and
/// matches the empty-array shape expected by Property 22.
///
/// # Privacy contract
///
/// By contract this function **never embeds an LLM API key value**
/// (Requirements 9.1 and 9.4). The API-key check happens at the redaction
/// layer ([`crate::redact::redact_pure`]) before this struct's fields are
/// written, and the SHA256 fields are 32-byte digests (rendered as 64-hex)
/// of the prompt and response — never the plaintext. The builder simply
/// forwards already-prepared values; it does not, and can not, perform
/// redaction itself.
///
/// _Requirements: 7.5, 9.1, 9.4_
#[must_use]
pub fn build_llm_provenance(calls: &[LlmCall]) -> LlmProvenance {
    LlmProvenance {
        schema_version: SCHEMA_VERSION,
        calls: calls.to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_call(idx: usize) -> LlmCall {
        // Deterministic 64-hex strings derived from `idx`. Real prompts /
        // responses produce real SHA256s; here we only need 64-hex lowercase
        // shape stability for serialization tests.
        let hex_a = format!("{:064x}", (idx * 2 + 1) as u128);
        let hex_b = format!("{:064x}", (idx * 2 + 2) as u128);
        LlmCall {
            provider: format!("provider-{idx}"),
            model: format!("model-{idx}"),
            request_at_utc: format!("2024-01-01T00:00:{idx:02}Z"),
            prompt_sha256: hex_a,
            response_sha256: hex_b,
        }
    }

    #[test]
    fn empty_calls_yields_empty_vec() {
        let prov = build_llm_provenance(&[]);
        assert_eq!(prov.schema_version, 1);
        assert!(prov.calls.is_empty(), "calls must be an empty Vec");
        // Property 22 / Requirement 7.5: serializes as an empty JSON array.
        let json = serde_json::to_string(&prov).unwrap();
        assert!(
            json.contains("\"calls\":[]"),
            "empty calls must serialize as []; got {json}"
        );
    }

    #[test]
    fn multi_call_preserves_order() {
        // Build calls in explicit order [c0, c1] and pass them in that
        // order. The builder must not sort.
        let c0 = sample_call(0);
        let c1 = sample_call(1);
        let prov = build_llm_provenance(&[c0.clone(), c1.clone()]);

        assert_eq!(prov.calls.len(), 2);
        assert_eq!(prov.calls[0], c0);
        assert_eq!(prov.calls[1], c1);

        // Reverse order in: the builder still preserves caller order.
        let prov_rev = build_llm_provenance(&[c1.clone(), c0.clone()]);
        assert_eq!(prov_rev.calls[0], c1);
        assert_eq!(prov_rev.calls[1], c0);
    }

    #[test]
    fn serde_round_trip_empty() {
        let prov = build_llm_provenance(&[]);
        let json = serde_json::to_vec(&prov).expect("LlmProvenance serializes");
        let parsed: LlmProvenance =
            serde_json::from_slice(&json).expect("LlmProvenance round-trips");
        assert_eq!(parsed, prov);
    }

    #[test]
    fn serde_round_trip_two_calls() {
        let calls = [sample_call(0), sample_call(1)];
        let prov = build_llm_provenance(&calls);

        let json = serde_json::to_vec(&prov).expect("LlmProvenance serializes");
        let parsed: LlmProvenance =
            serde_json::from_slice(&json).expect("LlmProvenance round-trips");
        assert_eq!(parsed, prov);
        assert_eq!(parsed.calls.len(), 2);
        assert_eq!(parsed.calls[0].provider, "provider-0");
        assert_eq!(parsed.calls[1].provider, "provider-1");
    }

    #[test]
    fn json_field_order_matches_struct_declaration_top_level() {
        let prov = build_llm_provenance(&[sample_call(0)]);
        let json = serde_json::to_string(&prov).unwrap();

        let pos = |needle: &str| {
            json.find(needle)
                .unwrap_or_else(|| panic!("missing field {needle} in {json}"))
        };
        let order = [pos("\"schema_version\""), pos("\"calls\"")];
        let mut sorted = order;
        sorted.sort_unstable();
        assert_eq!(
            order, sorted,
            "top-level JSON fields must appear in struct declaration order; got {json}"
        );
    }

    #[test]
    fn json_field_order_matches_struct_declaration_call() {
        let prov = build_llm_provenance(&[sample_call(0)]);
        let json = serde_json::to_string(&prov).unwrap();

        let pos = |needle: &str| {
            json.find(needle)
                .unwrap_or_else(|| panic!("missing field {needle} in {json}"))
        };
        let order = [
            pos("\"provider\""),
            pos("\"model\""),
            pos("\"request_at_utc\""),
            pos("\"prompt_sha256\""),
            pos("\"response_sha256\""),
        ];
        let mut sorted = order;
        sorted.sort_unstable();
        assert_eq!(
            order, sorted,
            "LlmCall JSON fields must appear in struct declaration order; got {json}"
        );
    }

    #[test]
    fn deterministic_for_identical_inputs() {
        let calls = [sample_call(0), sample_call(1), sample_call(2)];
        let p1 = build_llm_provenance(&calls);
        let p2 = build_llm_provenance(&calls);
        assert_eq!(p1, p2);

        let j1 = serde_json::to_vec(&p1).unwrap();
        let j2 = serde_json::to_vec(&p2).unwrap();
        assert_eq!(j1, j2, "identical inputs must produce byte-identical JSON");
    }

    #[test]
    fn deterministic_empty_input() {
        let p1 = build_llm_provenance(&[]);
        let p2 = build_llm_provenance(&[]);
        assert_eq!(p1, p2);

        let j1 = serde_json::to_vec(&p1).unwrap();
        let j2 = serde_json::to_vec(&p2).unwrap();
        assert_eq!(j1, j2);
    }

    #[test]
    fn schema_version_is_one() {
        assert_eq!(SCHEMA_VERSION, 1);
        let prov = build_llm_provenance(&[]);
        assert_eq!(prov.schema_version, 1);
    }
}
