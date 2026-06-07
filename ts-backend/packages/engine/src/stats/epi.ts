// stats/epi.ts — OR/RR and attributable risk (Phase 3, task 7.1).
// 2x2 table:  exposed/unexposed × event/no-event.
//
//            event (D+)   no event (D-)
//   exposed     a             b
//   unexposed   c             d
//
// CIs use the standard log-transform with a normal critical value (z=1.96 at 95%).

const Z_95 = 1.959963984540054;

export interface OrRrResult {
  a: number;
  b: number;
  c: number;
  d: number;
  oddsRatio: number;
  orCiLower: number;
  orCiUpper: number;
  riskRatio: number;
  rrCiLower: number;
  rrCiUpper: number;
}

/** Odds ratio and risk ratio with 95% log-based CIs. */
export function orRr(a: number, b: number, c: number, d: number, z = Z_95): OrRrResult {
  if ([a, b, c, d].some((v) => v < 0)) {
    throw new Error('2x2 cell counts must be non-negative.');
  }
  const oddsRatio = (a * d) / (b * c);
  const seLogOr = Math.sqrt(1 / a + 1 / b + 1 / c + 1 / d);
  const lnOr = Math.log(oddsRatio);
  const orCiLower = Math.exp(lnOr - z * seLogOr);
  const orCiUpper = Math.exp(lnOr + z * seLogOr);

  const riskExposed = a / (a + b);
  const riskUnexposed = c / (c + d);
  const riskRatio = riskExposed / riskUnexposed;
  const seLogRr = Math.sqrt(1 / a - 1 / (a + b) + 1 / c - 1 / (c + d));
  const lnRr = Math.log(riskRatio);
  const rrCiLower = Math.exp(lnRr - z * seLogRr);
  const rrCiUpper = Math.exp(lnRr + z * seLogRr);

  return {
    a,
    b,
    c,
    d,
    oddsRatio,
    orCiLower,
    orCiUpper,
    riskRatio,
    rrCiLower,
    rrCiUpper,
  };
}

export interface AttributableRiskResult {
  riskExposed: number;
  riskUnexposed: number;
  riskDifference: number;
  attributableRiskPercent: number;
  populationAttributableRisk: number;
}

/**
 * Attributable risk measures from a 2x2 table.
 * - risk difference = risk_exposed - risk_unexposed
 * - AR% (attributable fraction in the exposed) = (RR - 1)/RR
 * - PAR (population attributable risk) = risk_total - risk_unexposed
 */
export function attributableRisk(a: number, b: number, c: number, d: number): AttributableRiskResult {
  if ([a, b, c, d].some((v) => v < 0)) {
    throw new Error('2x2 cell counts must be non-negative.');
  }
  const riskExposed = a / (a + b);
  const riskUnexposed = c / (c + d);
  const riskDifference = riskExposed - riskUnexposed;
  const rr = riskExposed / riskUnexposed;
  const attributableRiskPercent = rr > 0 ? ((rr - 1) / rr) * 100 : 0;
  const total = a + b + c + d;
  const riskTotal = (a + c) / total;
  const populationAttributableRisk = riskTotal - riskUnexposed;
  return {
    riskExposed,
    riskUnexposed,
    riskDifference,
    attributableRiskPercent,
    populationAttributableRisk,
  };
}
