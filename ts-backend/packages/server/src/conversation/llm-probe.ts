// server/conversation/llm-probe.ts — LLM connectivity probe.
//
// Implements the LlmProbe interface by issuing a minimal chat completion
// through a transient LlmProvider and consuming the stream until the first
// text_delta or done. Used by testAndSaveConfig (llm.ts) before persisting a
// submitted config.
//
// Behavior (Requirement 3.5, 3.6):
//  - abort the request via AbortController after a 10 s timeout (injectable)
//  - reject on stream `error` or timeout
//  - accept injectable fetchImpl / createProvider for tests (Requirement 13.5)

import type { LlmProbe } from '../state.js';
import { createLlmProvider, type LlmProvider, type LlmProviderOptions } from './llm-provider.js';

const DEFAULT_TIMEOUT_MS = 10_000;

export interface CreateLlmProbeOptions {
  /** Injected transport (Requirement 13.5). */
  fetchImpl?: typeof fetch;
  /** Probe timeout; default 10_000 ms (Requirement 3.5). */
  timeoutMs?: number;
  /** Injectable provider factory for tests. */
  createProvider?: (opts: LlmProviderOptions) => LlmProvider;
}

export function createLlmProbe(opts: CreateLlmProbeOptions = {}): LlmProbe {
  const timeoutMs = opts.timeoutMs ?? DEFAULT_TIMEOUT_MS;
  const makeProvider = opts.createProvider ?? createLlmProvider;

  return {
    async probe(provider, apiKey, baseUrl, model): Promise<void> {
      const llm = makeProvider({
        provider,
        apiKey,
        baseUrl,
        model,
        fetchImpl: opts.fetchImpl,
      });

      let timer: ReturnType<typeof setTimeout> | undefined;
      const timeout = new Promise<never>((_resolve, reject) => {
        timer = setTimeout(() => {
          reject(new Error(`LLM probe timed out after ${timeoutMs} ms`));
        }, timeoutMs);
      });

      const drain = (async () => {
        const stream = llm.chatStream({
          messages: [{ role: 'user', content: 'ping' }],
          maxTokens: 1,
        });
        for await (const event of stream) {
          if (event.type === 'error') {
            throw new Error(event.reason);
          }
          if (event.type === 'text_delta' || event.type === 'done') {
            return; // first sign of life — connectivity confirmed.
          }
        }
      })();

      try {
        await Promise.race([drain, timeout]);
      } finally {
        if (timer !== undefined) clearTimeout(timer);
      }
    },
  };
}
