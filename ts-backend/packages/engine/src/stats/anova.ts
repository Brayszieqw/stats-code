// stats/anova.ts — one-way ANOVA (Phase 2, task 5.3).
// Transcribed from the Rust ANOVA path; uses the F-distribution p-value kernel.

import { fDistributionPValue } from '../math/distributions.js';

export interface AnovaResult {
  method: string;
  k: number;
  nTotal: number;
  ssBetween: number;
  ssWithin: number;
  ssTotal: number;
  dfBetween: number;
  dfWithin: number;
  msBetween: number;
  msWithin: number;
  fStatistic: number;
  pValue: number;
  etaSquared: number;
  degenerate: boolean;
}

/** One-way ANOVA across k groups. */
export function oneWayAnova(groups: readonly (readonly number[])[]): AnovaResult {
  const k = groups.length;
  if (k < 2) {
    throw new Error('ANOVA requires at least 2 groups.');
  }
  const allValues = groups.flat();
  const nTotal = allValues.length;
  if (nTotal <= k) {
    throw new Error('ANOVA requires more observations than groups.');
  }
  const grandMean = allValues.reduce((s, v) => s + v, 0) / nTotal;

  let ssBetween = 0;
  let ssWithin = 0;
  // Does any group hold genuinely differing values? Judged per group against
  // that group's own magnitude, so the answer does not depend on how far apart
  // the group means are.
  let withinVariation = false;
  for (const group of groups) {
    const ni = group.length;
    if (ni === 0) {
      throw new Error('ANOVA groups must be non-empty.');
    }
    const groupMean = group.reduce((s, v) => s + v, 0) / ni;
    ssBetween += ni * (groupMean - grandMean) ** 2;
    let groupScale = 1;
    for (const v of group) {
      ssWithin += (v - groupMean) ** 2;
      const magnitude = Math.abs(v);
      if (magnitude > groupScale) groupScale = magnitude;
    }
    if (!withinVariation) {
      const first = group[0]!;
      withinVariation = group.some((v) => Math.abs(v - first) > groupScale * 1e-12);
    }
  }
  if (!Number.isFinite(grandMean) || !Number.isFinite(ssBetween) || !Number.isFinite(ssWithin)) {
    throw new Error('ANOVA requires finite observations.');
  }
  const ssTotal = ssBetween + ssWithin;
  const dfBetween = k - 1;
  const dfWithin = nTotal - k;
  const msBetween = ssBetween / dfBetween;
  const msWithin = ssWithin / dfWithin;
  // `msWithin === 0` only caught groups whose repeated value is exactly
  // representable: [0.1,0.1,0.1] vs [0.2,0.2,0.2] leaves msWithin ≈ 7e-34 from
  // mean rounding — zero within-group variance in every sense that matters, yet
  // the exact test called it non-zero and the F ratio was reported as real.
  const degenerate = !withinVariation;
  const fStatistic = !degenerate
    ? msBetween / msWithin
    : msBetween > 0
      ? Number.POSITIVE_INFINITY
      : 0;
  const pValue = fDistributionPValue(fStatistic, dfBetween, dfWithin);
  const etaSquared = ssTotal > 0 ? ssBetween / ssTotal : 0;

  return {
    method: 'One-way ANOVA',
    k,
    nTotal,
    ssBetween,
    ssWithin,
    ssTotal,
    dfBetween,
    dfWithin,
    msBetween,
    msWithin,
    fStatistic,
    pValue,
    etaSquared,
    degenerate,
  };
}
