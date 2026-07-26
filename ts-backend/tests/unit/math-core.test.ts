import { describe, it, expect } from 'vitest';
import { math } from '@stats-code/engine';

const {
  normalCdf,
  chiSquareCdf,
  logGamma,
  inverseNormal,
  tDistributionPValue,
  fDistributionPValue,
  regularizedIncompleteBeta,
  matrixMultiply,
  invertMatrix,
  invertMatrixWithRidge,
  matrixDeterminant,
  cholesky,
  qr,
  jacobiEigh,
  transpose,
  dot,
} = math;

const close = (a: number, b: number, tol: number) => Math.abs(a - b) < tol;

describe('normal CDF (known values, matches R pnorm)', () => {
  it('pnorm(0) = 0.5', () => expect(close(normalCdf(0), 0.5, 1e-7)).toBe(true));
  it('pnorm(1.96) ≈ 0.9750021', () => expect(close(normalCdf(1.96), 0.97500210, 1e-6)).toBe(true));
  it('pnorm(-1.96) ≈ 0.0249979', () => expect(close(normalCdf(-1.96), 0.02499790, 1e-6)).toBe(true));
  it('pnorm(3) ≈ 0.9986501', () => expect(close(normalCdf(3), 0.9986501, 1e-5)).toBe(true));
});

describe('normal CDF tail (RELATIVE accuracy — these are p-values)', () => {
  // The absolute tolerances above cannot see tail error: the A&S rational
  // approximation this kernel used to employ carried ~7.5e-8 absolute error,
  // which swamps every value here, and past |z|≈9 it returned exactly 0 —
  // reporting p = 0 for Wald tests whose true p is ~1e-19.
  // Reference values from R's pnorm(-z, log.p = FALSE). Tolerance is 1e-6
  // relative: the incomplete-gamma continued fraction loses a few digits as the
  // argument grows, which is immaterial for a p-value but real.
  const R_PNORM_LOWER_TAIL: Array<[number, number]> = [
    [4, 3.167124183311998e-5],
    [5, 2.866515718791939e-7],
    [6, 9.865876450376946e-10],
    [8, 6.220960574271786e-16],
    [10, 7.619853024160525e-24],
    [15, 3.670966917678476e-51],
    [20, 2.753624119125215e-89],
    [30, 4.906713927148880e-198],
  ];

  for (const [z, expected] of R_PNORM_LOWER_TAIL) {
    it(`pnorm(-${z}) ≈ ${expected.toExponential(4)} to 1e-6 relative`, () => {
      const actual = normalCdf(-z);
      expect(actual).toBeGreaterThan(0);
      expect(Math.abs(actual - expected) / expected).toBeLessThan(1e-6);
    });
  }

  it('stays finite and monotone at the extremes', () => {
    expect(normalCdf(Number.NEGATIVE_INFINITY)).toBe(0);
    expect(normalCdf(Number.POSITIVE_INFINITY)).toBe(1);
    expect(Number.isNaN(normalCdf(Number.NaN))).toBe(true);
    expect(normalCdf(-30)).toBeGreaterThan(0);
    expect(normalCdf(-30)).toBeLessThan(normalCdf(-20));
    // Past |z|≈38, exp(-z²/2) falls below the smallest subnormal double, so 0
    // is the representable answer rather than a kernel defect. Documented here
    // so a future reader does not mistake it for the bug this suite guards.
    expect(normalCdf(-40)).toBe(0);
  });
});

describe('chi-square CDF (known values)', () => {
  it('pchisq(3.841, 1) ≈ 0.95', () => expect(close(chiSquareCdf(3.841, 1), 0.95, 5e-3)).toBe(true));
  it('pchisq(5.991, 2) ≈ 0.95', () => expect(close(chiSquareCdf(5.991, 2), 0.95, 5e-3)).toBe(true));
  it('pchisq(0, 1) = 0', () => expect(chiSquareCdf(0, 1)).toBe(0));
});

describe('log-gamma (known values)', () => {
  it('lgamma(1) = 0', () => expect(close(logGamma(1), 0, 1e-9)).toBe(true));
  it('lgamma(2) = 0', () => expect(close(logGamma(2), 0, 1e-9)).toBe(true));
  it('lgamma(5) = ln(24) ≈ 3.178054', () => expect(close(logGamma(5), 3.178054, 1e-4)).toBe(true));
  it('lgamma(0.5) = ln(√π) ≈ 0.5723649', () => expect(close(logGamma(0.5), 0.5723649, 1e-4)).toBe(true));
});

describe('inverse normal', () => {
  it('Φ⁻¹(0.5) = 0', () => expect(close(inverseNormal(0.5), 0, 1e-9)).toBe(true));
  it('matches high-precision reference quantiles', () => {
    expect(inverseNormal(1e-12)).toBeCloseTo(-7.034483825301131, 13);
    expect(inverseNormal(0.025)).toBeCloseTo(-1.959963984540054, 13);
    expect(inverseNormal(0.975)).toBeCloseTo(1.959963984540054, 13);
    expect(inverseNormal(1 - 1e-12)).toBeCloseTo(7.034486910047836, 13);
  });
  it('round-trips Φ(Φ⁻¹(0.975)) ≈ 0.975', () =>
    expect(close(normalCdf(inverseNormal(0.975)), 0.975, 1e-6)).toBe(true));
});

