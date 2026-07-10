/**
 * Normalize regression coefficient fields from TS backend (camelCase) and
 * legacy snake_case payloads so result tables/charts never crash on .toFixed.
 */

export interface NormalizedCoeff {
  term: string;
  beta: number | null;
  standardError: number | null;
  ciLower: number | null;
  ciUpper: number | null;
  pValue: number | null;
  oddsRatio: number | null;
  hazardRatio: number | null;
  reference: string | null;
}

function num(v: unknown): number | null {
  return typeof v === 'number' && Number.isFinite(v) ? v : null;
}

function str(v: unknown): string | null {
  return typeof v === 'string' && v.length > 0 ? v : null;
}

/** Format a finite number or return a dash placeholder. */
export function fmtNum(v: number | null | undefined, digits = 3): string {
  if (v === null || v === undefined || !Number.isFinite(v)) return '-';
  return v.toFixed(digits);
}

/** Format a p-value with the conventional <0.001 threshold. */
export function fmtP(v: number | null | undefined, digits = 3): string {
  if (v === null || v === undefined || !Number.isFinite(v)) return '-';
  if (v < 0.001) return '<0.001';
  return v.toFixed(digits);
}

export function normalizeCoeff(raw: Record<string, unknown>, index: number): NormalizedCoeff {
  const term =
    str(raw.term) ??
    str(raw.name) ??
    str(raw.variable) ??
    str(raw.predictor) ??
    (typeof raw.index === 'number' ? `β${raw.index}` : `β${index}`);

  return {
    term,
    beta: num(raw.beta) ?? num(raw.estimate) ?? num(raw.coef),
    standardError: num(raw.standard_error) ?? num(raw.stdError) ?? num(raw.se),
    ciLower: num(raw.ci_lower) ?? num(raw.ciLower) ?? num(raw.lower),
    ciUpper: num(raw.ci_upper) ?? num(raw.ciUpper) ?? num(raw.upper),
    pValue: num(raw.p_value) ?? num(raw.pValue) ?? num(raw.pvalue),
    oddsRatio: num(raw.odds_ratio) ?? num(raw.oddsRatio) ?? num(raw.or),
    hazardRatio: num(raw.hazard_ratio) ?? num(raw.hazardRatio) ?? num(raw.hr),
    reference: str(raw.reference),
  };
}

export function normalizeCoefficients(list: unknown): NormalizedCoeff[] {
  if (!Array.isArray(list)) return [];
  return list.map((item, i) =>
    normalizeCoeff(
      item && typeof item === 'object' ? (item as Record<string, unknown>) : {},
      i,
    ),
  );
}
