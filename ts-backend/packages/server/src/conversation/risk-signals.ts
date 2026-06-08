// server/conversation/risk-signals.ts — risk signal detection.
//
// Mirrors crates/agent-core/src/skill/risk.rs::detect_risk_signals verbatim:
//   - payload.p_value > 0.05            → PValueAboveAlpha
//   - payload.vif has any value > 10.0  → VifTooHigh
//   - payload.power < 0.8 (or achieved_power < 0.8) → LowPower
//   - payload.cox_ph_violated == true (or ph_test.violated == true)
//                                       → CoxPhAssumptionViolated

import type { RiskSignal } from './skill-runner-types.js';

function asNumber(value: unknown): number | undefined {
  return typeof value === 'number' && Number.isFinite(value) ? value : undefined;
}

export function detectRiskSignals(payload: Record<string, unknown>): RiskSignal[] {
  const signals: RiskSignal[] = [];

  const p = asNumber(payload.p_value);
  if (p !== undefined && p > 0.05) {
    signals.push('PValueAboveAlpha');
  }

  const vif = payload.vif;
  if (vif !== null && typeof vif === 'object' && !Array.isArray(vif)) {
    const anyHigh = Object.values(vif as Record<string, unknown>).some((v) => {
      const n = asNumber(v);
      return n !== undefined && n > 10.0;
    });
    if (anyHigh) signals.push('VifTooHigh');
  }

  const power = asNumber(payload.power);
  if (power !== undefined && power < 0.8) {
    signals.push('LowPower');
  }
  const achieved = asNumber(payload.achieved_power);
  if (achieved !== undefined && achieved < 0.8 && !signals.includes('LowPower')) {
    signals.push('LowPower');
  }

  if (payload.cox_ph_violated === true) {
    signals.push('CoxPhAssumptionViolated');
  }
  const phTest = payload.ph_test;
  if (phTest !== null && typeof phTest === 'object' && !Array.isArray(phTest)) {
    if (
      (phTest as Record<string, unknown>).violated === true &&
      !signals.includes('CoxPhAssumptionViolated')
    ) {
      signals.push('CoxPhAssumptionViolated');
    }
  }

  return signals;
}
