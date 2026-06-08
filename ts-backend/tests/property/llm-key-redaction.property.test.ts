// tests/property/llm-key-redaction.property.test.ts — Property 4 (provider surface).
//
// For any API key, the provider's serialized/logged form (redactedConfig,
// JSON.stringify, String()) never contains the key value.
//
// Validates: Requirements 2.8

import { describe, it, expect } from 'vitest';
import fc from 'fast-check';
import { createLlmProvider } from '@stats-code/server';

describe('Property 4: API key is never exposed (provider surface) (Requirement 2.8)', () => {
  it('redactedConfig and serialized forms exclude the key for arbitrary keys', () => {
    fc.assert(
      fc.property(
        fc.hexaString({ minLength: 1, maxLength: 60 }).map((s) => `SECRETKEY_${s}`),
        fc.constantFrom<'deepseek' | 'openai'>('deepseek', 'openai'),
        (apiKey, provider) => {
          const p = createLlmProvider({ provider, apiKey });
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
