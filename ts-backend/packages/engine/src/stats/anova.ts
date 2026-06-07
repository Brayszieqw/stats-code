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
  for (const group of groups) {
    const ni = group.length;
    if (ni === 0) {
      throw new Error('ANOVA groups must be non-empty.');
    }
    const groupMean = group.reduce((s, v) => s + v, 0) / ni;
    ssBetween += ni * (groupMean - grandMean) ** 2;
    for (const v of group) {
      ssWithin += (v - groupMean) ** 2;
    }
  }
  const ssTotal = ssBetween + ssWithin;
  const dfBetween = k - 1;
  const dfWithin = nTotal - k;
  const msBetween = ssBetween / dfBetween;
  const msWithin = ssWithin / dfWithin;
  const fStatistic = msWithin > 0 ? msBetween / msWithin : Number.POSITIVE_INFINITY;
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
  };
}
