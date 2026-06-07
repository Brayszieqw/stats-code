// stats/tableone.ts — Table One descriptive summaries (Phase 2, task 5.3).
// Produces per-variable continuous (mean/sd/median/IQR) and categorical
// (counts/percent) summaries, optionally stratified by a grouping variable.

export interface ContinuousSummary {
  variable: string;
  type: 'continuous';
  n: number;
  missing: number;
  mean: number;
  sd: number;
  median: number;
  q1: number;
  q3: number;
  min: number;
  max: number;
}

export interface CategoricalSummary {
  variable: string;
  type: 'categorical';
  n: number;
  missing: number;
  levels: { level: string; count: number; percent: number }[];
}

export type VariableSummary = ContinuousSummary | CategoricalSummary;

/** Quantile from a sorted array via linear interpolation (matches quantile_sorted). */
export function quantileSorted(sorted: readonly number[], q: number): number {
  if (sorted.length === 0) return Number.NaN;
  if (sorted.length === 1) return sorted[0]!;
  const n = sorted.length;
  const pos = q * (n - 1);
  const lower = Math.floor(pos);
  const upper = Math.ceil(pos);
  if (lower === upper || upper >= n) {
    return sorted[Math.min(lower, n - 1)]!;
  }
  const frac = pos - lower;
  return sorted[lower]! * (1 - frac) + sorted[upper]! * frac;
}

/** Summarize a continuous variable (missing = count of null/NaN entries). */
export function summarizeContinuous(variable: string, raw: readonly (number | null)[]): ContinuousSummary {
  const values = raw.filter((v): v is number => v !== null && Number.isFinite(v));
  const missing = raw.length - values.length;
  const n = values.length;
  if (n === 0) {
    return { variable, type: 'continuous', n: 0, missing, mean: NaN, sd: NaN, median: NaN, q1: NaN, q3: NaN, min: NaN, max: NaN };
  }
  const mean = values.reduce((s, v) => s + v, 0) / n;
  const variance = n > 1 ? values.reduce((s, v) => s + (v - mean) ** 2, 0) / (n - 1) : 0;
  const sd = Math.sqrt(variance);
  const sorted = [...values].sort((a, b) => a - b);
  return {
    variable,
    type: 'continuous',
    n,
    missing,
    mean,
    sd,
    median: quantileSorted(sorted, 0.5),
    q1: quantileSorted(sorted, 0.25),
    q3: quantileSorted(sorted, 0.75),
    min: sorted[0]!,
    max: sorted[sorted.length - 1]!,
  };
}

/** Summarize a categorical variable (missing = empty/null entries). */
export function summarizeCategorical(variable: string, raw: readonly (string | null)[]): CategoricalSummary {
  const values = raw.filter((v): v is string => v !== null && v !== '');
  const missing = raw.length - values.length;
  const counts = new Map<string, number>();
  for (const v of values) {
    counts.set(v, (counts.get(v) ?? 0) + 1);
  }
  const n = values.length;
  const levels = [...counts.entries()]
    .sort((a, b) => (a[0] < b[0] ? -1 : a[0] > b[0] ? 1 : 0))
    .map(([level, count]) => ({ level, count, percent: n > 0 ? (count / n) * 100 : 0 }));
  return { variable, type: 'categorical', n, missing, levels };
}
