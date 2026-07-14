// stats/nonparametric.ts — Mann-Whitney U and Kruskal-Wallis (Phase 2, task 5.3).
// Transcribed from crates/stats-code/src/math (kruskal_wallis_test) and the
// nonparametric handler. Uses the normal approximation with tie correction for
// Mann-Whitney, matching the Rust behavior.

import { normalCdf, chiSquareP } from '../math/distributions.js';
import { rankWithTies } from './rank.js';

export interface MannWhitneyResult {
  method: string;
  n1: number;
  n2: number;
  u: number;
  z: number;
  pValue: number;
}

/** Mann-Whitney U test (two-sided, normal approximation with tie correction). */
export function mannWhitneyU(a: readonly number[], b: readonly number[]): MannWhitneyResult {
  const n1 = a.length;
  const n2 = b.length;
  if (n1 === 0 || n2 === 0) {
    throw new Error('Mann-Whitney U requires non-empty groups.');
  }
  const pooled = [...a, ...b];
  const ranks = rankWithTies(pooled);
  let rankSum1 = 0;
  for (let i = 0; i < n1; i += 1) rankSum1 += ranks[i]!;

  const u1 = rankSum1 - (n1 * (n1 + 1)) / 2;
  const u2 = n1 * n2 - u1;
  const u = Math.min(u1, u2);

  const n = n1 + n2;
  const meanU = (n1 * n2) / 2;

  // Tie correction term.
  const tieGroups = new Map<number, number>();
  for (const v of pooled) {
    tieGroups.set(v, (tieGroups.get(v) ?? 0) + 1);
  }
  let tieSum = 0;
  for (const t of tieGroups.values()) {
    tieSum += t ** 3 - t;
  }
  const varU = ((n1 * n2) / 12) * (n + 1 - tieSum / (n * (n - 1)));
  if (varU <= 0) {
    return { method: 'Mann-Whitney U', n1, n2, u, z: 0, pValue: 1 };
  }
  const z = (u - meanU) / Math.sqrt(varU);
  const pValue = Math.min(1, 2 * normalCdf(-Math.abs(z)));
  return { method: 'Mann-Whitney U', n1, n2, u, z, pValue };
}

export interface KruskalWallisResult {
  method: string;
  k: number;
  n: number;
  h: number;
  df: number;
  pValue: number;
}

/** Kruskal-Wallis one-way ANOVA by ranks. Transcribed from kruskal_wallis_test. */
export function kruskalWallis(groups: readonly (readonly number[])[]): KruskalWallisResult {
  const k = groups.length;
  if (k < 2) {
    throw new Error('Kruskal-Wallis requires at least 2 groups.');
  }
  if (groups.some((group) => group.length === 0)) {
    throw new Error('Kruskal-Wallis groups must be non-empty.');
  }
  const pooled: { value: number; group: number }[] = [];
  groups.forEach((g, gi) => {
    for (const v of g) pooled.push({ value: v, group: gi });
  });
  const n = pooled.length;
  if (n < 3) {
    throw new Error('Kruskal-Wallis requires at least 3 observations.');
  }
  const ranks = rankWithTies(pooled.map((p) => p.value));
  const groupN = new Array<number>(k).fill(0);
  const groupRankSum = new Array<number>(k).fill(0);
  pooled.forEach((p, idx) => {
    groupN[p.group]! += 1;
    groupRankSum[p.group]! += ranks[idx]!;
  });
  let h = 0;
  for (let gi = 0; gi < k; gi += 1) {
    const ni = groupN[gi]!;
    if (ni > 0) {
      const meanRank = groupRankSum[gi]! / ni;
      h += ni * (meanRank - (n + 1) / 2) ** 2;
    }
  }
  h *= 12 / (n * (n + 1));
  const tieGroups = new Map<number, number>();
  for (const { value } of pooled) {
    tieGroups.set(value, (tieGroups.get(value) ?? 0) + 1);
  }
  let tieSum = 0;
  for (const tieSize of tieGroups.values()) {
    tieSum += tieSize ** 3 - tieSize;
  }
  const tieCorrection = 1 - tieSum / (n ** 3 - n);
  h = tieCorrection > 0 ? h / tieCorrection : 0;
  h = Math.max(0, h);
  const df = k - 1;
  const pValue = chiSquareP(h, df);
  return { method: 'Kruskal-Wallis', k, n, h, df, pValue };
}
