// tests/integration/conversation-e2e.test.ts — end-to-end conversational path
// (task 7.8). With a mock LLM transport (Requirement 13.5): configure a valid
// LLM, upload a dataset, send a recognized single-skill message, and assert the
// SSE stream emits skill_result → interpretation → done in order. Also confirms
// non-LLM routes work with no persisted config.
//
// _Requirements: 9.2, 10.2, 10.4, 13.1, 13.5_

import { describe, it, expect, afterEach } from 'vitest';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import {
  buildRouter,
  MemSessionStore,
  createFsDatasetStore,
  createOrchestrator,
  createLlmProvider,
  SkillRegistry,
  SkillRunner,
  type AppState,
  type LlmConfig,
  type LlmConfigStore,
  type LlmEvent,
  type LlmProbe,
  type LlmProvider,
} from '@stats-code/server';

const tmpDirs: string[] = [];
afterEach(() => {
  for (const d of tmpDirs.splice(0)) rmSync(d, { recursive: true, force: true });
});
function freshRoot(): string {
  const d = mkdtempSync(join(tmpdir(), 'sc-e2e-'));
  tmpDirs.push(d);
  return d;
}

const b64 = (s: string) => Buffer.from(s, 'utf8').toString('base64');

/** A mock LLM provider: first call → intent JSON, later calls → interpretation. */
function scriptedProvider(): LlmProvider {
  let call = 0;
  return {
    providerId: 'deepseek',
    redactedConfig: () => ({ provider: 'deepseek', baseUrl: 'x', model: 'm' }),
    // eslint-disable-next-line @typescript-eslint/require-await
    async *chatStream(): AsyncIterable<LlmEvent> {
      const intent =
        '{"skill_ids":["model_linear"],"resolved_args":{"outcome":"y","predictors":["x"],"dataset_id":"DATASET_ID"},"has_query_intent":true,"text_response":null}';
      const text = call === 0 ? intent : '线性回归解读。';
      call += 1;
      yield { type: 'text_delta', text };
      yield { type: 'done' };
    },
  };
}

/** In-memory config store + always-ok probe (mock transport). */
function memConfigStore(): LlmConfigStore & { current: LlmConfig | null } {
  return {
    current: null,
    read() {
      return this.current;
    },
    write(c) {
      this.current = c;
    },
  };
}

describe('conversation end-to-end (Requirements 10.2, 10.4, 13.5)', () => {
  it('configure LLM → upload dataset → message streams skill_result → interpretation → done', async () => {
    const sessionStore = new MemSessionStore();
    const datasetStore = createFsDatasetStore({ root: freshRoot() });
    const registry = SkillRegistry.withDefaults();
    const runner = new SkillRunner(registry);
    const llmConfigStore = memConfigStore();
    const llmProbe: LlmProbe = { probe: () => Promise.resolve() };

    // The provider factory re-reads the persisted config per message; we return
    // a scripted mock provider whenever a config is present (Requirement 10.2).
    let datasetId = '';
    const provider = scriptedProvider();
    const llmProviderFactory = (): LlmProvider | null => (llmConfigStore.read() ? provider : null);

    const messageHandler = createOrchestrator({
      sessionStore,
      datasetStore,
      registry,
      runner,
      llmProviderFactory,
    });

    const state: AppState = {
      sessionStore,
      datasetStore,
      llmConfigStore,
      llmProbe,
      messageHandler,
      oauthCapability: { available: false },
    };
    const app = buildRouter({ state });

    // 1. Initially unconfigured.
    const status0 = await app.inject({ method: 'GET', url: '/api/llm-status' });
    expect(status0.json().configured).toBe(false);

    // 2. Configure a valid LLM (probe passes, config persisted).
    const cfgRes = await app.inject({
      method: 'POST',
      url: '/api/llm-config',
      payload: { provider: 'deepseek', api_key: 'sk-test', base_url: null, model: null },
    });
    expect(cfgRes.statusCode).toBe(200);
    const status1 = await app.inject({ method: 'GET', url: '/api/llm-status' });
    expect(status1.json().configured).toBe(true);

    // 3. Create a session and upload a dataset.
    const sid = (await app.inject({ method: 'POST', url: '/api/sessions' })).json().id as string;
    const upload = await app.inject({
      method: 'POST',
      url: `/api/sessions/${sid}/datasets`,
      payload: { filename: 'data.csv', data: b64('y,x\n1,1\n2,2\n3,3.1\n4,3.9\n5,5\n') },
    });
    expect(upload.statusCode).toBe(201);
    datasetId = upload.json().dataset_id as string;
    expect(datasetId).toMatch(/[0-9a-f-]{36}/);

    // 4. Send a recognized single-skill message; assert the SSE stream order.
    const res = await app.inject({
      method: 'POST',
      url: `/api/sessions/${sid}/messages`,
      payload: { text: '对 y 做线性回归' },
    });
    expect(res.statusCode).toBe(200);
    expect(res.headers['content-type']).toContain('text/event-stream');
    const body = res.body;
    const skillResultIdx = body.indexOf('event: skill_result');
    const interpretationIdx = body.indexOf('event: interpretation');
    const doneIdx = body.lastIndexOf('event: done');
    expect(skillResultIdx).toBeGreaterThanOrEqual(0);
    expect(interpretationIdx).toBeGreaterThan(skillResultIdx);
    expect(doneIdx).toBeGreaterThan(interpretationIdx);

    await app.close();
  });

  it('non-LLM routes work with no persisted config; messages emit error+done', async () => {
    const sessionStore = new MemSessionStore();
    const datasetStore = createFsDatasetStore({ root: freshRoot() });
    const registry = SkillRegistry.withDefaults();
    const llmConfigStore = memConfigStore();
    const messageHandler = createOrchestrator({
      sessionStore,
      datasetStore,
      registry,
      runner: new SkillRunner(registry),
      llmProviderFactory: () => (llmConfigStore.read() ? createLlmProvider({ provider: 'deepseek', apiKey: 'x' }) : null),
    });
    const state: AppState = { sessionStore, datasetStore, llmConfigStore, messageHandler };
    const app = buildRouter({ state });

    // Health works; status reports unconfigured.
    expect((await app.inject({ method: 'GET', url: '/api/health' })).statusCode).toBe(200);
    expect((await app.inject({ method: 'GET', url: '/api/llm-status' })).json().configured).toBe(false);

    // A message with no configured LLM streams error then done.
    const sid = (await app.inject({ method: 'POST', url: '/api/sessions' })).json().id as string;
    const res = await app.inject({
      method: 'POST',
      url: `/api/sessions/${sid}/messages`,
      payload: { text: 'hello' },
    });
    expect(res.statusCode).toBe(200);
    expect(res.body).toContain('event: error');
    expect(res.body.trimEnd().endsWith('event: done\ndata: {}')).toBe(true);
    await app.close();
  });
});
