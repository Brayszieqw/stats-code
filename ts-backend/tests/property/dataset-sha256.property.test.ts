// tests/property/dataset-sha256.property.test.ts — Property 7.
//
// For arbitrary parseable CSV uploads, the DatasetSummary.sha256 is the 64-char
// lowercase hex SHA256 over the EXACT raw upload bytes, and readRawById returns
// those exact bytes.
//
// Validates: Requirements 6.2

import { describe, it, expect, afterEach } from 'vitest';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createHash, randomUUID } from 'node:crypto';
import fc from 'fast-check';
import { createFsDatasetStore } from '@stats-code/server';

const tmpDirs: string[] = [];
afterEach(() => {
  for (const d of tmpDirs.splice(0)) rmSync(d, { recursive: true, force: true });
});
function freshRoot(): string {
  const d = mkdtempSync(join(tmpdir(), 'sc-dsp-'));
  tmpDirs.push(d);
  return d;
}

describe('Property 7: dataset SHA256 integrity (Requirement 6.2)', () => {
  it('sha256 matches the exact raw bytes and readRawById round-trips', async () => {
    await fc.assert(
      fc.asyncProperty(
        // A CSV body: a header + arbitrary integer rows over 1-3 columns.
        fc.integer({ min: 1, max: 3 }),
        fc.array(fc.array(fc.integer({ min: -1000, max: 1000 }), { minLength: 1, maxLength: 3 }), {
          minLength: 1,
          maxLength: 8,
        }),
        async (cols, rawRows) => {
          const header = Array.from({ length: cols }, (_, i) => `c${i}`).join(',');
          const rows = rawRows.map((r) =>
            Array.from({ length: cols }, (_, i) => String(r[i] ?? 0)).join(','),
          );
          const csv = `${header}\n${rows.join('\n')}\n`;
          const bytes = new TextEncoder().encode(csv);
          const expected = createHash('sha256').update(bytes).digest('hex');

          const store = createFsDatasetStore({ root: freshRoot() });
          const summary = await store.saveAndParse(randomUUID(), 'd.csv', bytes);
          expect(summary.sha256).toBe(expected);
          expect(summary.sha256).toMatch(/^[0-9a-f]{64}$/);
          const raw = await store.readRawById(summary.dataset_id);
          expect(Buffer.from(raw).equals(Buffer.from(bytes))).toBe(true);
        },
      ),
      { numRuns: 50 },
    );
  });
});
