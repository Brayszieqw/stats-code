// tests/property/run-arg-validation.property.test.ts — Property 10.
//
// When RunRequest.args is missing a required parameter for the target skill,
// POST /api/sessions/:sid/run returns 422 SkillInvalidArgs and produces no
// result.
//
// Validates: Requirements 12.4

import { describe, it, expect } from 'vitest';
import fc from 'fast-check';
import {
  buildRouter,
  MemSessionStore,
  SkillRegistry,
  SkillRunner,
  type AppState,
  type DatasetStore,
  type DatasetSummary,
} from '@stats-code/server';

const CSV = 'y,x\n1,1\n2,2\n3,3\n';
const BYTES = new TextEncoder().encode(CSV);

function summary(datasetId: string): DatasetSummary {
  return {
    dataset_id: datasetId,
    file_name: 'data.csv',
    size_bytes: BYTES.byteLength,
    encoding: 'Utf8',
    row_count: 3,
    columns: [
      { name: 'y', inferred_type: 'Numeric', missing_count: 0 },
      { name: 'x', inferred_type: 'Numeric', missing_count: 0 },
    ],
    uploaded_at: '2026-01-01T00:00:00Z',
    sha256: null,
  };
}

function memDatasetStore(): DatasetStore {
  return {
    saveAndParse: async () => summary('unused'),
    readRawById: async () => BYTES,
  };
}

// model_linear still needs its statistical arguments in args. The route owns
// dataset_id at the top level and injects it into args before runner dispatch.
const REQUIRED_ARGS = ['outcome', 'predictors'] as const;
const FULL_ARGS: Record<string, unknown> = {
  outcome: 'y',
  predictors: ['x'],
};

describe('Property 10: run arg validation (Requirement 12.4)', () => {
  it('any args missing a required key → 422 SkillInvalidArgs, no result', async () => {
    await fc.assert(
      fc.asyncProperty(
        // Choose a non-empty subset of required keys to OMIT.
        fc.subarray([...REQUIRED_ARGS], { minLength: 1 }),
        async (omit) => {
          const registry = SkillRegistry.withDefaults();
          const state: AppState = {
            sessionStore: new MemSessionStore(),
            datasetStore: memDatasetStore(),
            skillRegistry: registry,
            skillRunner: new SkillRunner(registry),
          };
          const app = buildRouter({ state });
          const created = (await app.inject({ method: 'POST', url: '/api/sessions' })).json();
          const datasetId = '44444444-4444-4444-8444-444444444444';
          await state.sessionStore.appendDataset(created.id, summary(datasetId));

          const args: Record<string, unknown> = { ...FULL_ARGS, dataset_id: datasetId };
          for (const k of omit) delete args[k];

          const res = await app.inject({
            method: 'POST',
            url: `/api/sessions/${created.id}/run`,
            payload: { skill_id: 'model_linear', dataset_id: datasetId, args },
          });
          expect(res.statusCode).toBe(422);
          expect(res.json().error_code).toBe('SkillInvalidArgs');
          // No SkillResult shape leaked.
          expect(res.json().schema_version).toBeUndefined();
          await app.close();
        },
      ),
      { numRuns: 20 },
    );
  });

  it('top-level dataset_id is enough; args.dataset_id is injected before validation', async () => {
    const registry = SkillRegistry.withDefaults();
    const state: AppState = {
      sessionStore: new MemSessionStore(),
      datasetStore: memDatasetStore(),
      skillRegistry: registry,
      skillRunner: new SkillRunner(registry),
    };
    const app = buildRouter({ state });
    const created = (await app.inject({ method: 'POST', url: '/api/sessions' })).json();
    const datasetId = '44444444-4444-4444-8444-444444444444';
    await state.sessionStore.appendDataset(created.id, summary(datasetId));

    const res = await app.inject({
      method: 'POST',
      url: `/api/sessions/${created.id}/run`,
      payload: { skill_id: 'model_linear', dataset_id: datasetId, args: FULL_ARGS },
    });

    expect(res.statusCode).toBe(200);
    expect(res.json().analysis.params.dataset_id).toBe(datasetId);
    await app.close();
  });
});
