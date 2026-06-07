// stats/diagnostic.ts — diagnostic-test ROC / AUC (Phase 3, task 7.1).
// AUC via the trapezoidal rule over the empirical ROC curve; equivalent to the
// Mann-Whitney U statistic normalized by n_pos*n_neg.

export interface RocPoint {
  threshold: number;
  sensitivity: number; // true positive rate
  oneMinusSpecificity: number; // false positive rate
}

export interface RocResult {
  points: RocPoint[];
  auc: number;
}

/**
 * Empirical ROC curve and AUC.
 * `scores` are predicted values; `labels[i]=true` marks a positive case.
 * Higher scores indicate higher likelihood of the positive class.
 */
export function rocCurve(scores: readonly number[], labels: readonly boolean[]): RocResult {
  if (scores.length !== labels.length) {
    throw new Error('ROC requires aligned scores and labels.');
  }
  const nPos = labels.filter((l) => l).length;
  const nNeg = labels.length - nPos;
  if (nPos === 0 || nNeg === 0) {
    throw new Error('ROC requires at least one positive and one negative case.');
  }

  // Sort by descending score.
  const order = Array.from({ length: scores.length }, (_, i) => i).sort(
    (i, j) => scores[j]! - scores[i]!,
  );

  const points: RocPoint[] = [{ threshold: Number.POSITIVE_INFINITY, sensitivity: 0, oneMinusSpecificity: 0 }];
  let tp = 0;
  let fp = 0;
  let i = 0;
  while (i < order.length) {
    const currentScore = scores[order[i]!]!;
    // Process all ties at this score together.
    while (i < order.length && scores[order[i]!]! === currentScore) {
      if (labels[order[i]!]) tp += 1;
      else fp += 1;
      i += 1;
    }
    points.push({
      threshold: currentScore,
      sensitivity: tp / nPos,
      oneMinusSpecificity: fp / nNeg,
    });
  }

  // Trapezoidal AUC over (FPR, TPR).
  let auc = 0;
  for (let k = 1; k < points.length; k += 1) {
    const x0 = points[k - 1]!.oneMinusSpecificity;
    const x1 = points[k]!.oneMinusSpecificity;
    const y0 = points[k - 1]!.sensitivity;
    const y1 = points[k]!.sensitivity;
    auc += ((x1 - x0) * (y0 + y1)) / 2;
  }

  return { points, auc };
}

/** AUC equivalently computed from the Mann-Whitney U statistic (tie-aware). */
export function aucFromRanks(scores: readonly number[], labels: readonly boolean[]): number {
  const nPos = labels.filter((l) => l).length;
  const nNeg = labels.length - nPos;
  if (nPos === 0 || nNeg === 0) {
    throw new Error('AUC requires at least one positive and one negative case.');
  }
  // average ranks
  const indexed = scores.map((v, i) => ({ v, i }));
  indexed.sort((a, b) => a.v - b.v);
  const ranks = new Array<number>(scores.length).fill(0);
  let i = 0;
  while (i < indexed.length) {
    let j = i + 1;
    while (j < indexed.length && indexed[j]!.v === indexed[i]!.v) j += 1;
    const r = (i + 1 + j) / 2;
    for (let k = i; k < j; k += 1) ranks[indexed[k]!.i] = r;
    i = j;
  }
  let rankSumPos = 0;
  for (let k = 0; k < scores.length; k += 1) {
    if (labels[k]) rankSumPos += ranks[k]!;
  }
  const u = rankSumPos - (nPos * (nPos + 1)) / 2;
  return u / (nPos * nNeg);
}
