// tests/unit/llm-probe.test.ts — LLM connectivity probe (task 3.8).
//
// Success path resolves; error stream rejects; 10 s timeout rejects (injected
// provider / fake timers).
//
// _Requirements: 3.5, 3.6, 13.5_

import { describe, it, expect, vi, afterEach } from 'vitest';
import { createLlmProbe, type LlmEvent, type LlmProvider } from '@stats-code/server';

function providerEmitting(events: LlmEvent[]): LlmProvider {
  return {
    providerId: 'qwen',
    redactedConfig: () => ({ provider: 'qwen', baseUrl: 'x', model: 'm' }),
    // eslint-disable-next-line @typescript-eslint/require-await
    async *chatStream() {
      for (const e of events) yield e;
    },
  };
}

afterEach(() => {
  vi.useRealTimers();
});

describe('createLlmProbe (Requirements 3.5, 3.6)', () => {
  it('resolves when the stream yields a text_delta', async () => {
    const probe = createLlmProbe({
      createProvider: () => providerEmitting([{ type: 'text_delta', text: 'hi' }, { type: 'done' }]),
    });
    await expect(probe.probe('qwen', 'sk-x')).resolves.toBeUndefined();
  });

  it('resolves when the stream yields done with no deltas', async () => {
    const probe = createLlmProbe({ createProvider: () => providerEmitting([{ type: 'done' }]) });
    await expect(probe.probe('deepseek', 'sk-x')).resolves.toBeUndefined();
  });

  it('rejects when the stream yields an error', async () => {
    const probe = createLlmProbe({
      createProvider: () => providerEmitting([{ type: 'error', reason: 'unauthorized' }]),
    });
    await expect(probe.probe('qwen', 'sk-bad')).rejects.toThrow('unauthorized');
  });

  it('rejects on timeout when the stream never produces an event', async () => {
    vi.useFakeTimers();
    const hangingProvider: LlmProvider = {
      providerId: 'kimi',
      redactedConfig: () => ({ provider: 'kimi', baseUrl: 'x', model: 'm' }),
      // Never yields and never returns until aborted.
      // eslint-disable-next-line @typescript-eslint/require-await, require-yield
      async *chatStream() {
        await new Promise<void>(() => {
          /* hang forever */
        });
      },
    };
    const probe = createLlmProbe({ createProvider: () => hangingProvider, timeoutMs: 10_000 });
    const p = probe.probe('kimi', 'sk-x');
    const assertion = expect(p).rejects.toThrow(/timed out/);
    await vi.advanceTimersByTimeAsync(10_000);
    await assertion;
  });
});
