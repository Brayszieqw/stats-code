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
  continuityCorrected: boolean;
}

function correctedTable(a: number, b: number, c: number, d: number): {
  cells: [number, number, number, number];
  continuityCorrected: boolean;
} {
  if ([a, b, c, d].some((value) => !Number.isFinite(value) || value < 0)) {
    throw new Error('2x2 cell counts must be finite and non-negative.');
  }
  if (a + b === 0 || c + d === 0) {
    throw new Error('2x2 table requires non-empty exposed and unexposed groups.');
  }
  const continuityCorrected = [a, b, c, d].some((value) => value === 0);
  const correction = continuityCorrected ? 0.5 : 0;
  return {
    cells: [a + correction, b + correction, c + correction, d + correction],
    continuityCorrected,
  };
}

/** Odds ratio and risk ratio with 95% log-based CIs. */
export function orRr(a: number, b: number, c: number, d: number, z = Z_95): OrRrResult {
  if (!Number.isFinite(z) || z <= 0) {
    throw new Error('Confidence interval critical value must be positive and finite.');
  }
  const { cells: [aa, bb, cc, dd], continuityCorrected } = correctedTable(a, b, c, d);
  const oddsRatio = (aa * dd) / (bb * cc);
  const seLogOr = Math.sqrt(1 / aa + 1 / bb + 1 / cc + 1 / dd);
  const lnOr = Math.log(oddsRatio);
  const orCiLower = Math.exp(lnOr - z * seLogOr);
  const orCiUpper = Math.exp(lnOr + z * seLogOr);

  const riskExposed = aa / (aa + bb);
  const riskUnexposed = cc / (cc + dd);
  const riskRatio = riskExposed / riskUnexposed;
  const seLogRr = Math.sqrt(1 / aa - 1 / (aa + bb) + 1 / cc - 1 / (cc + dd));
  const lnRr = Math.log(riskRatio);
  const rrCiLower = Math.exp(lnRr - z * seLogRr);
  const rrCiUpper = Math.exp(lnRr + z * seLogRr);
  if ([oddsRatio, orCiLower, orCiUpper, riskRatio, rrCiLower, rrCiUpper]
    .some((value) => !Number.isFinite(value))) {
    throw new Error('2x2 effect estimates are not finite for the supplied counts.');
  }

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
    continuityCorrected,
  };
}

export interface AttributableRiskResult {
  riskExposed: number;
  riskUnexposed: number;
  riskDifference: number;
  attributableRiskPercent: number;
  populationAttributableRisk: number;
  continuityCorrected: boolean;
}

/**
 * Attributable risk measures from a 2x2 table.
 * - risk difference = risk_exposed - risk_unexposed
 * - AR% (attributable fraction in the exposed) = (RR - 1)/RR
 * - PAR (population attributable risk) = risk_total - risk_unexposed
 */
export function attributableRisk(a: number, b: number, c: number, d: number): AttributableRiskResult {
  const { cells: [aa, bb, cc, dd], continuityCorrected } = correctedTable(a, b, c, d);
  const riskExposed = aa / (aa + bb);
  const riskUnexposed = cc / (cc + dd);
  const riskDifference = riskExposed - riskUnexposed;
  const rr = riskExposed / riskUnexposed;
  const attributableRiskPercent = ((rr - 1) / rr) * 100;
  const total = aa + bb + cc + dd;
  const riskTotal = (aa + cc) / total;
  const populationAttributableRisk = riskTotal - riskUnexposed;
  if ([riskExposed, riskUnexposed, riskDifference, attributableRiskPercent, populationAttributableRisk]
    .some((value) => !Number.isFinite(value))) {
    throw new Error('Attributable-risk estimates are not finite for the supplied counts.');
  }
  return {
    riskExposed,
    riskUnexposed,
    riskDifference,
    attributableRiskPercent,
    populationAttributableRisk,
    continuityCorrected,
  };
}
