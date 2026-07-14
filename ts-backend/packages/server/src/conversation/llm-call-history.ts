import { createHash } from 'node:crypto';

import { snapshot as engineSnapshot } from '@stats-code/engine';

import type { LlmEvent, LlmProvider, LlmRequest } from './llm-provider.js';

export interface LlmCallHistory {
  /** Wrap a provider so completed/failed streams are recorded for one session. */
  wrap(sessionId: string, provider: LlmProvider): LlmProvider;
  /** Return a defensive copy in call order. */
  list(sessionId: string): engineSnapshot.LlmCall[];
}

export interface CreateLlmCallHistoryOptions {
  now?: () => Date;
  /** Bound retained provenance per session; oldest calls are dropped first. */
  maxCallsPerSession?: number;
}

function sha256Text(value: string): string {
  return createHash('sha256').update(value, 'utf8').digest('hex');
}

function promptPayload(request: LlmRequest): string {
  return JSON.stringify({
    messages: request.messages,
    model: request.model ?? null,
    max_tokens: request.maxTokens ?? null,
    temperature: request.temperature ?? null,
  });
}

/** In-memory, hash-only provenance. Raw prompts/responses and API keys are never retained. */
export function createLlmCallHistory(opts: CreateLlmCallHistoryOptions = {}): LlmCallHistory {
  const now = opts.now ?? (() => new Date());
  const maxCalls = opts.maxCallsPerSession ?? 1_000;
  if (!Number.isSafeInteger(maxCalls) || maxCalls < 1) {
    throw new Error('maxCallsPerSession must be a positive safe integer');
  }
  const callsBySession = new Map<string, engineSnapshot.LlmCall[]>();

  function append(sessionId: string, call: engineSnapshot.LlmCall): void {
    const calls = callsBySession.get(sessionId) ?? [];
    calls.push(call);
    if (calls.length > maxCalls) calls.splice(0, calls.length - maxCalls);
    callsBySession.set(sessionId, calls);
  }

  function wrap(sessionId: string, provider: LlmProvider): LlmProvider {
    const config = provider.redactedConfig();
    return {
      providerId: provider.providerId,
      redactedConfig: () => provider.redactedConfig(),
      async *chatStream(request: LlmRequest): AsyncIterable<LlmEvent> {
        const requestedAt = now().toISOString();
        const responseParts: string[] = [];
        try {
          for await (const event of provider.chatStream(request)) {
            if (event.type === 'text_delta') responseParts.push(event.text);
            yield event;
          }
        } finally {
          append(sessionId, {
            provider: config.provider,
            model: request.model && request.model.length > 0 ? request.model : config.model,
            request_at_utc: requestedAt,
            prompt_sha256: sha256Text(promptPayload(request)),
            response_sha256: sha256Text(responseParts.join('')),
          });
        }
      },
    };
  }

  return {
    wrap,
    list(sessionId) {
      return (callsBySession.get(sessionId) ?? []).map((call) => ({ ...call }));
    },
  };
}
