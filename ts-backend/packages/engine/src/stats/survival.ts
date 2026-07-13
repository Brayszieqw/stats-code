// stats/survival.ts — Kaplan-Meier and life-table (Phase 3, task 7.1).
// Transcribed from the Rust survival path.

import { chiSquareP } from '../math/distributions.js';
import { invertMatrix, type Matrix } from '../math/linalg.js';

export interface KaplanMeierPoint {
  time: number;
  atRisk: number;
  events: number;
  censored: number;
  survival: number;
  /** Greenwood standard error of the survival estimate. */
  stdError: number;
}

export interface KaplanMeierResult {
  points: KaplanMeierPoint[];
  /** Median survival time, or null if survival never drops to ≤ 0.5. */
  medianSurvival: number | null;
}

function validateSurvivalInputs(times: readonly number[], events: readonly boolean[]): void {
  if (times.length !== events.length) {
    throw new Error('Survival analysis requires aligned times and events.');
  }
  if (times.some((time) => !Number.isFinite(time) || time < 0)) {
    throw new Error('Survival times must be finite non-negative numbers.');
  }
}

/**
 * Kaplan-Meier survival estimator.
 * `times` and `events` are aligned: events[i]=true means an event (death),
 * false means censored at that time.
 */
export function kaplanMeier(times: readonly number[], events: readonly boolean[]): KaplanMeierResult {
  validateSurvivalInputs(times, events);
  const n = times.length;
  if (n === 0) {
    return { points: [], medianSurvival: null };
  }
  const order = Array.from({ length: n }, (_, i) => i).sort((i, j) => times[i]! - times[j]!);

  // Group by distinct time.
  const distinctTimes: number[] = [];
  const eventsAt = new Map<number, number>();
  const censoredAt = new Map<number, number>();
  for (const idx of order) {
    const t = times[idx]!;
    if (!eventsAt.has(t)) {
      distinctTimes.push(t);
      eventsAt.set(t, 0);
      censoredAt.set(t, 0);
    }
    if (events[idx]) {
      eventsAt.set(t, eventsAt.get(t)! + 1);
    } else {
      censoredAt.set(t, censoredAt.get(t)! + 1);
    }
  }

  const points: KaplanMeierPoint[] = [];
  let atRisk = n;
  let survival = 1;
  let greenwoodSum = 0;
  let medianSurvival: number | null = null;

  for (const t of distinctTimes) {
    const dEvents = eventsAt.get(t)!;
    const dCensored = censoredAt.get(t)!;
    if (dEvents > 0) {
      survival *= 1 - dEvents / atRisk;
      if (atRisk > dEvents) {
        greenwoodSum += dEvents / (atRisk * (atRisk - dEvents));
      }
    }
    const stdError = survival === 0 ? 0 : survival * Math.sqrt(greenwoodSum);
    points.push({ time: t, atRisk, events: dEvents, censored: dCensored, survival, stdError });
    if (medianSurvival === null && survival <= 0.5) {
      medianSurvival = t;
    }
    atRisk -= dEvents + dCensored;
  }

  return { points, medianSurvival };
}

export type LogRankUnavailableReason = 'insufficient_groups' | 'no_events' | 'degenerate_variance';

export interface LogRankGroupSummary {
  group: string;
  n: number;
  events: number;
  censored: number;
  observed: number;
  expected: number;
}

export type LogRankResult = {
  status: 'computed';
  method: 'log_rank';
  statistic: number;
  degreesOfFreedom: number;
  pValue: number;
  groups: LogRankGroupSummary[];
} | {
  status: 'not_computed';
  method: 'log_rank';
  statistic: null;
  degreesOfFreedom: null;
  pValue: null;
  reason: LogRankUnavailableReason;
  groups: LogRankGroupSummary[];
};

