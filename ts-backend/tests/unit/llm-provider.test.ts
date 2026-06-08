// tests/unit/llm-provider.test.ts — OpenAI-compatible streaming provider (task 3.2).
//
// Exercises the provider against a mock fetchImpl (Requirement 13.5):
//  - chunked `data:` frames → text deltas; `[DONE]` → done
//  - bearer header present; DeepSeek defaults applied
//  - 4xx → error, no retry; 5xx → retry-then-error
//
// _Requirements: 2.1, 2.2, 2.3, 2.4, 2.7, 13.5_

import { describe, it, expect, vi } from 'vitest';
import { createLlmProvider, DEFAULT_BASE_URLS, DEFAULT_MODELS, type LlmEvent } from '@stats-code/server';

/** Build a streaming Response from a list of chunk strings. */
function streamingResponse(chunks: string[], init: ResponseInit = { status: 200 }): Response {
  const encoder = new TextEncoder();
  const stream = new ReadableStream<Uint8Array>({
    start(controller) {
      for (const c of chunks) controller.enqueue(encoder.encode(c));
      controller.close();
    },
  });
  return new Response(stream, init);
}

async function collect(it: AsyncIterable<LlmEvent>): Promise<LlmEvent[]> {
  const out: LlmEvent[] = [];
  for await (const e of it) out.push(e);
  return out;
}

const sseDelta = (content: string) =>
  `data: ${JSON.stringify({ choices: [{ delta: { content } }] })}\n\n`;

describe('createLlmProvider streaming (Requirements 2.1, 2.2, 2.3)', () => {
  it('emits one text_delta per non-empty content then done on [DONE]', async () => {
    const fetchImpl = vi.fn(async () =>
      streamingResponse([sseDelta('Hello'), sseDelta(', world'), 'data: [DONE]\n\n']),
    ) as unknown as typeof fetch;
    const provider = createLlmProvider({ provider: 'openai', apiKey: 'sk-x', fetchImpl });
    const events = await collect(provider.chatStream({ messages: [{ role: 'user', content: 'hi' }] }));
    expect(events).toEqual([
      { type: 'text_delta', text: 'Hello' },
      { type: 'text_delta', text: ', world' },
      { type: 'done' },
    ]);
  });

  it('skips empty delta content', async () => {
    const fetchImpl = vi.fn(async () =>
      streamingResponse([sseDelta(''), sseDelta('A'), 'data: [DONE]\n\n']),
    ) as unknown as typeof fetch;
    const provider = createLlmProvider({ provider: 'openai', apiKey: 'sk-x', fetchImpl });
    const events = await collect(provider.chatStream({ messages: [] }));
    expect(events).toEqual([{ type: 'text_delta', text: 'A' }, { type: 'done' }]);
  });

  it('emits done when the body ends without an explicit [DONE]', async () => {
    const fetchImpl = vi.fn(async () => streamingResponse([sseDelta('only')])) as unknown as typeof fetch;
    const provider = createLlmProvider({ provider: 'openai', apiKey: 'sk-x', fetchImpl });
    const events = await collect(provider.chatStream({ messages: [] }));
    expect(events).toEqual([{ type: 'text_delta', text: 'only' }, { type: 'done' }]);
  });

  it('handles a delta split across two chunks at a frame boundary', async () => {
    const frame = sseDelta('Split');
    const mid = Math.floor(frame.length / 2);
    const fetchImpl = vi.fn(async () =>
      streamingResponse([frame.slice(0, mid), frame.slice(mid), 'data: [DONE]\n\n']),
    ) as unknown as typeof fetch;
    const provider = createLlmProvider({ provider: 'openai', apiKey: 'sk-x', fetchImpl });
    const events = await collect(provider.chatStream({ messages: [] }));
    expect(events).toEqual([{ type: 'text_delta', text: 'Split' }, { type: 'done' }]);
  });
});

describe('request shape and auth (Requirements 2.1, 2.4, 2.7)', () => {
  it('POSTs to {baseUrl}/chat/completions with a bearer header and stream:true', async () => {
    const calls: { url: string; init: RequestInit }[] = [];
    const fetchImpl = vi.fn(async (url: string, init: RequestInit) => {
      calls.push({ url, init });
      return streamingResponse(['data: [DONE]\n\n']);
    }) as unknown as typeof fetch;
    const provider = createLlmProvider({ provider: 'deepseek', apiKey: 'sk-secret', fetchImpl });
    await collect(provider.chatStream({ messages: [{ role: 'user', content: 'q' }] }));
    expect(calls).toHaveLength(1);
    expect(calls[0].url).toBe(`${DEFAULT_BASE_URLS.deepseek}/chat/completions`);
    const headers = calls[0].init.headers as Record<string, string>;
    expect(headers.authorization).toBe('Bearer sk-secret');
    const body = JSON.parse(calls[0].init.body as string);
    expect(body.stream).toBe(true);
    expect(body.model).toBe(DEFAULT_MODELS.deepseek);
    expect(body.messages).toEqual([{ role: 'user', content: 'q' }]);
  });

  it('uses a custom baseUrl and model when provided', async () => {
    const calls: { url: string }[] = [];
    const fetchImpl = vi.fn(async (url: string) => {
      calls.push({ url });
      return streamingResponse(['data: [DONE]\n\n']);
    }) as unknown as typeof fetch;
    const provider = createLlmProvider({
      provider: 'openai',
      apiKey: 'k',
      baseUrl: 'https://proxy.example.com/v1/',
      model: 'custom-model',
      fetchImpl,
    });
    await collect(provider.chatStream({ messages: [] }));
    expect(calls[0].url).toBe('https://proxy.example.com/v1/chat/completions');
  });
});

describe('retry policy (Requirements 2.5, 2.6)', () => {
  it('does NOT retry on a 4xx and emits an error immediately', async () => {
    const fetchImpl = vi.fn(async () => new Response('bad', { status: 401 })) as unknown as typeof fetch;
    const provider = createLlmProvider({ provider: 'openai', apiKey: 'k', fetchImpl });
    const events = await collect(provider.chatStream({ messages: [] }));
    expect((fetchImpl as unknown as { mock: { calls: unknown[] } }).mock.calls).toHaveLength(1);
    expect(events).toHaveLength(1);
    expect(events[0].type).toBe('error');
  });

  it('retries a 5xx up to 3 total attempts then emits an error', async () => {
    const fetchImpl = vi.fn(async () => new Response('boom', { status: 503 })) as unknown as typeof fetch;
    const sleepImpl = vi.fn(async () => undefined);
    const provider = createLlmProvider({ provider: 'openai', apiKey: 'k', fetchImpl, sleepImpl });
    const events = await collect(provider.chatStream({ messages: [] }));
    expect((fetchImpl as unknown as { mock: { calls: unknown[] } }).mock.calls).toHaveLength(3);
    expect(events[events.length - 1].type).toBe('error');
  });

  it('retries a network error then succeeds on a later attempt', async () => {
    let n = 0;
    const fetchImpl = vi.fn(async () => {
      n += 1;
      if (n < 2) throw new Error('ECONNRESET');
      return streamingResponse([sseDelta('ok'), 'data: [DONE]\n\n']);
    }) as unknown as typeof fetch;
    const sleepImpl = vi.fn(async () => undefined);
    const provider = createLlmProvider({ provider: 'openai', apiKey: 'k', fetchImpl, sleepImpl });
    const events = await collect(provider.chatStream({ messages: [] }));
    expect(n).toBe(2);
    expect(events).toEqual([{ type: 'text_delta', text: 'ok' }, { type: 'done' }]);
  });
});
