// tests/property/llm-status-redaction.property.test.ts — Property 4 (status route).
//
// For any persisted config with an arbitrary api_key, the GET /api/llm-status
// response body never contains the key value.
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
} from '@stats-code/server';

function fixedStore(cfg: LlmConfig): LlmConfigStore {
  return { read: () => cfg, write: () => undefined };
}

describe('Property 4: API key is never exposed (status route) (Requirement 3.8)', () => {
  it('GET /api/llm-status omits the api key for arbitrary keys', async () => {
    await fc.assert(
      fc.asyncProperty(
        fc.hexaString({ minLength: 1, maxLength: 60 }).map((s) => `SECRETKEY_${s}`),
        fc.constantFrom<'deepseek' | 'openai'>('deepseek', 'openai'),
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
});
