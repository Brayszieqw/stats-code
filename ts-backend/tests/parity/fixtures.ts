// tests/parity/fixtures.ts — recorded Reference_Software baseline loader.
//
// The parity suite diffs TS algorithm output against values recorded from the
// Reference_Software (R/SAS/Python/SPSS) and stored under
// crates/stats-code/validation/known_values/<software>/<algorithm>/baseline.json.
// These recordings are the parity oracle: a `live` cell is parity-validatable
// because a baseline exists here. A required-but-missing baseline is itself a
// parity failure (Requirement 2.6).

import { readFileSync, existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
// tests/parity → repo/ts-backend/tests/parity ; baselines live in the Rust crate.
export const KNOWN_VALUES_ROOT = join(
  here,
  '..',
  '..',
  '..',
  'crates',
  'stats-code',
  'validation',
  'known_values',
);

export interface Baseline {
  expected_outputs: Record<string, number>;
  input: {
    dataset_csv: string;
    spec: Record<string, string>;
  };
  input_dataset_sha256: string;
  software: string;
  version: string;
}

/** Load a recorded baseline, or null when the recording is absent. */
export function loadBaseline(software: string, algorithm: string): Baseline | null {
  const path = join(KNOWN_VALUES_ROOT, software, algorithm, 'baseline.json');
  if (!existsSync(path)) {
    return null;
  }
  return JSON.parse(readFileSync(path, 'utf8')) as Baseline;
}

/** Parse a simple headered CSV into rows of string cells (no quoting needed). */
export function parseCsv(text: string): { header: string[]; rows: string[][] } {
  const lines = text.split('\n').filter((l) => l.length > 0);
  const header = lines[0]!.split(',');
  const rows = lines.slice(1).map((l) => l.split(','));
  return { header, rows };
}

/** Extract one numeric column by name. */
export function numericColumn(
  csv: { header: string[]; rows: string[][] },
  column: string,
): number[] {
  const idx = csv.header.indexOf(column);
  if (idx === -1) throw new Error(`column ${column} not found in fixture CSV`);
  return csv.rows.map((r) => Number(r[idx]));
}

/** Split a value column by a grouping column into { group → values }. */
export function groupedColumn(
  csv: { header: string[]; rows: string[][] },
  valueCol: string,
  groupCol: string,
): Map<string, number[]> {
  const vIdx = csv.header.indexOf(valueCol);
  const gIdx = csv.header.indexOf(groupCol);
  if (vIdx === -1 || gIdx === -1) throw new Error('value/group column not found');
  const out = new Map<string, number[]>();
  for (const row of csv.rows) {
    const g = row[gIdx]!;
    const v = Number(row[vIdx]);
    const arr = out.get(g) ?? [];
    arr.push(v);
    out.set(g, arr);
  }
  return out;
}
