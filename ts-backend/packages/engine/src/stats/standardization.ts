// stats/standardization.ts — direct & indirect rate standardization (task 7.1).

export interface DirectStandardizationResult {
  /** Crude rate of the study population. */
  crudeRate: number;
  /** Age-adjusted (directly standardized) rate per the standard population. */
  standardizedRate: number;
}

/**
 * Direct standardization. For each stratum: study events, study population,
 * and the standard-population weight (counts or proportions). The standardized
 * rate applies the study stratum-specific rates to the standard population.
 */
export function directStandardization(
  studyEvents: readonly number[],
  studyPopulation: readonly number[],
  standardPopulation: readonly number[],
): DirectStandardizationResult {
  const k = studyEvents.length;
  if (studyPopulation.length !== k || standardPopulation.length !== k) {
    throw new Error('Direct standardization requires aligned strata.');
  }
  const totalStudyEvents = studyEvents.reduce((s, v) => s + v, 0);
  const totalStudyPop = studyPopulation.reduce((s, v) => s + v, 0);
  const totalStandard = standardPopulation.reduce((s, v) => s + v, 0);
  if (totalStudyPop <= 0 || totalStandard <= 0) {
    throw new Error('Direct standardization requires positive populations.');
  }
  const crudeRate = totalStudyEvents / totalStudyPop;

  let expectedInStandard = 0;
  for (let i = 0; i < k; i += 1) {
    const stratumRate = studyPopulation[i]! > 0 ? studyEvents[i]! / studyPopulation[i]! : 0;
    expectedInStandard += stratumRate * standardPopulation[i]!;
  }
  const standardizedRate = expectedInStandard / totalStandard;
  return { crudeRate, standardizedRate };
}

export interface IndirectStandardizationResult {
  observed: number;
  expected: number;
  /** Standardized morbidity/mortality ratio = observed / expected. */
  smr: number;
}

/**
 * Indirect standardization. Applies the standard-population stratum rates to
 * the study population to compute expected events, then SMR = observed/expected.
 */
export function indirectStandardization(
  observed: number,
  studyPopulation: readonly number[],
  standardRates: readonly number[],
): IndirectStandardizationResult {
  const k = studyPopulation.length;
  if (standardRates.length !== k) {
    throw new Error('Indirect standardization requires aligned strata.');
  }
  let expected = 0;
  for (let i = 0; i < k; i += 1) {
    expected += standardRates[i]! * studyPopulation[i]!;
  }
  const smr = expected > 0 ? observed / expected : 0;
  return { observed, expected, smr };
}
