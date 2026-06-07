// scripts/run-parity.mjs — full Parity_Validator suite runner + CI gate (task 17.1).
//
// Aggregates every per-phase parity test (tests/parity/**) into one run over all
// (algorithm, software, case, metric) combinations and maps the outcome to the
// preserved parity exit codes:
//
//   0  ALL_PASS              — every comparison within tolerance
//   2  FAIL_ROW              — at least one metric exceeded its Parity_Threshold
//   3  UNKNOWN_FILTER        — a requested --methods filter matched no suite
//   4  MISSING_TOLERANCE     — a required tolerance/threshold was unavailable
//   5  MATRIX_CONTRADICTION  — the coverage matrix is internally inconsistent
//
// Usage:
//   node scripts/run-parity.mjs                # full suite
//   node scripts/run-parity.mjs --methods cox,logistic
//
// The CI pipeline invokes this at the Phase 8 cutover; a non-zero exit fails
// the build (Requirements 2.4, 14.3, 14.4, 15.2, 15.3).

import { spawnSync } from 'node:child_process';
import { readdirSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const PARITY_EXIT = {
  ALL_PASS: 0,
  FAIL_ROW: 2,
  UNKNOWN_FILTER: 3,
  MISSING_TOLERANCE: 4,
  MATRIX_CONTRADICTION: 5,
};

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, '..');
const parityDir = resolve(root, 'tests', 'parity');

function parseMethods(argv) {
  const idx = argv.indexOf('--methods');
  if (idx === -1 || idx + 1 >= argv.length) return null;
  return argv[idx + 1].split(',').map((s) => s.trim()).filter(Boolean);
}

function availableSuites() {
  return readdirSync(parityDir)
    .filter((f) => f.endsWith('.parity.test.ts'))
    .map((f) => f.replace('.parity.test.ts', ''));
}

function main() {
  const methods = parseMethods(process.argv.slice(2));
  const suites = availableSuites();

  // Map a requested method/algorithm to its parity suite file stem.
  const ALGO_TO_SUITE = {
    tableone: 'batch-a',
    ttest: 'batch-a',
    anova: 'batch-a',
    nonparametric: 'batch-a',
    correlation: 'batch-a',
    or_rr: 'batch-b',
    attributable_risk: 'batch-b',
    standardization: 'batch-b',
    kaplan_meier: 'batch-b',
    life_table: 'batch-b',
    linear: 'batch-b',
    diagnostic_roc: 'batch-b',
    cox: 'iterative',
    logistic: 'iterative',
    power_single_arm: 'power',
    power_phase2: 'power',
    power_phase3: 'power',
  };

  let targetFiles = ['tests/parity'];
  if (methods && methods.length > 0) {
    const stems = new Set();
    for (const m of methods) {
      const stem = ALGO_TO_SUITE[m] ?? (suites.includes(m) ? m : undefined);
      if (stem) stems.add(stem);
    }
    if (stems.size === 0) {
      console.error(`[parity] no parity suite matches --methods ${methods.join(',')}`);
      process.exit(PARITY_EXIT.UNKNOWN_FILTER);
    }
    targetFiles = [...stems].map((s) => `tests/parity/${s}.parity.test.ts`);
  }

  console.log(`[parity] running suites: ${targetFiles.join(', ')}`);
  // Invoke the local vitest binary via Node directly (avoids npx resolution
  // differences across platforms / shells).
  const vitestBin = resolve(root, 'node_modules', 'vitest', 'vitest.mjs');
  const result = spawnSync(process.execPath, [vitestBin, 'run', ...targetFiles], {
    cwd: root,
    stdio: 'inherit',
    encoding: 'utf8',
  });

  if (result.status === 0) {
    console.log('[parity] ALL_PASS');
    process.exit(PARITY_EXIT.ALL_PASS);
  }
  // vitest exits 1 on test failure; map that to FAIL_ROW (a metric exceeded its
  // threshold or a required reference baseline was unavailable).
  console.error('[parity] FAIL_ROW — at least one parity comparison failed');
  process.exit(PARITY_EXIT.FAIL_ROW);
}

main();
