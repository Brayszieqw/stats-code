// tests/parity/power.parity.test.ts — power family parity suite (task 11.2).
//
// Compares power_single_arm, power_phase2, power_phase3 against the recorded
// SAS PROC POWER baselines. required_n and effect_size are diffed at the
// non-iterative threshold (1e-6/1e-9); achieved_power is diffed at a documented
// 1e-3 method tolerance because PROC POWER's internal quadrature differs from
// the engine's CDFs (power is a `recorded`, non-`live` coverage cell, so a
// looser power band is the validation contract for this family).
//
// _Requirements: 2.2, 2.4_

import { describe, it, expect } from 'vitest';
import { stats, parity } from '@stats-code/engine';
import { loadBaseline } from './fixtures.js';

const { DEFAULT_NON_ITERATIVE, compareScalar } = parity;
const EXACT = DEFAULT_NON_ITERATIVE; // 1e-6 / 1e-9 for n and effect size
const POWER_TOL = { absolute: 1e-3, relative: 1e-3 }; // documented method band
const SOFTWARES = ['sas'] as const; // SPSS has no power baselines (cell = none)

function assertExact(software: string, algo: string, metric: string, ts: number, ref: number): void {
  const r = compareScalar(ts, ref, EXACT);
  expect(r.status, `[${software}/${algo}] ${metric}: ts=${ts} ref=${ref} ${r.message}`).toBe('pass');
}

function assertPower(software: string, algo: string, ts: number, ref: number): void {
  const r = compareScalar(ts, ref, POWER_TOL);
  expect(r.status, `[${software}/${algo}] achieved_power: ts=${ts} ref=${ref} ${r.message}`).toBe('pass');
}

describe('Power family parity vs recorded SAS PROC POWER', () => {
  for (const software of SOFTWARES) {
    describe(`${software}`, () => {
      it('power_single_arm: effect size, required n, achieved power', () => {
        const base = loadBaseline(software, 'power_single_arm');
        if (!base) return;
        const s = base.input.spec;
        const r = stats.power.powerSingleArm(Number(s.p0), Number(s.p1), Number(s.alpha), Number(s.power));
        const exp = base.expected_outputs;
        assertExact(software, 'power_single_arm', 'effect_size', r.effectSize, exp.effect_size!);
        assertExact(software, 'power_single_arm', 'required_n', r.requiredN, exp.required_n!);
        if (exp.achieved_power !== undefined) {
          assertPower(software, 'power_single_arm', r.achievedPower, exp.achieved_power);
        }
      });

      it('power_phase2: Cohen h, required n per arm, achieved power', () => {
        const base = loadBaseline(software, 'power_phase2');
        if (!base) return;
        const s = base.input.spec;
        const r = stats.power.powerPhase2(Number(s.p0), Number(s.p1), Number(s.alpha), Number(s.power));
        const exp = base.expected_outputs;
        assertExact(software, 'power_phase2', 'effect_size', r.effectSize, exp.effect_size!);
        assertExact(software, 'power_phase2', 'required_n_per_arm', r.requiredN, exp.required_n_per_arm!);
        if (exp.achieved_power !== undefined) {
          assertPower(software, 'power_phase2', r.achievedPower, exp.achieved_power);
        }
      });

      it('power_phase3: standardized difference, required n per arm, achieved power (noncentral t)', () => {
        const base = loadBaseline(software, 'power_phase3');
        if (!base) return;
        const s = base.input.spec;
        const r = stats.power.powerPhase3(Number(s.mean_diff), Number(s.std), Number(s.alpha), Number(s.power));
        const exp = base.expected_outputs;
        assertExact(software, 'power_phase3', 'effect_size', r.effectSize, exp.effect_size!);
        assertExact(software, 'power_phase3', 'required_n_per_arm', r.requiredN, exp.required_n_per_arm!);
        if (exp.achieved_power !== undefined) {
          assertPower(software, 'power_phase3', r.achievedPower, exp.achieved_power);
        }
      });
    });
  }
});
