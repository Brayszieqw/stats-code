import { describe, it, expect } from 'vitest';
import { stats } from '@stats-code/engine';

const { power } = stats;

describe('power_single_arm (one-proportion precision)', () => {
  it('n = z²·p(1-p)/margin²; p=0.5, margin=0.05, α=0.05 → 385', () => {
    // z_0.975 = 1.959964; n = 1.959964² * 0.25 / 0.0025 = 384.15 → 385
    const r = power.powerSingleArm(0.5, 0.05, 0.05);
    expect(r.totalN).toBe(385);
  });

  it('rejects an invalid margin', () => {
    expect(() => power.powerSingleArm(0.5, 0, 0.05)).toThrow();
  });
});

describe('power_phase2 (two proportions)', () => {
  it('produces a sensible, symmetric (allocation 1) sample size', () => {
    const r = power.powerPhase2(0.5, 0.3, 0.05, 0.8, 1);
    expect(r.group1N).toBe(r.group2N);
    expect(r.totalN).toBe(r.group1N + r.group2N);
    expect(r.effectSize).toBeCloseTo(0.2, 12);
    // Known ballpark for p1=0.5,p2=0.3,α=0.05,power=0.8: ~93 per group.
    expect(r.group1N).toBeGreaterThan(80);
    expect(r.group1N).toBeLessThan(110);
  });

  it('rejects equal proportions', () => {
    expect(() => power.powerPhase2(0.4, 0.4)).toThrow();
  });
});

describe('power_phase3 (two means)', () => {
  it('matches the textbook formula for a standardized effect', () => {
    // mean diff 5, sd 10 → d=0.5; α=0.05, power=0.8, allocation 1.
    // n1 = (1.959964 + 0.841621)² * 100 * 2 / 25 ≈ 62.79 → 63 per group.
    const r = power.powerPhase3(10, 5, 10, 0.05, 0.8, 1);
    expect(r.effectSize).toBeCloseTo(0.5, 12);
    expect(r.group1N).toBe(63);
    expect(r.group2N).toBe(63);
  });

  it('rejects a non-positive sd', () => {
    expect(() => power.powerPhase3(1, 2, 0)).toThrow();
  });
});
