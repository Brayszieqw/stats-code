// tests/property/skill-algorithm-map.property.test.ts — Property 1.
//
// skillToAlgorithm is a pure total function: repeated invocations return the
// same result, and the six output-level ids map to fixed algorithm ids; all
// other ids map to null.
//
// Validates: Requirements 4.4, 4.5, 4.6, 13.3

import { describe, it, expect } from 'vitest';
import fc from 'fast-check';
import { skillToAlgorithm } from '@stats-code/server';

const FIXED: Record<string, string> = {
  tableone: 'tableone',
  ttest: 'ttest',
  model_linear: 'linear',
  model_logistic: 'logistic',
  model_cox: 'cox',
  survival_km: 'kaplan_meier',
};

describe('Property 1: skill→algorithm determinism (Requirements 4.4, 4.5, 4.6, 13.3)', () => {
  it('maps the output-level ids to their fixed algorithm ids', () => {
    for (const [skill, algo] of Object.entries(FIXED)) {
      expect(skillToAlgorithm(skill)).toBe(algo);
    }
  });

  it('returns the same result on repeated invocations for arbitrary ids', () => {
    fc.assert(
      fc.property(fc.string(), (skillId) => {
        const first = skillToAlgorithm(skillId);
        for (let i = 0; i < 10; i += 1) {
          expect(skillToAlgorithm(skillId)).toBe(first);
        }
        // Output-level ids map to their fixed value; everything else → null.
        if (Object.prototype.hasOwnProperty.call(FIXED, skillId)) {
          expect(first).toBe(FIXED[skillId]);
        } else {
          expect(first).toBeNull();
        }
      }),
      { numRuns: 200 },
    );
  });

  it('non-output-level skills (inspect, power) and unknowns map to null', () => {
    expect(skillToAlgorithm('inspect')).toBeNull();
    expect(skillToAlgorithm('power')).toBeNull();
    expect(skillToAlgorithm('')).toBeNull();
    expect(skillToAlgorithm('nonexistent')).toBeNull();
  });
});
