// server/conversation/risk-signals.ts — risk signal detection.
//
// Only actionable method/diagnostic problems belong here. A p-value above an
// arbitrary alpha is a result, not a method risk. Power is assessed only while
// designing a study; post-hoc/observed power must not be inferred from results.

import type { RiskSignal } from './skill-runner-types.js';

function asNumber(value: unknown): number | undefined {
  return typeof value === 'number' && Number.isFinite(value) ? value : undefined;
}

export interface DetectRiskSignalsOptions {
  phase?: 'analysis' | 'design';
}

export function detectRiskSignals(
  payload: Record<string, unknown>,
  options: DetectRiskSignalsOptions = {},
): RiskSignal[] {
  const signals: RiskSignal[] = [];

  const vif = payload.vif;
  if (vif !== null && typeof vif === 'object' && !Array.isArray(vif)) {
    const anyHigh = Object.values(vif as Record<string, unknown>).some((v) => {
      const n = asNumber(v);
      return n !== undefined && n > 10.0;
    });
    if (anyHigh) signals.push('VifTooHigh');
  }

  if (options.phase === 'design') {
    const power = asNumber(payload.power) ?? asNumber(payload.achieved_power);
    if (power !== undefined && power < 0.8) {
      signals.push('LowPower');
    }
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

  const modelDiagnostics = payload.model_diagnostics;
  if (modelDiagnostics !== null && typeof modelDiagnostics === 'object' && !Array.isArray(modelDiagnostics)) {
    const diagnostics = modelDiagnostics as Record<string, unknown>;
    const convergence = diagnostics.convergence;
    if (convergence !== null && typeof convergence === 'object' && !Array.isArray(convergence)) {
      if ((convergence as Record<string, unknown>).status === 'failed') {
        signals.push('ModelConvergenceFailed');
      }
    }
    const sparseData = diagnostics.sparse_data;
    if (sparseData !== null && typeof sparseData === 'object' && !Array.isArray(sparseData)) {
      if ((sparseData as Record<string, unknown>).status === 'warning') {
        signals.push('SparseData');
      }
    }
    const collinearity = diagnostics.collinearity;
    if (collinearity !== null && typeof collinearity === 'object' && !Array.isArray(collinearity)) {
      const status = (collinearity as Record<string, unknown>).status;
      if (status === 'warning' || status === 'failed') {
        signals.push('CollinearityDetected');
      }
    }
  }

  return signals;
}
