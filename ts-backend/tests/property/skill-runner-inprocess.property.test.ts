// tests/property/skill-runner-inprocess.property.test.ts — Property 6.
//
// In-process execution, no external runtime. For arbitrary numeric datasets,
// the SkillRunner produces a result entirely in-process: running it INSIDE the
// engine's guardedSpawn scope (which throws on any forbidden runtime spawn)
// never trips the forbidden-spawn guard.
//
// Validates: Requirements 5.1

import { describe, it, expect } from 'vitest';
import { createHash } from 'node:crypto';
import fc from 'fast-check';
import { guardedSpawn, isGuardActive, ForbiddenSpawnError } from '@stats-code/engine';
import {
  SkillRegistry,
  SkillRunner,
  type SkillContext,
  type DatasetSummary,
} from '@stats-code/server';

function ctxFor(csv: string, columnNames: string[]): SkillContext {
  const bytes = new TextEncoder().encode(csv);
  const summary: DatasetSummary = {
    dataset_id: 'ds-1',
    file_name: 'data.csv',
    size_bytes: bytes.byteLength,
    encoding: 'Utf8',
    row_count: csv.trim().split('\n').length - 1,
    columns: columnNames.map((name) => ({ name, inferred_type: 'Numeric' as const, missing_count: 0 })),
    uploaded_at: '2026-01-01T00:00:00Z',
    sha256: createHash('sha256').update(bytes).digest('hex'),
  };
  return { datasetBytes: bytes, datasetSummary: summary };
}

describe('Property 6: in-process execution, no external runtime (Requirement 5.1)', () => {
  const reg = SkillRegistry.withDefaults();
  const runner = new SkillRunner(reg);

  it('linear regression runs inside the forbidden-spawn guard without tripping it', async () => {
    await fc.assert(
      fc.asyncProperty(
        fc.array(
          fc.record({ x: fc.integer({ min: -50, max: 50 }), noise: fc.integer({ min: -2, max: 2 }) }),
          { minLength: 4, maxLength: 12 },
        ),
        async (pts) => {
          const rows = pts.map((p) => `${2 * p.x + p.noise},${p.x}`);
          const csv = `y,x\n${rows.join('\n')}\n`;
          const ctx = ctxFor(csv, ['y', 'x']);

          let forbidden = false;
          // The runner is synchronous CPU work wrapped in a Promise; we execute
          // it inside the guarded scope and confirm no forbidden spawn occurs.
          try {
            await guardedSpawn(async () =>
              runner.run(reg.get('model_linear')!, {
                outcome: 'y',
                predictors: ['x'],
                dataset_id: 'ds-1',
              }, ctx),
            );
          } catch (e) {
            if (e instanceof ForbiddenSpawnError) forbidden = true;
            // Other engine errors (e.g. singular matrix) are acceptable — the
            // property is specifically about NOT spawning an external runtime.
          }
          expect(forbidden).toBe(false);
          // The guard is always restored after the scope.
          expect(isGuardActive()).toBe(false);
        },
      ),
      { numRuns: 40 },
    );
  });
});
