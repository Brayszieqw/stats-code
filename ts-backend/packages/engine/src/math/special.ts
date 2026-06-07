// math/special.ts — special functions (Phase 2, task 5.1).
// Transcribed from crates/stats-code/src/math/mod.rs.
//
// Pure, deterministic, no native addons: log-gamma (Lanczos g=7), regularized
// lower/upper incomplete gamma, and the regularized incomplete beta. These are
// the primary parity risk and are validated independently against the Rust
// outputs and known values.

const SQRT_2PI = 2.5066282746310002;

/** Numerically stable sigmoid. */
export function sigmoid(value: number): number {
  if (value >= 0) {
    const e = Math.exp(-value);
    return 1 / (1 + e);
  }
  const e = Math.exp(value);
  return e / (1 + e);
}

/** Lanczos log-gamma with g=7 (9 coefficients), matching the Rust kernel. */
export function logGamma(x: number): number {
  if (x < 0.5) {
    const reflection = Math.PI / Math.sin(Math.PI * x);
    return Math.log(reflection) - logGamma(1 - x);
  }
  const c = [
    0.9999999999998099, 676.5203681218851, -1259.1392167224028, 771.3234287776531,
    -176.6150291621406, 12.507343278686905, -0.13857109526572012, 9.984369578019572e-6,
    1.5056327351493116e-7,
  ];
  const xx = x - 1;
  const t = xx + 7.5;
  let s = c[0]!;
  for (let i = 1; i < c.length; i += 1) {
    s += c[i]! / (xx + i);
  }
  return 0.5 * Math.log(2 * Math.PI) + (xx + 0.5) * Math.log(t) - t + Math.log(s);
}

/** ln(n choose k). */
export function lnChoose(n: number, k: number): number {
  if (k > n) {
    return Number.NEGATIVE_INFINITY;
  }
  return logGamma(n + 1) - logGamma(k + 1) - logGamma(n - k + 1);
}

/** Upper regularized gamma Q(a,x) via continued fraction (Lentz's method). */
export function regularizedUpperGammaCf(a: number, x: number): number {
  const logPrefix = -x + a * Math.log(x) - logGamma(a);
  const tiny = 1e-30;
  let b = x + 1 - a;
  let c = 1 / tiny;
  let d = 1 / b;
  let h = d;
  for (let i = 1; i < 300; i += 1) {
    const an = -i * (i - a);
    b += 2;
    d = an * d + b;
    if (Math.abs(d) < tiny) d = tiny;
    c = b + an / c;
    if (Math.abs(c) < tiny) c = tiny;
    d = 1 / d;
    const del = d * c;
    h *= del;
    if (Math.abs(del - 1) < 1e-14) break;
  }
  return clamp01(h * Math.exp(logPrefix));
}

/** Regularized lower incomplete gamma P(a,x) = γ(a,x)/Γ(a). */
export function regularizedLowerGamma(a: number, x: number): number {
  if (x <= 0) return 0;
  if (x > a + 1) {
    return 1 - regularizedUpperGammaCf(a, x);
  }
  const logPrefix = -x + a * Math.log(x) - logGamma(a);
  let sum = 1 / a;
  let term = 1 / a;
  for (let n = 1; n < 300; n += 1) {
    term *= x / (a + n);
    sum += term;
    if (Math.abs(term) < Math.abs(sum) * 1e-14) break;
  }
  return clamp01(sum * Math.exp(logPrefix));
}

/** Regularized incomplete beta function I_x(a,b) via continued fraction. */
export function regularizedIncompleteBeta(a: number, b: number, x: number): number {
  if (x <= 0) return 0;
  if (x >= 1) return 1;
  // Symmetry transform for better convergence.
  if (x > (a + 1) / (a + b + 2)) {
    return 1 - regularizedIncompleteBeta(b, a, 1 - x);
  }
  const logPrefix =
    a * Math.log(x) + b * Math.log(1 - x) - logGamma(a) - logGamma(b) + logGamma(a + b);
  const prefix = Math.exp(logPrefix) / a;
  let c = 1;
  let d = 1 / (1 - ((a + b) * x) / (a + 1));
  let f = d;
  for (let m = 1; m < 200; m += 1) {
    const mf = m;
    // Even step
    const aEven = (mf * (b - mf) * x) / ((a + 2 * mf - 1) * (a + 2 * mf));
    d = 1 / (1 + aEven * d);
    c = 1 + aEven / c;
    f *= c * d;
    // Odd step
    const aOdd = (-(a + mf) * (a + b + mf) * x) / ((a + 2 * mf) * (a + 2 * mf + 1));
    d = 1 / (1 + aOdd * d);
    c = 1 + aOdd / c;
    const delta = c * d;
    f *= delta;
    if (Math.abs(delta - 1) < 1e-15) break;
  }
  return prefix * f;
}

/** Alias with the (x, a, b) parameter order used by the F/t kernels. */
export function regularizedBetaIncomplete(x: number, a: number, b: number): number {
  return regularizedIncompleteBeta(a, b, x);
}

export function clamp01(v: number): number {
  if (v < 0) return 0;
  if (v > 1) return 1;
  return v;
}

export { SQRT_2PI };
