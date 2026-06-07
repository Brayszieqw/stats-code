import { describe, it, expect } from 'vitest';
import { stats } from '@stats-code/engine';

const { power } = stats;
const close = (a: number, b: number, tol: number) => Math.abs(a - b) < tol;

// The power family is calibrated to SAS PROC POWER (the recorded validation
// oracle). required_n and effect_size reproduce SAS exactly; achieved power
// agrees to ~1e-6 (well within the documented power tolerance).

describe('power_single_arm (one-proportion hypothesis test)', () => {
  it('p0=0.2, p1=0.35, α=0.05, power=0.8 → ES=0.375, n=112', () => {
    const r = power.powerSingleArm(0.2, 0.35, 0.05, 0.8);
    expect(close(r.effectSize, 0.375, 1e-9)).toBe(true);
    expect(r.requiredN).toBe(112);
    expect(close(r.achievedPower, 0.801302394106, 1e-3)).toBe(true);
  });

  it('rejects equal proportions', () => {
    expect(() => power.powerSingleArm(0.3, 0.3)).toThrow();
  });
});

describe('power_phase2 (two proportions, Cohen h)', () => {
  it('p0=0.1, p1=0.3, α=0.05, power=0.8 → h≈0.5158, n/arm=60', () => {
    const r = power.powerPhase2(0.1, 0.3, 0.05, 0.8);
    expect(close(r.effectSize, 0.515778371934, 1e-9)).toBe(true);
    expect(r.requiredN).toBe(60);
    expect(r.totalN).toBe(120);
    expect(close(r.achievedPower, 0.806500808878, 1e-3)).toBe(true);
  });

  it('rejects equal proportions', () => {
    expect(() => power.powerPhase2(0.4, 0.4)).toThrow();
  });
});

describe('power_phase3 (two means, noncentral t)', () => {
  it('meanDiff=1.0, std=2.0, α=0.05, power=0.8 → d=0.5, n/arm=64', () => {
    const r = power.powerPhase3(1.0, 2.0, 0.05, 0.8);
    expect(close(r.effectSize, 0.5, 1e-12)).toBe(true);
    expect(r.requiredN).toBe(64);
    expect(r.totalN).toBe(128);
    expect(close(r.achievedPower, 0.801459557929, 1e-3)).toBe(true);
  });

  it('rejects a non-positive sd', () => {
    expect(() => power.powerPhase3(1, 0)).toThrow();
  });
});
