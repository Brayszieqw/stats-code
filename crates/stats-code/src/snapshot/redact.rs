//! Secret-and-path redaction policy as seen by the Audit Snapshot Exporter.
//!
//! Feature: parity-and-multilang-sidecar — task 2.4.
//!
//! This module is a thin re-export of the canonical redaction policy that
//! lives at the crate-level path [`crate::redact`]. The snapshot exporter
//! reuses the very same `redact_pure` / `RedactionPolicy` as the sidecar
//! generator so manifest values, narrative prose, llm provenance, and
//! workflow YAML strings all flow through the same byte-deterministic
//! rewriter (Requirements 9.1, 9.3, 9.4, 9.5).

pub use crate::redact::*;
