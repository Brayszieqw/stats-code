//! Parity validation subcommand support.
//!
//! Feature: parity-and-multilang-sidecar (tasks 9.1 / 9.2 / 9.3).
//!
//! Two submodules collaborate here:
//!
//! - [`tolerance`] (task 9.3) — `tolerance_config.yaml` loader plus the
//!   spec-mandated default Parity Threshold constants. Pure and
//!   `Path`-driven so it can be unit-tested without the launcher,
//!   agent-server, or any external runtime (Requirement 12.1, 12.2,
//!   12.3, 12.6).
//! - [`run_local`] (task 9.2) — the wave-1 driver invoked from
//!   [`crate::handlers::run`] when `Command::Parity` fires. Owns the
//!   public exit-code surface (`0 / 2 / 3 / 4 / 5`) defined in
//!   design.md §6 "Internal `parity` subcommand → Exit codes" and
//!   captures it in the structured [`run_local::ParityOutcome`] enum
//!   (Requirements 5.1, 5.4, 5.5, 5.6, 5.7, 6.6, 12.1, 12.6).
//!
//! [`run_local::run_local`] is re-exported under the bare name so the
//! handler can call it as `parity::run_local(args)` per design.md.
//!
//! _Requirements: 5.1, 5.4–5.7, 6.6, 12.1, 12.2, 12.3, 12.6_

pub mod run_local;
pub mod tolerance;

pub use run_local::run_local;
