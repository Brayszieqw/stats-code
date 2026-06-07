# Test suites

| Directory      | Purpose                                                                 |
|----------------|-------------------------------------------------------------------------|
| `unit/`        | Per-component unit tests (vitest).                                       |
| `integration/` | Cross-component tests (launcher lifecycle, server wiring).               |
| `property/`    | fast-check property tests (determinism, idempotence, round-trip, spawn). |
| `parity/`      | TS engine output vs Reference_Software within thresholds.               |

Property tests are also placed next to the code they validate; this tree holds
cross-cutting and phase-checkpoint suites.