describe('t and F p-values', () => {
  it('t two-sided p at t=2, df=20 in (0,1)', () => {
    const p = tDistributionPValue(2, 20);
    expect(p).toBeGreaterThan(0);
    expect(p).toBeLessThan(1);
  });
  it('F upper-tail p at f=3, df=(2,20) in (0,1)', () => {
    const p = fDistributionPValue(3, 2, 20);
    expect(p).toBeGreaterThan(0);
    expect(p).toBeLessThan(1);
  });
  it('maps infinite test statistics to the significant tail without hiding NaN', () => {
    expect(tDistributionPValue(Number.POSITIVE_INFINITY, 20)).toBe(0);
    expect(tDistributionPValue(Number.NEGATIVE_INFINITY, 20)).toBe(0);
    expect(fDistributionPValue(Number.POSITIVE_INFINITY, 2, 20)).toBe(0);
    expect(Number.isNaN(tDistributionPValue(Number.NaN, 20))).toBe(true);
    expect(Number.isNaN(fDistributionPValue(Number.NaN, 2, 20))).toBe(true);
  });
  it('regularized incomplete beta is monotone in x', () => {
    expect(regularizedIncompleteBeta(2, 3, 0.2)).toBeLessThan(regularizedIncompleteBeta(2, 3, 0.8));
    expect(regularizedIncompleteBeta(2, 3, 0)).toBe(0);
    expect(regularizedIncompleteBeta(2, 3, 1)).toBe(1);
  });
});

describe('linear algebra', () => {
  it('matrix multiply', () => {
    const a = [
      [1, 2],
      [3, 4],
    ];
    const b = [
      [5, 6],
      [7, 8],
    ];
    expect(matrixMultiply(a, b)).toEqual([
      [19, 22],
      [43, 50],
    ]);
  });

  it('inverts a 2x2 and A·A⁻¹ = I', () => {
    const a = [
      [4, 7],
      [2, 6],
    ];
    const inv = invertMatrix(a);
    const prod = matrixMultiply(a, inv);
    expect(close(prod[0]![0]!, 1, 1e-12)).toBe(true);
    expect(close(prod[1]![1]!, 1, 1e-12)).toBe(true);
    expect(close(prod[0]![1]!, 0, 1e-12)).toBe(true);
  });

  it('throws on a singular matrix', () => {
    expect(() =>
      invertMatrix([
        [1, 2],
        [2, 4],
      ]),
    ).toThrow();
  });

  it('reports the ridge used to invert a singular matrix', () => {
    const result = invertMatrixWithRidge([
      [1, 1],
      [1, 1],
    ]);
    expect(result.ridgeApplied).toBe(true);
    expect(result.ridgeValue).toBeGreaterThan(0);
    expect(result.inverse.flat().every(Number.isFinite)).toBe(true);
  });

  it('determinant of [[4,7],[2,6]] = 10', () => {
    expect(close(matrixDeterminant([[4, 7], [2, 6]]), 10, 1e-12)).toBe(true);
  });

  it('cholesky: L·Lᵀ reconstructs an SPD matrix', () => {
    const a = [
      [4, 2],
      [2, 3],
    ];
    const l = cholesky(a);
    const recon = matrixMultiply(l, transpose(l));
    expect(close(recon[0]![0]!, 4, 1e-12)).toBe(true);
    expect(close(recon[0]![1]!, 2, 1e-12)).toBe(true);
    expect(close(recon[1]![1]!, 3, 1e-12)).toBe(true);
  });

  it('cholesky rejects a non-positive-definite matrix', () => {
    expect(() =>
      cholesky([
        [1, 2],
        [2, 1],
      ]),
    ).toThrow();
  });

  it('QR: Q·R reconstructs A and Q has orthonormal columns', () => {
    const a = [
      [12, -51],
      [6, 167],
      [-4, 24],
    ];
    const { q, r } = qr(a);
    const recon = matrixMultiply(q, r);
    for (let i = 0; i < 3; i += 1) {
      for (let j = 0; j < 2; j += 1) {
        expect(close(recon[i]![j]!, a[i]![j]!, 1e-9)).toBe(true);
      }
    }
    // columns orthonormal: q0·q0 = 1, q0·q1 = 0
    const col0 = q.map((row) => row[0]!);
    const col1 = q.map((row) => row[1]!);
    expect(close(dot(col0, col0), 1, 1e-9)).toBe(true);
    expect(close(dot(col0, col1), 0, 1e-9)).toBe(true);
  });

  it('jacobi eigendecomposition of a diagonal matrix returns sorted eigenvalues', () => {
    const { eigenvalues } = jacobiEigh([
      [2, 0],
      [0, 5],
    ]);
    expect(close(eigenvalues[0]!, 5, 1e-9)).toBe(true);
    expect(close(eigenvalues[1]!, 2, 1e-9)).toBe(true);
  });

  it('jacobi eigenvalues of a symmetric 2x2 match the characteristic roots', () => {
    // [[2,1],[1,2]] has eigenvalues 3 and 1.
    const { eigenvalues } = jacobiEigh([
      [2, 1],
      [1, 2],
    ]);
    expect(close(eigenvalues[0]!, 3, 1e-9)).toBe(true);
    expect(close(eigenvalues[1]!, 1, 1e-9)).toBe(true);
  });
});
