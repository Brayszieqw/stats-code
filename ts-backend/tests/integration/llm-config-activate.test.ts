// tests/integration/llm-config-activate.test.ts — POST /api/llm-config/activate
// (switch the active provider using its cached config, re-probing first).
//
// Cases: cached config + successful re-probe → 200 and becomes active;
// no cached config for the requested provider → 400 NO_CACHED_CONFIG;
// cached config but the re-probe fails → 422 LLM_PROBE_FAILED (existing
// testAndSaveConfig failure path, unchanged).

import { describe, it, expect } from 'vitest';
import {
  buildRouter,
  MemSessionStore,
  type AppState,
  type LlmConfig,
  type LlmConfigStore,
  type LlmProbe,
} from '@stats-code/server';

function makeState(overrides: Partial<AppState> = {}): AppState {
  return { sessionStore: new MemSessionStore(), ...overrides };
}

/** An in-memory multi-provider cache store mirroring the real v2 semantics. */
function multiStore(initial: Record<string, LlmConfig> = {}): LlmConfigStore & {
  cache: Record<string, LlmConfig>;
  active: string | null;
} {
  return {
    cache: { ...initial },
    active: null,
    read() {
      return this.active ? (this.cache[this.active] ?? null) : null;
    },
    write(c) {
      this.cache[c.provider] = c;
      this.active = c.provider;
    },
    listCached() {
      return Object.keys(this.cache) as LlmConfig['provider'][];
    },
    readProvider(provider) {
      return this.cache[provider] ?? null;
    },
  };
}

describe('POST /api/llm-config/activate', () => {
  it('activates a cached provider after a successful re-probe (200)', async () => {
    const store = multiStore({
      kimi: { provider: 'kimi', api_key: 'sk-kimi', base_url: null, model: 'kimi-latest' },
    });
    const probe: LlmProbe = { probe: async () => undefined };
    const app = buildRouter({ state: makeState({ llmConfigStore: store, llmProbe: probe }) });

    const res = await app.inject({
      method: 'POST',
      url: '/api/llm-config/activate',
      payload: { provider: 'kimi' },
    });

    expect(res.statusCode).toBe(200);
    expect(store.active).toBe('kimi');
    expect(store.read()?.provider).toBe('kimi');
    await app.close();
  });

  it('returns 400 NO_CACHED_CONFIG when the provider has no cached credentials', async () => {
    const store = multiStore(); // empty cache
    const probe: LlmProbe = { probe: async () => undefined };
    const app = buildRouter({ state: makeState({ llmConfigStore: store, llmProbe: probe }) });

    const res = await app.inject({
      method: 'POST',
      url: '/api/llm-config/activate',
      payload: { provider: 'zhipu' },
    });

    expect(res.statusCode).toBe(400);
    expect(res.json().error_code).toBe('NO_CACHED_CONFIG');
    expect(store.active).toBeNull();
    await app.close();
  });

  it('returns 422 LLM_PROBE_FAILED when the cached credentials no longer work', async () => {
    const store = multiStore({
      qwen: { provider: 'qwen', api_key: 'sk-stale', base_url: null, model: 'qwen-plus' },
    });
    const probe: LlmProbe = {
      probe: async () => {
        throw new Error('unauthorized');
      },
    };
    const app = buildRouter({ state: makeState({ llmConfigStore: store, llmProbe: probe }) });

    const res = await app.inject({
      method: 'POST',
      url: '/api/llm-config/activate',
      payload: { provider: 'qwen' },
    });

    expect(res.statusCode).toBe(422);
    expect(res.json().error_code).toBe('LLM_PROBE_FAILED');
    // The failed re-probe must not flip the active provider.
    expect(store.active).toBeNull();
    await app.close();
  });

  it('returns 422 for a malformed body (missing provider)', async () => {
    const store = multiStore();
    const probe: LlmProbe = { probe: async () => undefined };
    const app = buildRouter({ state: makeState({ llmConfigStore: store, llmProbe: probe }) });

    const res = await app.inject({ method: 'POST', url: '/api/llm-config/activate', payload: {} });

    expect(res.statusCode).toBe(422);
    await app.close();
  });

  it('returns 500 when the server has no llmConfigStore/llmProbe configured', async () => {
    const app = buildRouter({ state: makeState() });
    const res = await app.inject({
      method: 'POST',
      url: '/api/llm-config/activate',
      payload: { provider: 'deepseek' },
    });
    expect(res.statusCode).toBe(500);
    await app.close();
  });
});
