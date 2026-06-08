// tests/unit/orchestrator.test.ts — orchestrator decision table + ordering
// (tasks 7.2, 7.4). Uses a mock LLM provider (Requirement 13.5) and a mock
// dataset store / runner where needed.
//
// _Requirements: 7.2, 7.3, 7.4, 7.5, 7.6, 8.2, 13.2_

import { describe, it, expect } from 'vitest';
import { createHash } from 'node:crypto';
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
  type SessionSettings,
} from '@stats-code/server';

/** A mock LLM provider that replays a fixed text for every chatStream call. */
function mockLlm(textByCall: string[] | string): LlmProvider {
  let call = 0;
  const texts = Array.isArray(textByCall) ? textByCall : null;
  return {
    providerId: 'openai',
    redactedConfig: () => ({ provider: 'openai', baseUrl: 'x', model: 'm' }),
    // eslint-disable-next-line @typescript-eslint/require-await
    async *chatStream(): AsyncIterable<LlmEvent> {
      const text = texts ? (texts[Math.min(call, texts.length - 1)] ?? '') : (textByCall as string);
      call += 1;
      yield { type: 'text_delta', text };
      yield { type: 'done' };
    },
  };
}

/** A mock LLM that always errors (LLM unavailable mid-stream). */
function erroringLlm(): LlmProvider {
  return {
    providerId: 'openai',
    redactedConfig: () => ({ provider: 'openai', baseUrl: 'x', model: 'm' }),
    // eslint-disable-next-line @typescript-eslint/require-await
    async *chatStream(): AsyncIterable<LlmEvent> {
      yield { type: 'error', reason: 'unauthorized' };
    },
  };
}

const CSV = 'y,x\n1,1\n2,2\n3,3.1\n4,3.9\n5,5\n';

function fixtureSummary(): DatasetSummary {
  const bytes = new TextEncoder().encode(CSV);
  return {
    dataset_id: 'ds-1',
    file_name: 'data.csv',
    size_bytes: bytes.byteLength,
    encoding: 'Utf8',
    row_count: 5,
    columns: [
      { name: 'y', inferred_type: 'Numeric', missing_count: 0 },
      { name: 'x', inferred_type: 'Numeric', missing_count: 0 },
    ],
    uploaded_at: '2026-01-01T00:00:00Z',
    sha256: createHash('sha256').update(bytes).digest('hex'),
  };
}

function mockDatasetStore(): DatasetStore {
  return {
    saveAndParse: () => Promise.resolve(fixtureSummary()),
    readRawById: () => Promise.resolve(new TextEncoder().encode(CSV)),
  };
}

async function collect(stream: AsyncIterable<AgentEvent>): Promise<AgentEvent[]> {
  const out: AgentEvent[] = [];
  for await (const e of stream) out.push(e);
  return out;
}

const settings = (decision_assistant = false): SessionSettings => ({ decision_assistant });

function buildHarness(llm: LlmProvider) {
  const registry = SkillRegistry.withDefaults();
  const runner = new SkillRunner(registry);
  const sessionStore = new MemSessionStore();
  const datasetStore = mockDatasetStore();
  const orchestrator = createOrchestrator({
    sessionStore,
    datasetStore,
    registry,
    runner,
    llmProviderFactory: () => llm,
  });
  return { orchestrator, sessionStore };
}

describe('orchestrator decision table (Requirements 7.2–7.6)', () => {
  it('zero skills → text_delta then done', async () => {
    const { orchestrator } = buildHarness(
      mockLlm('{"skill_ids":[],"resolved_args":{},"has_query_intent":false,"text_response":"你好"}'),
    );
    const events = await collect(orchestrator.handleMessage('s', { text: 'hi', settings: settings() }));
    expect(events.map((e) => e.type)).toEqual(['text_delta', 'done']);
  });

  it('one skill with missing args → choice_prompt then done', async () => {
    const { orchestrator } = buildHarness(
      mockLlm('{"skill_ids":["model_linear"],"resolved_args":{"outcome":"y"},"has_query_intent":true,"text_response":null}'),
    );
    const events = await collect(orchestrator.handleMessage('s', { text: 'linear', settings: settings() }));
    expect(events.map((e) => e.type)).toEqual(['choice_prompt', 'done']);
  });

  it('multiple skills → choice_prompt then done', async () => {
    const { orchestrator } = buildHarness(
      mockLlm('{"skill_ids":["model_linear","model_logistic"],"resolved_args":{},"has_query_intent":true,"text_response":null}'),
    );
    const events = await collect(orchestrator.handleMessage('s', { text: 'model', settings: settings() }));
    expect(events[0].type).toBe('choice_prompt');
    expect(events.at(-1)?.type).toBe('done');
  });

  it('LLM unavailable → error then done', async () => {
    const { orchestrator } = buildHarness(erroringLlm());
    const events = await collect(orchestrator.handleMessage('s', { text: 'x', settings: settings() }));
    expect(events.map((e) => e.type)).toEqual(['error', 'done']);
  });

  it('LLM not configured (factory null) → error then done', async () => {
    const registry = SkillRegistry.withDefaults();
    const orchestrator = createOrchestrator({
      sessionStore: new MemSessionStore(),
      datasetStore: mockDatasetStore(),
      registry,
      runner: new SkillRunner(registry),
      llmProviderFactory: () => null,
    });
    const events = await collect(orchestrator.handleMessage('s', { text: 'x', settings: settings() }));
    expect(events.map((e) => e.type)).toEqual(['error', 'done']);
  });
});

describe('orchestrator skill dispatch + ordering (Requirements 8.2, 13.2)', () => {
  async function runSingleSkill(decisionAssistant: boolean): Promise<AgentEvent[]> {
    // Intent recognition returns a resolved single skill; the interpretation
    // call returns plain text. Both come from the same mock LLM.
    const llm = mockLlm([
      '{"skill_ids":["model_linear"],"resolved_args":{"outcome":"y","predictors":["x"],"dataset_id":"ds-1"},"has_query_intent":true,"text_response":null}',
      '线性回归结果显示 x 与 y 显著正相关。',
    ]);
    const { orchestrator, sessionStore } = buildHarness(llm);
    // Seed a session whose datasets contain the fixture (so loadDatasetContext finds it).
    const session = await sessionStore.create();
    await sessionStore.appendDataset(session.id, fixtureSummary());
    return collect(
      orchestrator.handleMessage(session.id, { text: '对 y 做线性回归', settings: settings(decisionAssistant) }),
    );
  }

  it('emits skill_call → skill_result → interpretation → done, in order', async () => {
    const events = await runSingleSkill(false);
    expect(events.map((e) => e.type)).toEqual(['skill_call', 'skill_result', 'interpretation', 'done']);
  });

  it('skill_result precedes any interpretation', async () => {
    const events = await runSingleSkill(false);
    const resultIdx = events.findIndex((e) => e.type === 'skill_result');
    const interpIdx = events.findIndex((e) => e.type === 'interpretation');
    expect(resultIdx).toBeGreaterThanOrEqual(0);
    expect(interpIdx).toBeGreaterThan(resultIdx);
    // Exactly one skill_result.
    expect(events.filter((e) => e.type === 'skill_result')).toHaveLength(1);
  });

  it('appends a follow-up choice_prompt when decision_assistant is enabled', async () => {
    const events = await runSingleSkill(true);
    expect(events.map((e) => e.type)).toEqual([
      'skill_call',
      'skill_result',
      'interpretation',
      'choice_prompt',
      'done',
    ]);
  });
});
