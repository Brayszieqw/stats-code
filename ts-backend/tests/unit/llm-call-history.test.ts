import { createHash } from 'node:crypto';

import { describe, expect, it } from 'vitest';
import {
  createLlmCallHistory,
  type LlmProvider,
  type LlmRequest,
} from '@stats-code/server';

function sha(value: string): string {
  return createHash('sha256').update(value, 'utf8').digest('hex');
}

describe('createLlmCallHistory', () => {
  it('records session-scoped hash-only prompt/response provenance', async () => {
    const provider: LlmProvider = {
      providerId: 'openai',
      redactedConfig: () => ({ provider: 'openai', baseUrl: 'https://example.test/v1', model: 'gpt-test' }),
      async *chatStream() {
        yield { type: 'text_delta', text: 'answer-' } as const;
        yield { type: 'text_delta', text: 'body' } as const;
        yield { type: 'done' } as const;
      },
    };
    const history = createLlmCallHistory({ now: () => new Date('2026-07-14T03:00:00Z') });
    const request: LlmRequest = {
      messages: [{ role: 'user', content: 'private prompt' }],
      maxTokens: 7,
      temperature: 0,
    };

    for await (const _event of history.wrap('session-a', provider).chatStream(request)) {
      // Drain the stream so the wrapper records its terminal provenance entry.
    }

    const calls = history.list('session-a');
    expect(calls).toHaveLength(1);
    expect(calls[0]).toMatchObject({
      provider: 'openai',
      model: 'gpt-test',
      request_at_utc: '2026-07-14T03:00:00.000Z',
      response_sha256: sha('answer-body'),
    });
    expect(calls[0]!.prompt_sha256).toBe(sha(JSON.stringify({
      messages: request.messages,
      model: null,
      max_tokens: 7,
      temperature: 0,
    })));
    expect(JSON.stringify(calls)).not.toContain('private prompt');
    expect(history.list('session-b')).toEqual([]);
  });
});
