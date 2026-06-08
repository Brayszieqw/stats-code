// tests/property/llm-retry.property.test.ts — Property 5: Bounded LLM retries.
//
// For any sequence of transient (5xx / network) failures, the provider issues
// at most `maxAttempts` (default 3) total fetch attempts before emitting an
// `error` event; a 4xx anywhere short-circuits with exactly one attempt.
//
// Validates: Requirements 2.5, 2.6

import { describe, it, expect } from 'vitest';
import fc from 'fast-check';
import { createLlmProvider, type LlmEvent } from '@stats-code/server';

async function collect(it: AsyncIterable<LlmEvent>): Promise<LlmEvent[]> {
  const out: LlmEvent[] = [];
  for await (const e of it) out.push(e);
  return out;
}

describe('Property 5: bounded LLM retries (Requirements 2.5, 2.6)', () => {
  it('issues at most maxAttempts attempts for transient failures, then errors', async () => {
    await fc.assert(
      fc.asyncProperty(
        // A run of transient failures (5xx status or thrown network error).
        fc.array(fc.constantFrom<'500' | '503' | 'network'>('500', '503', 'network'), {
          minLength: 1,
          maxLength: 8,
        }),
        fc.integer({ min: 1, max: 5 }),
        async (failures, maxAttempts) => {
          let attempts = 0;
          const fetchImpl = (async () => {
            attempts += 1;
            const kind = failures[Math.min(attempts - 1, failures.length - 1)];
            if (kind === 'network') throw new Error('ECONNRESET');
            return new Response('err', { status: Number(kind) });
          }) as unknown as typeof fetch;
          const provider = createLlmProvider({
            provider: 'openai',
            apiKey: 'k',
            fetchImpl,
            maxAttempts,
            sleepImpl: async () => undefined,
          });
          const events = await collect(provider.chatStream({ messages: [] }));
          // Never exceeds the configured attempt ceiling.
          expect(attempts).toBeLessThanOrEqual(maxAttempts);
          // All-transient failures exhaust attempts and end in a single error.
          expect(attempts).toBe(maxAttempts);
          expect(events).toHaveLength(1);
          expect(events[0].type).toBe('error');
        },
      ),
      { numRuns: 60 },
    );
  });

  it('a 4xx short-circuits with exactly one attempt (no retry)', async () => {
    await fc.assert(
      fc.asyncProperty(fc.constantFrom(400, 401, 403, 404, 422, 429), async (status) => {
        let attempts = 0;
        const fetchImpl = (async () => {
          attempts += 1;
          return new Response('bad', { status });
        }) as unknown as typeof fetch;
        const provider = createLlmProvider({
          provider: 'openai',
          apiKey: 'k',
          fetchImpl,
          maxAttempts: 3,
          sleepImpl: async () => undefined,
        });
        const events = await collect(provider.chatStream({ messages: [] }));
        expect(attempts).toBe(1);
        expect(events).toHaveLength(1);
        expect(events[0].type).toBe('error');
      }),
      { numRuns: 30 },
    );
  });
});
