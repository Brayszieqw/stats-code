import { describe, it, expect, vi } from 'vitest';
import { transcribeAudio, SpeechTranscribeError } from '@stats-code/server';

describe('transcribeAudio', () => {
  it('POSTs multipart to {base}/audio/transcriptions and returns text', async () => {
    const calls: { url: string; init: RequestInit }[] = [];
    const fetchImpl = vi.fn(async (url: string, init?: RequestInit) => {
      calls.push({ url, init: init ?? {} });
      return new Response(JSON.stringify({ text: ' 你好世界 ' }), { status: 200 });
    }) as unknown as typeof fetch;

    const result = await transcribeAudio({
      bytes: new Uint8Array([1, 2, 3]),
      contentType: 'audio/webm',
      config: {
        provider: 'custom',
        api_key: 'sk-test',
        base_url: 'https://relay.example.com/v1',
        model: 'whisper-1',
      },
      fetchImpl,
    });

    expect(calls[0]!.url).toBe('https://relay.example.com/v1/audio/transcriptions');
    expect(result.text).toBe('你好世界');
    expect(result.confidence).toBeGreaterThanOrEqual(0.6);
    expect(result.auto_processed).toBe(true);
    const headers = calls[0]!.init.headers as Record<string, string>;
    expect(headers.authorization).toBe('Bearer sk-test');
  });

  it('normalizes a custom relay host without trailing slash changes', async () => {
    const urls: string[] = [];
    const fetchImpl = vi.fn(async (url: string) => {
      urls.push(url);
      return new Response(JSON.stringify({ text: 'ok' }), { status: 200 });
    }) as unknown as typeof fetch;

    await transcribeAudio({
      bytes: new Uint8Array([1]),
      config: { provider: 'custom', api_key: 'k', base_url: 'https://relay.example.com/v1/', model: null },
      fetchImpl,
    });
    expect(urls[0]).toBe('https://relay.example.com/v1/audio/transcriptions');
  });

  it('maps 404 to a clear Whisper-unavailable error', async () => {
    const fetchImpl = vi.fn(async () => new Response('nope', { status: 404 })) as unknown as typeof fetch;
    await expect(
      transcribeAudio({
        bytes: new Uint8Array([1]),
        config: { provider: 'deepseek', api_key: 'k', base_url: 'https://api.deepseek.com', model: null },
        fetchImpl,
      }),
    ).rejects.toMatchObject({
      name: 'SpeechTranscribeError',
      code: 'LlmUnavailable',
    });
  });

  it('rejects empty api key', async () => {
    await expect(
      transcribeAudio({
        bytes: new Uint8Array([1]),
        config: { provider: 'custom', api_key: '', base_url: null, model: null },
      }),
    ).rejects.toBeInstanceOf(SpeechTranscribeError);
  });
});
