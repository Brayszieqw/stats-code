// tests/unit/skill-runner.test.ts — in-process SkillRunner (task 5.9).
//
// In-process invocation of linear/logistic/cox/kaplan_meier against a fixture
// dataset returns a SkillResult with analysis metadata; missing arg →
// invalid_args; thrown engine error → execution_failed (≤ 2048 chars).
//
// _Requirements: 5.2, 5.3, 5.5, 5.6_

import { describe, it, expect } from 'vitest';
import { createHash } from 'node:crypto';
import {
  SkillRegistry,
  SkillRunner,
  SkillRunErrorException,
  type SkillContext,
  type DatasetSummary,
} from '@stats-code/server';

function ctxFor(csv: string, columns: DatasetSummary['columns']): SkillContext {
  const bytes = new TextEncoder().encode(csv);
  const summary: DatasetSummary = {
    dataset_id: 'ds-1',
    file_name: 'data.csv',
    size_bytes: bytes.byteLength,
    encoding: 'Utf8',
    row_count: csv.trim().split('\n').length - 1,
    columns,
    uploaded_at: '2026-01-01T00:00:00Z',
    sha256: createHash('sha256').update(bytes).digest('hex'),
  };
  return { datasetBytes: bytes, datasetSummary: summary };
}

const num = (name: string) => ({ name, inferred_type: 'Numeric' as const, missing_count: 0 });

describe('SkillRunner in-process execution (Requirements 5.2, 5.3, 5.5, 5.6)', () => {
  const reg = SkillRegistry.withDefaults();
  const runner = new SkillRunner(reg);

  it('runs linear regression and attaches analysis metadata', async () => {
    const ctx = ctxFor('y,x\n1,1\n2,2\n3,3.1\n4,3.9\n5,5\n', [num('y'), num('x')]);
    const result = await runner.run(reg.get('model_linear')!, {
      outcome: 'y',
      predictors: ['x'],
      dataset_id: 'ds-1',
    }, ctx);
    expect(result.schema_version).toBe('1.0');
    const payload = result.payload as { r_squared: number };
    expect(payload.r_squared).toBeGreaterThan(0.9);
    expect(result.analysis).not.toBeNull();
    expect(result.analysis?.algorithm_id).toBe('linear');
    expect(result.analysis?.dataset_id).toBe('ds-1');
    expect(result.analysis?.dataset_sha256).toBe(ctx.datasetSummary.sha256);
  });

  it('runs logistic regression', async () => {
    const ctx = ctxFor('y,x\n0,1\n0,2\n1,5\n1,6\n0,2\n1,7\n', [num('y'), num('x')]);
    const result = await runner.run(reg.get('model_logistic')!, {
      outcome: 'y',
      predictors: ['x'],
      dataset_id: 'ds-1',
    }, ctx);
    const payload = result.payload as { odds_ratios: number[] };
    expect(Array.isArray(payload.odds_ratios)).toBe(true);
    expect(result.analysis?.algorithm_id).toBe('logistic');
  });

  it('runs cox regression', async () => {
    const ctx = ctxFor('t,e,x\n5,1,1\n6,0,2\n7,1,1\n8,1,3\n3,1,2\n9,0,1\n', [num('t'), num('e'), num('x')]);
    const result = await runner.run(reg.get('model_cox')!, {
      time: 't',
      event: 'e',
      predictors: ['x'],
      dataset_id: 'ds-1',
    }, ctx);
    const payload = result.payload as { hazard_ratios: number[] };
    expect(Array.isArray(payload.hazard_ratios)).toBe(true);
    expect(result.analysis?.algorithm_id).toBe('cox');
  });

  it('runs kaplan-meier survival', async () => {
    const ctx = ctxFor('t,e\n5,1\n6,0\n7,1\n8,1\n3,1\n', [num('t'), num('e')]);
    const result = await runner.run(reg.get('survival_km')!, {
      time: 't',
      event: 'e',
      dataset_id: 'ds-1',
    }, ctx);
    const payload = result.payload as { survival_table: unknown[] };
    expect(Array.isArray(payload.survival_table)).toBe(true);
    expect(result.analysis?.algorithm_id).toBe('kaplan_meier');
  });

  it('rejects with invalid_args when a required argument is missing', async () => {
    const ctx = ctxFor('y,x\n1,1\n2,2\n', [num('y'), num('x')]);
    await expect(
      runner.run(reg.get('model_linear')!, { outcome: 'y', dataset_id: 'ds-1' }, ctx),
    ).rejects.toMatchObject({ detail: { kind: 'invalid_args', missing: ['predictors'] } });
  });

  it('maps a thrown engine error to execution_failed with a bounded excerpt', async () => {
    const ctx = ctxFor('y,x\n1,1\n2,2\n', [num('y'), num('x')]);
    try {
      await runner.run(reg.get('model_linear')!, {
        outcome: 'missing_col',
        predictors: ['x'],
        dataset_id: 'ds-1',
      }, ctx);
      expect.unreachable('should have thrown');
    } catch (err) {
      expect(err).toBeInstanceOf(SkillRunErrorException);
      const detail = (err as SkillRunErrorException).detail;
      expect(detail.kind).toBe('execution_failed');
      if (detail.kind === 'execution_failed') {
        expect(detail.diagnosticExcerpt.length).toBeLessThanOrEqual(2048);
        expect(detail.diagnosticExcerpt).toContain('column not found');
      }
    }
  });

  it('runs native inspect skill without an algorithm mapping', async () => {
    const ctx = ctxFor('a,b\n1,2\n', [num('a'), num('b')]);
    const result = await runner.run(reg.get('inspect')!, { dataset_id: 'ds-1' }, ctx);
    const payload = result.payload as { row_count: number };
    expect(payload.row_count).toBe(1);
    expect(result.analysis).toBeNull();
  });
});
