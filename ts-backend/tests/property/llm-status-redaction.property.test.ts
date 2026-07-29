// tests/property/llm-status-redaction.property.test.ts — Property 4 (status route).
//
// For any persisted config with an arbitrary api_key, the GET /api/llm-status
// response body never contains the key value. Extended to cover the v2
// multi-provider cache: cached_providers must carry only provider ids, and no
// cached provider's key may leak even when several are cached simultaneously.
//
// Validates: Requirements 3.8

import { describe, it, expect } from 'vitest';
import fc from 'fast-check';
import {
  buildRouter,
  MemSessionStore,
  type AppState,
  type LlmConfig,
  type LlmConfigStore,
  type LlmProviderId,
} from '@stats-code/server';

function fixedStore(cfg: LlmConfig): LlmConfigStore {
  return {
    read: () => cfg,
    write: () => undefined,
    listCached: () => [cfg.provider],
    readProvider: () => cfg,
  };
}

/** v2-style multi-provider cache fixture: several cached entries, one active. */
function multiProviderStore(entries: LlmConfig[], active: LlmProviderId): LlmConfigStore {
  const cache = new Map(entries.map((e) => [e.provider, e]));
  return {
    read: () => cache.get(active) ?? null,
    write: () => undefined,
    listCached: () => [...cache.keys()],
    readProvider: (provider) => cache.get(provider) ?? null,
  };
}

describe('Property 4: API key is never exposed (status route) (Requirement 3.8)', () => {
  it('GET /api/llm-status omits the api key for arbitrary keys', async () => {
    await fc.assert(
      fc.asyncProperty(
        fc.hexaString({ minLength: 1, maxLength: 60 }).map((s) => `SECRETKEY_${s}`),
        fc.constantFrom<'deepseek' | 'qwen' | 'kimi' | 'zhipu' | 'custom'>(
          'deepseek',
          'qwen',
          'kimi',
          'zhipu',
          'custom',
        ),
        async (apiKey, provider) => {
          const state: AppState = {
            sessionStore: new MemSessionStore(),
            llmConfigStore: fixedStore({ provider, api_key: apiKey, base_url: null, model: null }),
          };
          const app = buildRouter({ state });
          const res = await app.inject({ method: 'GET', url: '/api/llm-status' });
          expect(res.statusCode).toBe(200);
          expect(res.body).not.toContain(apiKey);
          expect(res.json().configured).toBe(true);
          await app.close();
        },
      ),
      { numRuns: 40 },
    );
  });

  it('GET /api/llm-status cached_providers carries only provider ids, never any cached key (v2 multi-provider)', async () => {
    await fc.assert(
      fc.asyncProperty(
        fc.uniqueArray(
          fc.record({
            provider: fc.constantFrom<LlmProviderId>('deepseek', 'qwen', 'kimi', 'zhipu', 'custom'),
            apiKey: fc.hexaString({ minLength: 1, maxLength: 60 }).map((s) => `SECRETKEY_${s}`),
          }),
          { selector: (e) => e.provider, minLength: 1, maxLength: 5 },
        ),
        async (entries) => {
          const configs: LlmConfig[] = entries.map((e) => ({
            provider: e.provider,
            api_key: e.apiKey,
            base_url: null,
            model: null,
          }));
          const active = configs[0]!.provider;
          const state: AppState = {
            sessionStore: new MemSessionStore(),
            llmConfigStore: multiProviderStore(configs, active),
          };
          const app = buildRouter({ state });
          const res = await app.inject({ method: 'GET', url: '/api/llm-status' });
          expect(res.statusCode).toBe(200);
          // No fuzzed key from any cached provider — not just the active one — may
          // appear anywhere in the serialized response body.
          for (const entry of entries) {
            expect(res.body).not.toContain(entry.apiKey);
          }
          const json = res.json() as { configured: boolean; cached_providers: string[] };
          expect(json.configured).toBe(true);
          expect([...json.cached_providers].sort()).toEqual(
            configs.map((c) => c.provider).sort(),
          );
          await app.close();
        },
      ),
      { numRuns: 60 },
    );
  });
});

