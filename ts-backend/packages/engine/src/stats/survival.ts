// stats/survival.ts — Kaplan-Meier and life-table (Phase 3, task 7.1).
// Transcribed from the Rust survival path.

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

/**
 * Kaplan-Meier survival estimator.
 * `times` and `events` are aligned: events[i]=true means an event (death),
 * false means censored at that time.
 */
export function kaplanMeier(times: readonly number[], events: readonly boolean[]): KaplanMeierResult {
  if (times.length !== events.length) {
    throw new Error('Kaplan-Meier requires aligned times and events.');
  }
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
      greenwoodSum += dEvents / (atRisk * (atRisk - dEvents));
    }
    const stdError = survival * Math.sqrt(greenwoodSum);
    points.push({ time: t, atRisk, events: dEvents, censored: dCensored, survival, stdError });
    if (medianSurvival === null && survival <= 0.5) {
      medianSurvival = t;
    }
    atRisk -= dEvents + dCensored;
  }

  return { points, medianSurvival };
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
