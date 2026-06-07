import { describe, it, expect } from 'vitest';
import fc from 'fast-check';
import { parity } from '@stats-code/engine';

const {
  THRESHOLDS,
  DEFAULT_NON_ITERATIVE,
  DEFAULT_ITERATIVE,
  isIterativeAlgorithm,
  toleranceForAlgorithm,
  differences,
  failPredicate,
  compareScalar,
} = parity;

describe('threshold constants and class selection', () => {
  it('encodes the spec-mandated default thresholds', () => {
    expect(DEFAULT_NON_ITERATIVE).toEqual({ absolute: 1e-9, relative: 1e-6 });
    expect(DEFAULT_ITERATIVE).toEqual({ absolute: 1e-7, relative: 1e-4 });
    expect(THRESHOLDS.default).toEqual(DEFAULT_NON_ITERATIVE);
    expect(THRESHOLDS.iterative).toEqual(DEFAULT_ITERATIVE);
  });

  it('classifies cox and logistic as iterative', () => {
    expect(isIterativeAlgorithm('cox')).toBe(true);
    expect(isIterativeAlgorithm('logistic')).toBe(true);
    expect(isIterativeAlgorithm('ttest')).toBe(false);
  });

  it('selects tolerance by algorithm class', () => {
    expect(toleranceForAlgorithm('ttest')).toEqual(DEFAULT_NON_ITERATIVE);
    expect(toleranceForAlgorithm('cox')).toEqual(DEFAULT_ITERATIVE);
  });

  it('honors per-algorithm overrides', () => {
    const override = { ttest: { absolute: 1e-3, relative: 1e-2 } };
    expect(toleranceForAlgorithm('ttest', override)).toEqual(override.ttest);
  });
});

describe('differences', () => {
  it('returns null relative diff when |reference| <= absTol (n/a case)', () => {
    const d = differences(1e-12, 5e-10, 1e-9);
    expect(d.relativeDifference).toBeNull();
    expect(d.absoluteDifference).toBeCloseTo(Math.abs(1e-12 - 5e-10), 20);
  });

  it('computes relative diff when reference magnitude exceeds absTol', () => {
    const d = differences(2, 1, 1e-9);
    expect(d.absoluteDifference).toBe(1);
    expect(d.relativeDifference).toBe(1);
  });
});

describe('failPredicate', () => {
  it('never fails when relDiff is null', () => {
    expect(failPredicate(1e9, null, 1e-9, 1e-6)).toBe(false);
  });

  it('fails only when BOTH abs and rel exceed their tolerances', () => {
    expect(failPredicate(2e-9, 2e-6, 1e-9, 1e-6)).toBe(true);
    expect(failPredicate(2e-9, 0.5e-6, 1e-9, 1e-6)).toBe(false); // rel within
    expect(failPredicate(0.5e-9, 2e-6, 1e-9, 1e-6)).toBe(false); // abs within
  });

  it('uses strict > (equal-to-tolerance does not trip)', () => {
    expect(failPredicate(1e-9, 1e-6, 1e-9, 1e-6)).toBe(false);
  });
});

describe('compareScalar', () => {
  it('passes identical values', () => {
    expect(compareScalar(1.5, 1.5, DEFAULT_NON_ITERATIVE).status).toBe('pass');
  });

  it('errors on non-finite inputs', () => {
    expect(compareScalar(NaN, 1, DEFAULT_NON_ITERATIVE).status).toBe('error');
    expect(compareScalar(1, Infinity, DEFAULT_NON_ITERATIVE).status).toBe('error');
  });

  it('fails a gross mismatch', () => {
    expect(compareScalar(10, 1, DEFAULT_NON_ITERATIVE).status).toBe('fail');
  });
});

describe('Property 7: parity threshold predicate (fast-check)', () => {
  it('identical finite values always pass', () => {
    fc.assert(
      fc.property(
        fc.double({ min: -1e6, max: 1e6, noNaN: true, noDefaultInfinity: true }),
        (v) => {
          expect(compareScalar(v, v, DEFAULT_NON_ITERATIVE).status).toBe('pass');
        },
      ),
    );
  });

  it('failPredicate matches the documented conjunction for arbitrary diffs', () => {
    fc.assert(
      fc.property(
        fc.double({ min: 0, max: 1e6, noNaN: true, noDefaultInfinity: true }),
        fc.option(fc.double({ min: 0, max: 1e6, noNaN: true, noDefaultInfinity: true }), {
          nil: null,
        }),
        fc.double({ min: 0, max: 1, noNaN: true, noDefaultInfinity: true }),
        fc.double({ min: 0, max: 1, noNaN: true, noDefaultInfinity: true }),
        (absDiff, relDiff, absTol, relTol) => {
          const expected = relDiff === null ? false : absDiff > absTol && relDiff > relTol;
          expect(failPredicate(absDiff, relDiff, absTol, relTol)).toBe(expected);
        },
      ),
    );
  });
});
