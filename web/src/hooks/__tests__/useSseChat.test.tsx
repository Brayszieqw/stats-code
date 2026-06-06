/**
 * Tests for `useSseChat` SSE event dispatch — verifies that the hook
 * correctly parses the wire format emitted by `crates/agent-server/src/handlers/message.rs`
 * (Requirement 9.5).
 *
 * Critical regression check: text_delta and interpretation events are
 * wrapped in `{ "text": "..." }` envelopes; the hook must extract `.text`,
 * not treat the JSON object as a string (otherwise users see [object Object]).
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';
import { useSseChat } from '../useSseChat';

// Minimal ReadableStream of SSE bytes
function sseStream(frames: string[]): ReadableStream<Uint8Array> {
  const encoder = new TextEncoder();
  const data = frames.join('');
  let sent = false;
  return new ReadableStream({
    pull(controller) {
      if (!sent) {
        controller.enqueue(encoder.encode(data));
        sent = true;
      } else {
        controller.close();
      }
    },
  });
}

function frame(event: string, data: object | string): string {
  const payload = typeof data === 'string' ? data : JSON.stringify(data);
  return `event: ${event}\ndata: ${payload}\n\n`;
}

describe('useSseChat', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it('extracts text from text_delta envelope (not [object Object])', async () => {
    // Mock fetch to return an SSE stream with the wire format the backend uses.
    const body = sseStream([
      frame('text_delta', { text: '你好' }),
      frame('text_delta', { text: '世界' }),
      frame('done', {}),
    ]);
    vi.stubGlobal(
      'fetch',
      vi.fn(async () =>
        new Response(body, {
          status: 200,
          headers: { 'Content-Type': 'text/event-stream' },
        }),
      ),
    );

    const { result } = renderHook(() => useSseChat('test-session'));

    await act(async () => {
      result.current.sendMessage('hi');
    });

    await waitFor(() => {
      const agent = result.current.messages.find((m) => m.role === 'agent');
      expect(agent?.content).toBe('你好世界');
    });
    // Critical regression check: the bug would render the JSON object as
    // "[object Object][object Object]"
    const agent = result.current.messages.find((m) => m.role === 'agent');
    expect(agent?.content).not.toContain('[object Object]');
  });

  it('extracts text from interpretation envelope', async () => {
    const body = sseStream([
      frame('interpretation', { text: '根据 P 值，结果显著。' }),
      frame('done', {}),
    ]);
    vi.stubGlobal(
      'fetch',
      vi.fn(async () =>
        new Response(body, {
          status: 200,
          headers: { 'Content-Type': 'text/event-stream' },
        }),
      ),
    );

    const { result } = renderHook(() => useSseChat('test-session'));
    await act(async () => {
      result.current.sendMessage('analyze');
    });

    await waitFor(() => {
      const agent = result.current.messages.find((m) => m.role === 'agent');
      expect(agent?.interpretation).toBe('根据 P 值，结果显著。');
    });
  });

  it('carries result.analysis through the skill_result event', async () => {
    const skillResultWithAnalysis = {
      schema_version: '1',
      payload: { coefficients: [] },
      risk_signals: [],
      analysis: {
        algorithm_id: 'logistic',
        dataset_id: 'ds-001',
        dataset_sha256: 'a'.repeat(64),
        columns: [{ name: 'age', inferred_type: 'Numeric', missing_count: 0 }],
        params: { outcome: 'event' },
        run_id: 'run-123',
        run_status: 'completed',
      },
    };
    const body = sseStream([
      frame('skill_result', skillResultWithAnalysis),
      frame('done', {}),
    ]);
    vi.stubGlobal(
      'fetch',
      vi.fn(async () =>
        new Response(body, {
          status: 200,
          headers: { 'Content-Type': 'text/event-stream' },
        }),
      ),
    );

    const { result } = renderHook(() => useSseChat('test-session'));
    await act(async () => {
      result.current.sendMessage('run logistic');
    });

    await waitFor(() => {
      const agent = result.current.messages.find((m) => m.role === 'agent');
      expect(agent?.skillResult?.analysis).toBeDefined();
    });
    const agent = result.current.messages.find((m) => m.role === 'agent');
    expect(agent?.skillResult?.analysis?.algorithm_id).toBe('logistic');
    expect(agent?.skillResult?.analysis?.dataset_sha256).toBe('a'.repeat(64));
    expect(agent?.skillResult?.analysis?.run_id).toBe('run-123');
    expect(agent?.skillResult?.analysis?.run_status).toBe('completed');
    expect(agent?.skillResult?.analysis?.columns).toHaveLength(1);
    expect(agent?.skillResult?.analysis?.params).toEqual({ outcome: 'event' });
  });

  it('carries skill_result without analysis (legacy path)', async () => {
    const skillResultNoAnalysis = {
      schema_version: '1',
      payload: { value: 42 },
      risk_signals: ['PValueAboveAlpha'],
    };
    const body = sseStream([
      frame('skill_result', skillResultNoAnalysis),
      frame('done', {}),
    ]);
    vi.stubGlobal(
      'fetch',
      vi.fn(async () =>
        new Response(body, {
          status: 200,
          headers: { 'Content-Type': 'text/event-stream' },
        }),
      ),
    );

    const { result } = renderHook(() => useSseChat('test-session'));
    await act(async () => {
      result.current.sendMessage('inspect');
    });

    await waitFor(() => {
      const agent = result.current.messages.find((m) => m.role === 'agent');
      expect(agent?.skillResult).toBeDefined();
    });
    const agent = result.current.messages.find((m) => m.role === 'agent');
    expect(agent?.skillResult?.analysis).toBeUndefined();
    expect(agent?.skillResult?.payload).toEqual({ value: 42 });
  });

  it('records error event into state', async () => {
    const body = sseStream([
      frame('error', {
        error_code: 'LlmUnavailable',
        message: 'AI 服务暂时不可用',
      }),
      frame('done', {}),
    ]);
    vi.stubGlobal(
      'fetch',
      vi.fn(async () =>
        new Response(body, {
          status: 200,
          headers: { 'Content-Type': 'text/event-stream' },
        }),
      ),
    );

    const { result } = renderHook(() => useSseChat('test-session'));
    await act(async () => {
      result.current.sendMessage('analyze');
    });

    await waitFor(() => {
      expect(result.current.error?.error_code).toBe('LlmUnavailable');
    });
    expect(result.current.error?.message).toBe('AI 服务暂时不可用');
  });
});
