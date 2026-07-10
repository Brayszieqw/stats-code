// server/conversation/llm-provider.ts — OpenAI-compatible streaming LLM client.
//
// Mirrors the request/response shape from crates/api/src/providers/openai_compat.rs
// and the LlmEvent enum from crates/agent-core/src/traits/llm_provider.rs.
//
// Behavior (Requirement 2):
//  - POST {baseUrl}/chat/completions with {model, messages, stream:true, ...}
//  - Authorization: Bearer {apiKey}
//  - parse streamed `data:` SSE frames → one text_delta per non-empty
//    choices[0].delta.content; emit done on `[DONE]` or body end
//  - retry 5xx/network up to 3 total attempts, exponential backoff (500ms,
//    doubling); 4xx → error immediately, no retry
//  - the API key is never included in any serialized/logged surface (2.8)

export type LlmEvent =
  | { type: 'text_delta'; text: string }
  | { type: 'done' }
  | { type: 'error'; reason: string };

export interface LlmMessage {
  role: 'system' | 'user' | 'assistant';
  content: string;
}

export interface LlmRequest {
  messages: LlmMessage[];
  /** Provider falls back to its configured/default model when omitted. */
  model?: string;
  maxTokens?: number;
  temperature?: number;
}

export interface LlmProvider {
  /** Stream a chat completion as an async iterable of LlmEvents. */
  chatStream(req: LlmRequest): AsyncIterable<LlmEvent>;
  readonly providerId: 'deepseek' | 'openai';
  /** Loggable config view that NEVER includes the API key (Requirement 2.8). */
  redactedConfig(): { provider: 'deepseek' | 'openai'; baseUrl: string; model: string };
}

export interface LlmProviderOptions {
  provider: 'deepseek' | 'openai';
  apiKey: string;
  /** Defaults per provider. */
  baseUrl?: string;
  /** Default model per provider. */
  model?: string;
  /** Injectable transport for tests (Requirement 13.5). */
  fetchImpl?: typeof fetch;
  /** Retry knobs; defaults match Requirement 2.5. */
  maxAttempts?: number; // default 3 total attempts
  initialDelayMs?: number; // default 500, doubles each retry
  /** Injectable sleep for deterministic tests. */
  sleepImpl?: (ms: number) => Promise<void>;
}

/** Default base URLs mirroring the Rust OpenAiCompatConfig defaults. */
export const DEFAULT_BASE_URLS: Record<'deepseek' | 'openai', string> = {
  deepseek: 'https://api.deepseek.com/v1',
  openai: 'https://api.openai.com/v1',
};

/** Default models per provider. */
export const DEFAULT_MODELS: Record<'deepseek' | 'openai', string> = {
  deepseek: 'deepseek-chat',
  openai: 'gpt-4o-mini',
};

const DEFAULT_MAX_ATTEMPTS = 3;
const DEFAULT_INITIAL_DELAY_MS = 500;

function defaultSleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

/**
 * Normalize a provider base URL so `/chat/completions` and `/audio/transcriptions`
 * resolve correctly. Saved configs often omit `/v1` (e.g. `https://api.deepseek.com`).
 */
export function normalizeProviderBaseUrl(
  provider: 'deepseek' | 'openai',
  baseUrl?: string | null,
): string {
  let base =
    baseUrl && baseUrl.trim().length > 0 ? baseUrl.trim() : DEFAULT_BASE_URLS[provider];
  base = base.replace(/\/+$/, '');
  // Strip a trailing /chat/completions if a user pasted the full path.
  base = base.replace(/\/chat\/completions$/i, '');
  // Official hosts without the version segment.
  if (/^https?:\/\/api\.deepseek\.com$/i.test(base)) {
    base = `${base}/v1`;
  } else if (/^https?:\/\/api\.openai\.com$/i.test(base)) {
    base = `${base}/v1`;
  }
  return base;
}

/** Append `/chat/completions` to a base URL, idempotently. */
function chatCompletionsEndpoint(baseUrl: string): string {
  const trimmed = baseUrl.replace(/\/+$/, '');
  return trimmed.endsWith('/chat/completions') ? trimmed : `${trimmed}/chat/completions`;
}

/** A non-retryable signal carrying the reason to surface as an `error` event. */
class NonRetryableError extends Error {}

/** Format fetch/network failures so SSE surfaces a useful reason (ECONNRESET, etc.). */
function formatFetchError(err: unknown): string {
  if (!(err instanceof Error)) return String(err);
  const cause = (err as Error & { cause?: unknown }).cause;
  if (cause && typeof cause === 'object') {
    const c = cause as { code?: string; message?: string };
    if (c.code) return `fetch failed (${c.code})`;
    if (c.message) return `fetch failed (${c.message})`;
  }
  return err.message || 'fetch failed';
}

/** Split an SSE buffer into complete frames, returning [frames, remainder]. */
function splitSseFrames(buffer: string): { frames: string[]; rest: string } {
  const frames: string[] = [];
  let rest = buffer;
  for (;;) {
    let idx = rest.indexOf('\n\n');
    let sepLen = 2;
    const crlfIdx = rest.indexOf('\r\n\r\n');
    if (crlfIdx !== -1 && (idx === -1 || crlfIdx < idx)) {
      idx = crlfIdx;
      sepLen = 4;
    }
    if (idx === -1) break;
    frames.push(rest.slice(0, idx));
    rest = rest.slice(idx + sepLen);
  }
  return { frames, rest };
}

