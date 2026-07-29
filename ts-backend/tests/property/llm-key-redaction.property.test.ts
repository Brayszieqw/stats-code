// tests/property/llm-key-redaction.property.test.ts — Property 4 (provider surface).
//
// For any API key, the provider's serialized/logged form (redactedConfig,
// JSON.stringify, String()) never contains the key value.
//
// Extended to cover POST /api/llm-config/activate: for arbitrary v2
// multi-provider caches (each with its own arbitrary key), activating a
// cached provider (success path), a stale/rejected cached provider (probe
// -failure path), or an uncached provider (400 path) must never leak any
// cached key into the response body or error message.
//
// Validates: Requirements 2.8, and the activate-endpoint hard requirement
// ("必须扩展到 v2 多 provider 格式 + activate 端点").

import { describe, it, expect } from 'vitest';
import fc from 'fast-check';
import {
  createLlmProvider,
  buildRouter,
  MemSessionStore,
  type AppState,
  type LlmConfig,
  type LlmConfigStore,
  type LlmProbe,
  type LlmProviderId,
} from '@stats-code/server';

describe('Property 4: API key is never exposed (provider surface) (Requirement 2.8)', () => {
  it('redactedConfig and serialized forms exclude the key for arbitrary keys', () => {
    fc.assert(
      fc.property(
        fc.hexaString({ minLength: 1, maxLength: 60 }).map((s) => `SECRETKEY_${s}`),
        fc.constantFrom<'deepseek' | 'qwen' | 'kimi' | 'zhipu' | 'custom'>(
          'deepseek',
          'qwen',
          'kimi',
          'zhipu',
          'custom',
        ),
        (apiKey, provider) => {
          const p = createLlmProvider(
            provider === 'custom' ? { provider, apiKey, baseUrl: 'https://relay.example.com/v1' } : { provider, apiKey },
          );
          const redacted = p.redactedConfig();
          expect(JSON.stringify(redacted)).not.toContain(apiKey);
          expect(Object.values(redacted)).not.toContain(apiKey);
          // The provider object itself must not serialize the key.
          // (chatStream is a function; only redactedConfig/providerId are data.)
          const surface = JSON.stringify({ providerId: p.providerId, redacted });
          expect(surface).not.toContain(apiKey);
        },
      ),
      { numRuns: 100 },
    );
  });
});

/** In-memory v2-style multi-provider cache, mirroring the real store's semantics. */
function multiStore(initial: LlmConfig[]): LlmConfigStore & { active: string | null } {
  const cache = new Map(initial.map((c) => [c.provider, c]));
  return {
    active: null,
    read() {
      return this.active ? (cache.get(this.active) ?? null) : null;
    },
    write(c) {
      cache.set(c.provider, c);
      this.active = c.provider;
    },
    listCached() {
      return [...cache.keys()] as LlmProviderId[];
    },
    readProvider(provider) {
      return cache.get(provider) ?? null;
    },
  };
}

describe('Property 4: API key is never exposed (activate endpoint, v2 multi-provider cache)', () => {
  it('POST /api/llm-config/activate never leaks any cached key, across success/probe-failure/uncached paths', async () => {
    await fc.assert(
      fc.asyncProperty(
        fc.uniqueArray(
          fc.record({
            provider: fc.constantFrom<LlmProviderId>('deepseek', 'qwen', 'kimi', 'zhipu', 'custom'),
            apiKey: fc.hexaString({ minLength: 1, maxLength: 60 }).map((s) => `SECRETKEY_${s}`),
          }),
          { selector: (e) => e.provider, minLength: 1, maxLength: 5 },
        ),
        fc.constantFrom<LlmProviderId>('deepseek', 'qwen', 'kimi', 'zhipu', 'custom'),
        fc.boolean(),
        async (entries, target, probeSucceeds) => {
          const cached: LlmConfig[] = entries.map((e) => ({
            provider: e.provider,
            api_key: e.apiKey,
            base_url: e.provider === 'custom' ? 'https://relay.example.com/v1' : null,
            model: null,
          }));
          const store = multiStore(cached);
          // A real provider failure surfaces the provider's own HTTP error text,
          // never anything derived from the locally-held key — modeled here with
          // a fixed, key-independent message.
          const probe: LlmProbe = {
            probe: async () => {
              if (!probeSucceeds) throw new Error('probe failed: unauthorized (401 from upstream)');
            },
          };
          const app = buildRouter({ state: { sessionStore: new MemSessionStore(), llmConfigStore: store, llmProbe: probe } });

          const res = await app.inject({
            method: 'POST',
            url: '/api/llm-config/activate',
            payload: { provider: target },
          });

          // Whichever path executes (200 success, 422 stale-key probe failure, or
          // 400 no-cached-config), no cached provider's key may appear anywhere
          // in the response body.
          for (const entry of entries) {
            expect(res.body).not.toContain(entry.apiKey);
          }

          const isCached = cached.some((c) => c.provider === target);
          if (!isCached) {
            expect(res.statusCode).toBe(400);
          } else if (probeSucceeds) {
            expect(res.statusCode).toBe(200);
          } else {
            expect(res.statusCode).toBe(422);
          }

          await app.close();
        },
      ),
      { numRuns: 60 },
    );
  });
});

