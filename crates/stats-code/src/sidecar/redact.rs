//! Secret-and-path redaction policy as seen by the Sidecar Code Generator.
//!
//! Feature: parity-and-multilang-sidecar — task 2.4.
//!
//! This module is a thin re-export of the canonical redaction policy that
//! lives at the crate-level path [`crate::redact`]. The re-export exists so
//! sidecar callers can keep writing `sidecar::redact::redact_pure(…)` /
//! `sidecar::redact::RedactionPolicy::…` without having to know that the
//! implementation is shared with the snapshot exporter
//! (Requirements 2.6, 9.1, 9.3, 9.4, 9.5).

pub use crate::redact::*;
