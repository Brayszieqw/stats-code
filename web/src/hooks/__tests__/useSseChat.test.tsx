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

/**
 * Like `sseStream`, but the underlying ReadableStream never closes after
 * delivering its one chunk. Used to simulate a still-open SSE connection so
 * `status` can be observed settled on 'streaming' instead of racing to
 * 'idle' via a synchronous done-event cascade (no artificial delay separates
 * the two reads in the plain `sseStream` helper).
 */
function openSseStream(frames: string[]): ReadableStream<Uint8Array> {
  const encoder = new TextEncoder();
  const data = frames.join('');
  let sent = false;
  return new ReadableStream({
    pull(controller) {
      if (!sent) {
        controller.enqueue(encoder.encode(data));
        sent = true;
      }
      // Intentionally no controller.close(): the connection stays open.
    },
  });
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

  /**
   * R14.4 恢复路径：3 秒内没有首包会先置一个「网络连接异常」的推测错误，
   * 但流随后成功时那条横幅必须被回收——否则用户在正常出结果的界面上一直
   * 看到红色报错（真机审计缺陷①）。
   */
  it('retracts the 3s slow-connection false alarm once the stream actually starts', async () => {
    vi.useFakeTimers();
    let releaseResponse: (() => void) | null = null;
    const gate = new Promise<void>((resolve) => {
      releaseResponse = resolve;
    });
    const body = openSseStream([frame('text_delta', { text: '结果' })]);
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => {
        await gate;
        return new Response(body, {
          status: 200,
          headers: { 'Content-Type': 'text/event-stream' },
        });
      }),
    );

    try {
      const { result } = renderHook(() => useSseChat('test-session'));
      act(() => {
        result.current.sendMessage('慢连接');
      });

      // 首包未到，3 秒定时器把状态推成 error。
      await act(async () => {
        await vi.advanceTimersByTimeAsync(3100);
      });
      expect(result.current.status).toBe('error');
      expect(result.current.error?.error_code).toBe('LlmUnavailable');

      // 首包到达：假警报必须撤回。
      await act(async () => {
        releaseResponse?.();
        await vi.advanceTimersByTimeAsync(50);
      });
      expect(result.current.error).toBeNull();
      expect(result.current.status).toBe('streaming');
    } finally {
      vi.useRealTimers();
    }
  });

  it('keeps a server-pushed error event instead of auto-clearing it', async () => {
    const body = sseStream([
      frame('error', { error_code: 'ResearchProtocolRequired', message: '必须先审批研究协议' }),
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

    // 服务端明确推来的错误不是超时推测，流正常结束也不得被清掉。
    await waitFor(() => {
      expect(result.current.error?.error_code).toBe('ResearchProtocolRequired');
    });
    expect(result.current.error?.message).toBe('必须先审批研究协议');
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

  it('finalizes skill_call bubble when skill fails', async () => {
    const body = sseStream([
      frame('skill_call', { skill_id: 'inspect', args: {} }),
      frame('error', {
        error_code: 'ResearchProtocolRequired',
        message: '必须先审批研究协议',
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
      result.current.sendMessage('描述变量');
    });

    await waitFor(() => {
      expect(result.current.error?.error_code).toBe('ResearchProtocolRequired');
    });
    const agent = result.current.messages.find((m) => m.role === 'agent');
    expect(agent?.content).toMatch(/\[失败:/);
    expect(agent?.content).not.toMatch(/\[正在执行:/);
  });
});