/** Standard Mantel–Haenszel log-rank test with the full k-group covariance. */
export function logRankTest(
  times: readonly number[],
  events: readonly boolean[],
  groupValues: readonly string[],
): LogRankResult {
  validateSurvivalInputs(times, events);
  if (times.length !== groupValues.length) {
    throw new Error('Log-rank test requires aligned times, events, and groups.');
  }
  if (groupValues.some((group) => group.trim().length === 0)) {
    throw new Error('Log-rank groups must be non-empty strings.');
  }

  const labels = [...new Set(groupValues)].sort();
  const labelIndex = new Map(labels.map((label, index) => [label, index]));
  const observed = new Array<number>(labels.length).fill(0);
  const expected = new Array<number>(labels.length).fill(0);
  const covariance: Matrix = Array.from({ length: labels.length }, () => new Array(labels.length).fill(0));
  const eventTimes = [...new Set(times.filter((_, index) => events[index]))].sort((a, b) => a - b);

  const summaries = () => labels.map((group, index) => {
    const members = groupValues.map((value, row) => value === group ? row : -1).filter((row) => row >= 0);
    const eventCount = members.filter((row) => events[row]).length;
    return {
      group,
      n: members.length,
      events: eventCount,
      censored: members.length - eventCount,
      observed: observed[index]!,
      expected: expected[index]!,
    };
  });

  if (labels.length < 2) {
    return { status: 'not_computed', method: 'log_rank', statistic: null, degreesOfFreedom: null, pValue: null, reason: 'insufficient_groups', groups: summaries() };
  }
  if (eventTimes.length === 0) {
    return { status: 'not_computed', method: 'log_rank', statistic: null, degreesOfFreedom: null, pValue: null, reason: 'no_events', groups: summaries() };
  }

  for (const time of eventTimes) {
    const atRisk = new Array<number>(labels.length).fill(0);
    const deaths = new Array<number>(labels.length).fill(0);
    for (let row = 0; row < times.length; row += 1) {
      const group = labelIndex.get(groupValues[row]!)!;
      if (times[row]! >= time) atRisk[group]! += 1;
      if (times[row] === time && events[row]) deaths[group]! += 1;
    }
    const totalAtRisk = atRisk.reduce((sum, value) => sum + value, 0);
    const totalDeaths = deaths.reduce((sum, value) => sum + value, 0);
    if (totalAtRisk === 0 || totalDeaths === 0) continue;

    for (let group = 0; group < labels.length; group += 1) {
      observed[group]! += deaths[group]!;
      expected[group]! += totalDeaths * atRisk[group]! / totalAtRisk;
    }
    if (totalAtRisk <= 1) continue;
    const varianceFactor = totalDeaths * (totalAtRisk - totalDeaths) / (totalAtRisk - 1);
    for (let first = 0; first < labels.length; first += 1) {
      const firstFraction = atRisk[first]! / totalAtRisk;
      for (let second = 0; second < labels.length; second += 1) {
        const secondFraction = atRisk[second]! / totalAtRisk;
        covariance[first]![second]! += varianceFactor * (
          first === second
            ? firstFraction * (1 - firstFraction)
            : -firstFraction * secondFraction
        );
      }
    }
  }

  const contrasts = observed.slice(0, -1).map((value, index) => value - expected[index]!);
  const covarianceContrast = covariance.slice(0, -1).map((row) => row.slice(0, -1));
  try {
    const inverse = invertMatrix(covarianceContrast);
    let statistic = 0;
    for (let row = 0; row < contrasts.length; row += 1) {
      for (let column = 0; column < contrasts.length; column += 1) {
        statistic += contrasts[row]! * inverse[row]![column]! * contrasts[column]!;
      }
    }
    if (!Number.isFinite(statistic) || statistic < -1e-10) throw new Error('Invalid log-rank statistic.');
    statistic = Math.max(0, statistic);
    const degreesOfFreedom = labels.length - 1;
    return {
      status: 'computed',
      method: 'log_rank',
      statistic,
      degreesOfFreedom,
      pValue: chiSquareP(statistic, degreesOfFreedom),
      groups: summaries(),
    };
  } catch {
    return { status: 'not_computed', method: 'log_rank', statistic: null, degreesOfFreedom: null, pValue: null, reason: 'degenerate_variance', groups: summaries() };
  }
}

export interface LifeTableInterval {
  intervalStart: number;
  intervalEnd: number;
  atRisk: number;
  events: number;
  censored: number;
  effectiveAtRisk: number;
  conditionalProbDeath: number;
  conditionalProbSurvival: number;
  cumulativeSurvival: number;
}

/**
 * Actuarial life table over fixed intervals. `breaks` defines interval edges
 * (length k+1 for k intervals). Censored individuals contribute half to the
 * effective number at risk (actuarial assumption).
 */
export function lifeTable(
  times: readonly number[],
  events: readonly boolean[],
  breaks: readonly number[],
): LifeTableInterval[] {
  if (times.length !== events.length) {
    throw new Error('Life table requires aligned times and events.');
  }
  if (breaks.length < 2) {
    throw new Error('Life table requires at least two interval edges.');
  }
  const intervals: LifeTableInterval[] = [];
  let atRisk = times.length;
  let cumulativeSurvival = 1;

  for (let k = 0; k < breaks.length - 1; k += 1) {
    const start = breaks[k]!;
    const end = breaks[k + 1]!;
    let events_ = 0;
    let censored = 0;
    for (let i = 0; i < times.length; i += 1) {
      const t = times[i]!;
      if (t >= start && t < end) {
        if (events[i]) events_ += 1;
        else censored += 1;
      }
    }
    const effectiveAtRisk = atRisk - censored / 2;
    const conditionalProbDeath = effectiveAtRisk > 0 ? events_ / effectiveAtRisk : 0;
    const conditionalProbSurvival = 1 - conditionalProbDeath;
    cumulativeSurvival *= conditionalProbSurvival;
    intervals.push({
      intervalStart: start,
      intervalEnd: end,
      atRisk,
      events: events_,
      censored,
      effectiveAtRisk,
      conditionalProbDeath,
      conditionalProbSurvival,
      cumulativeSurvival,
    });
    atRisk -= events_ + censored;
  }
  return intervals;
}
