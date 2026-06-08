// tests/property/orchestrator-ordering.property.test.ts — Property 3.
//
// For any successful single-skill dispatch, the orchestrator emits exactly one
// skill_result, at least one interpretation strictly AFTER it, and terminates
// with exactly one done. Uses a mock LLM (Requirement 13.5).
//
// Validates: Requirements 8.2, 8.6, 13.2

import { describe, it, expect } from 'vitest';
import { createHash } from 'node:crypto';
import fc from 'fast-check';
import {
  createOrchestrator,
  SkillRegistry,
  SkillRunner,
  MemSessionStore,
  type AgentEvent,
  type DatasetStore,
  type DatasetSummary,
  type LlmEvent,
  type LlmProvider,
} from '@stats-code/server';

function summaryFor(csv: string): DatasetSummary {
  const bytes = new TextEncoder().encode(csv);
  return {
    dataset_id: 'ds-1',
    file_name: 'data.csv',
    size_bytes: bytes.byteLength,
    encoding: 'Utf8',
    row_count: csv.trim().split('\n').length - 1,
    columns: [
      { name: 'y', inferred_type: 'Numeric', missing_count: 0 },
      { name: 'x', inferred_type: 'Numeric', missing_count: 0 },
    ],
    uploaded_at: '2026-01-01T00:00:00Z',
    sha256: createHash('sha256').update(bytes).digest('hex'),
  };
}

function mockLlm(intentJson: string, interpretation: string): LlmProvider {
  let call = 0;
  return {
    providerId: 'openai',
    redactedConfig: () => ({ provider: 'openai', baseUrl: 'x', model: 'm' }),
    // eslint-disable-next-line @typescript-eslint/require-await
    async *chatStream(): AsyncIterable<LlmEvent> {
      const text = call === 0 ? intentJson : interpretation;
      call += 1;
      yield { type: 'text_delta', text };
      yield { type: 'done' };
    },
  };
}

async function collect(stream: AsyncIterable<AgentEvent>): Promise<AgentEvent[]> {
  const out: AgentEvent[] = [];
  for await (const e of stream) out.push(e);
  return out;
}

describe('Property 3: skill_result precedes interpretation (Requirements 8.2, 8.6, 13.2)', () => {
  it('holds for arbitrary linear-regression datasets and decision-assistant flags', async () => {
    await fc.assert(
      fc.asyncProperty(
        fc.array(fc.integer({ min: -30, max: 30 }), { minLength: 4, maxLength: 10 }),
        fc.boolean(),
        async (xs, decisionAssistant) => {
          const csv = `y,x\n${xs.map((x, i) => `${2 * x + (i % 2)},${x}`).join('\n')}\n`;
          const summary = summaryFor(csv);
          const datasetStore: DatasetStore = {
            saveAndParse: () => Promise.resolve(summary),
            readRawById: () => Promise.resolve(new TextEncoder().encode(csv)),
          };
          const registry = SkillRegistry.withDefaults();
          const sessionStore = new MemSessionStore();
          const session = await sessionStore.create();
          await sessionStore.appendDataset(session.id, summary);
          const orchestrator = createOrchestrator({
            sessionStore,
            datasetStore,
            registry,
            runner: new SkillRunner(registry),
            llmProviderFactory: () =>
              mockLlm(
                '{"skill_ids":["model_linear"],"resolved_args":{"outcome":"y","predictors":["x"],"dataset_id":"ds-1"},"has_query_intent":true,"text_response":null}',
                '解读文本。',
              ),
          });

          const events = await collect(
            orchestrator.handleMessage(session.id, {
              text: '线性回归',
              settings: { decision_assistant: decisionAssistant },
            }),
          );
          const types = events.map((e) => e.type);

          // Exactly one skill_result.
          expect(types.filter((t) => t === 'skill_result')).toHaveLength(1);
          // At least one interpretation, strictly after the skill_result.
          const resultIdx = types.indexOf('skill_result');
          const interpIdx = types.indexOf('interpretation');
          expect(interpIdx).toBeGreaterThan(resultIdx);
          // Exactly one terminal done, and it is last.
          expect(types.filter((t) => t === 'done')).toHaveLength(1);
          expect(types.at(-1)).toBe('done');
        },
      ),
      { numRuns: 40 },
    );
  });
});
