// stats/rank.ts — average-rank assignment with ties (task 5.3).
// Transcribed from math::rank_with_ties.

/** Assign average ranks for ties; returns ranks in the original order. */
export function rankWithTies(values: readonly number[]): number[] {
  if (values.some((value) => !Number.isFinite(value))) {
    throw new Error('Ranking requires finite values.');
  }
  const indexed = values.map((v, i) => ({ v, i }));
  indexed.sort((a, b) => a.v - b.v);
  const ranks = new Array<number>(values.length).fill(0);
  let i = 0;
  while (i < indexed.length) {
    let j = i + 1;
    while (j < indexed.length && indexed[j]!.v === indexed[i]!.v) {
      j += 1;
    }
    const rank = (i + 1 + j) / 2;
    for (let k = i; k < j; k += 1) {
      ranks[indexed[k]!.i] = rank;
    }
    i = j;
  }
  return ranks;
}
