import { describe, expect, it } from 'vitest';
import { detectRiskSignals } from '../../packages/server/src/conversation/risk-signals.js';

describe('detectRiskSignals', () => {
  it('does not treat a p-value above alpha as a method risk', () => {
    expect(detectRiskSignals({ p_value: 0.72 })).toEqual([]);
  });

  it('does not infer observed power from an analysis result', () => {
    expect(detectRiskSignals({ achieved_power: 0.42 })).toEqual([]);
    expect(detectRiskSignals({ power: 0.42 })).toEqual([]);
  });

  it('reports low power only for a design-stage calculation', () => {
    expect(detectRiskSignals({ achieved_power: 0.79 }, { phase: 'design' })).toEqual([
      'LowPower',
    ]);
    expect(detectRiskSignals({ power: 0.8 }, { phase: 'design' })).toEqual([]);
  });

  it('preserves actionable model-diagnostic risks', () => {
    expect(
      detectRiskSignals({
        vif: { age: 2.1, bmi: 12.4 },
        ph_test: { violated: true },
      }),
    ).toEqual(['VifTooHigh', 'CoxPhAssumptionViolated']);
  });

  it('maps the unified model diagnostic states to method risks', () => {
    expect(detectRiskSignals({
      model_diagnostics: {
        convergence: { status: 'failed' },
        sparse_data: { status: 'warning' },
        collinearity: { status: 'warning' },
      },
    })).toEqual(['ModelConvergenceFailed', 'SparseData', 'CollinearityDetected']);
    expect(detectRiskSignals({
      model_diagnostics: {
        convergence: { status: 'passed' },
        sparse_data: { status: 'passed' },
        collinearity: { status: 'passed' },
      },
    })).toEqual([]);
  });
});