/**
 * Parse one SSE frame into its `data:` payload. Returns:
 *  - { done: true } for `[DONE]`
 *  - { text } for a non-empty delta content
 *  - null for comments / empty / non-content frames
 */
function parseFrame(frame: string): { done: true } | { text: string } | null {
  const trimmed = frame.trim();
  if (trimmed.length === 0) return null;
  const dataLines: string[] = [];
  for (const line of trimmed.split(/\r?\n/)) {
    if (line.startsWith(':')) continue; // SSE comment
    if (line.startsWith('data:')) {
      dataLines.push(line.slice('data:'.length).replace(/^\s/, ''));
    }
  }
  if (dataLines.length === 0) return null;
  const payload = dataLines.join('\n');
  if (payload === '[DONE]') return { done: true };
  let json: unknown;
  try {
    json = JSON.parse(payload);
  } catch {
    return null;
  }
  const content = (json as { choices?: { delta?: { content?: unknown } }[] })?.choices?.[0]?.delta
    ?.content;
  if (typeof content === 'string' && content.length > 0) {
    return { text: content };
  }
  return null;
}

export function createLlmProvider(opts: LlmProviderOptions): LlmProvider {
  const provider = opts.provider;
  const baseUrl = normalizeProviderBaseUrl(provider, opts.baseUrl);
  const defaultModel = opts.model && opts.model.length > 0 ? opts.model : DEFAULT_MODELS[provider];
  const fetchImpl = opts.fetchImpl ?? globalThis.fetch;
  const maxAttempts = opts.maxAttempts ?? DEFAULT_MAX_ATTEMPTS;
  const initialDelayMs = opts.initialDelayMs ?? DEFAULT_INITIAL_DELAY_MS;
  const sleep = opts.sleepImpl ?? defaultSleep;
  const apiKey = opts.apiKey;

  const endpoint = chatCompletionsEndpoint(baseUrl);

  /** Perform one request attempt; returns a Response or throws. */
  async function attempt(req: LlmRequest): Promise<Response> {
    const body = {
      model: req.model && req.model.length > 0 ? req.model : defaultModel,
      messages: req.messages,
      stream: true as const,
      ...(req.maxTokens !== undefined ? { max_tokens: req.maxTokens } : {}),
      ...(req.temperature !== undefined ? { temperature: req.temperature } : {}),
    };
    let res: Response;
    try {
      res = await fetchImpl(endpoint, {
        method: 'POST',
        headers: {
          'content-type': 'application/json',
          authorization: `Bearer ${apiKey}`,
        },
        body: JSON.stringify(body),
      });
    } catch (err) {
      throw new Error(formatFetchError(err));
    }
    if (res.ok) return res;
    // 4xx → non-retryable; 5xx → retryable.
    const errBody = await res.text().catch(() => '');
    const detail = errBody.trim().slice(0, 180);
    const msg = detail
      ? `LLM provider returned ${res.status}: ${detail}`
      : `LLM provider returned ${res.status}`;
    if (res.status >= 400 && res.status < 500) {
      throw new NonRetryableError(msg);
    }
    throw new Error(msg);
  }

  /** Send with retry, returning a successful Response or throwing the final error. */
  async function sendWithRetry(req: LlmRequest): Promise<Response> {
    let lastError: unknown;
    for (let attemptNo = 1; attemptNo <= maxAttempts; attemptNo += 1) {
      try {
        return await attempt(req);
      } catch (err) {
        if (err instanceof NonRetryableError) {
          throw err;
        }
        lastError = err;
        if (attemptNo < maxAttempts) {
          // Backoff 500ms, doubling each retry: 500, 1000, ...
          const delay = initialDelayMs * 2 ** (attemptNo - 1);
          await sleep(delay);
        }
      }
    }
    throw lastError instanceof Error ? lastError : new Error('LLM request failed');
  }

  async function* chatStream(req: LlmRequest): AsyncIterable<LlmEvent> {
    let res: Response;
    try {
      res = await sendWithRetry(req);
    } catch (err) {
      yield { type: 'error', reason: formatFetchError(err) };
      return;
    }

    const bodyStream = res.body;
    if (!bodyStream) {
      // No streaming body — treat as immediate completion.
      yield { type: 'done' };
      return;
    }

    const decoder = new TextDecoder();
    const reader = bodyStream.getReader();
    let buffer = '';
    try {
      for (;;) {
        const { value, done } = await reader.read();
        if (done) break;
        buffer += decoder.decode(value, { stream: true });
        const { frames, rest } = splitSseFrames(buffer);
        buffer = rest;
        for (const frame of frames) {
          const parsed = parseFrame(frame);
          if (parsed === null) continue;
          if ('done' in parsed) {
            yield { type: 'done' };
            return;
          }
          yield { type: 'text_delta', text: parsed.text };
        }
      }
      // Flush any trailing buffered frame.
      const tail = buffer + decoder.decode();
      const { frames } = splitSseFrames(`${tail}\n\n`);
      for (const frame of frames) {
        const parsed = parseFrame(frame);
        if (parsed === null) continue;
        if ('done' in parsed) {
          yield { type: 'done' };
          return;
        }
        yield { type: 'text_delta', text: parsed.text };
      }
    } catch (err) {
      yield { type: 'error', reason: (err as Error).message };
      return;
    } finally {
      reader.releaseLock();
    }
    // Body ended without an explicit [DONE]: emit completion (Requirement 2.3).
    yield { type: 'done' };
  }

  return {
    providerId: provider,
    chatStream,
    redactedConfig() {
      return { provider, baseUrl, model: defaultModel };
    },
  };
}
