/**
 * Unit tests for `useSnapshotExport`.
 *
 * Covers:
 *   - Initial state is `{ loading: false }` with no result / error.
 *   - Happy path (HTTP 200) → `state.result` is populated and the request
 *     hits `POST /api/snapshot/export` with the JSON body the caller passed.
 *   - 409 (`RunNotCompleted`) → `errorCode === "RunNotCompleted"` and
 *     `actualStatus` is threaded through (Requirement 7.8).
 *   - 413 (`PayloadTooLarge`) → `errorCode === "PayloadTooLarge"` plus
 *     `measuredBytes` / `ceilingBytes` (Requirement 7.7).
 *   - Generic 4xx / 5xx → `errorCode` falls back to the body's `error_code`
 *     field (e.g. `"ForbiddenSpawn"`, `"InternalError"`).
 *
 * Validates: Requirements 7.1, 7.7, 7.8
 */

import { describe, it, expect, vi } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';

import { useSnapshotExport } from './useSnapshotExport';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });
}

// ---------------------------------------------------------------------------
// Initial state
// ---------------------------------------------------------------------------

describe('useSnapshotExport — initial state', () => {
  it('starts at { loading: false } with no result or error', () => {
    const fetchImpl = vi.fn();
    const { result } = renderHook(() =>
      useSnapshotExport(fetchImpl as unknown as typeof fetch),
    );

    expect(result.current.state.loading).toBe(false);
    expect(result.current.state.result).toBeUndefined();
    expect(result.current.state.error).toBeUndefined();
    expect(fetchImpl).not.toHaveBeenCalled();
  });
});

// ---------------------------------------------------------------------------
// Happy path
// ---------------------------------------------------------------------------

describe('useSnapshotExport — happy path', () => {
  it('issues POST /api/snapshot/export with the JSON body and exposes the response', async () => {
    const expected = {
      snapshot_path: 'C:/tmp/run-1.zip',
      sha256: 'a'.repeat(64),
    };
    const fetchImpl = vi.fn(
      async (input: RequestInfo | URL, init?: RequestInit) => {
        const url = typeof input === 'string' ? input : input.toString();
        expect(url).toBe('/api/snapshot/export');
        expect(init?.method).toBe('POST');
        expect(
          (init?.headers as Record<string, string> | undefined)?.[
            'Content-Type'
          ],
        ).toBe('application/json');
        expect(typeof init?.body).toBe('string');
        const decoded = JSON.parse(init?.body as string) as Record<
          string,
          unknown
        >;
        expect(decoded).toEqual({
          run_id: 'run-1',
          destination: 'C:/tmp/run-1.zip',
        });
        return jsonResponse(expected);
      },
    );

    const { result } = renderHook(() =>
      useSnapshotExport(fetchImpl as unknown as typeof fetch),
    );

    await act(async () => {
      await result.current.exportSnapshot({
        run_id: 'run-1',
        destination: 'C:/tmp/run-1.zip',
      });
    });

    expect(fetchImpl).toHaveBeenCalledTimes(1);
    expect(result.current.state.loading).toBe(false);
    expect(result.current.state.error).toBeUndefined();
    expect(result.current.state.result).toEqual(expected);
  });
});

// ---------------------------------------------------------------------------
// 409 — RunNotCompleted (Requirement 7.8)
// ---------------------------------------------------------------------------

describe('useSnapshotExport — 409 RunNotCompleted', () => {
  it('decodes actual_status from the body and sets errorCode = "RunNotCompleted"', async () => {
    const fetchImpl = vi.fn(async () =>
      jsonResponse(
        {
          error_code: 'RunNotCompleted',
          message: 'run status is running; snapshot export requires completed',
          actual_status: 'running',
        },
        409,
      ),
    );

    const { result } = renderHook(() =>
      useSnapshotExport(fetchImpl as unknown as typeof fetch),
    );

    await act(async () => {
      await result.current.exportSnapshot({
        run_id: 'run-2',
        destination: 'C:/tmp/run-2.zip',
      });
    });

    expect(result.current.state.loading).toBe(false);
    expect(result.current.state.result).toBeUndefined();
    expect(result.current.state.error).toBeDefined();
    expect(result.current.state.error?.errorCode).toBe('RunNotCompleted');
    expect(result.current.state.error?.actualStatus).toBe('running');
    expect(result.current.state.error?.message).toMatch(/running/);
  });

  it('still sets errorCode = "RunNotCompleted" even when actual_status is missing', async () => {
    const fetchImpl = vi.fn(async () =>
      jsonResponse({ error_code: 'RunNotCompleted', message: 'nope' }, 409),
    );

    const { result } = renderHook(() =>
      useSnapshotExport(fetchImpl as unknown as typeof fetch),
    );

    await act(async () => {
      await result.current.exportSnapshot({
        run_id: 'run-3',
        destination: 'C:/tmp/run-3.zip',
      });
    });

    expect(result.current.state.error?.errorCode).toBe('RunNotCompleted');
    expect(result.current.state.error?.actualStatus).toBeUndefined();
  });
});

// ---------------------------------------------------------------------------
// 413 — PayloadTooLarge (Requirement 7.7)
// ---------------------------------------------------------------------------

describe('useSnapshotExport — 413 PayloadTooLarge', () => {
  it('decodes measured_bytes / ceiling_bytes and sets errorCode = "PayloadTooLarge"', async () => {
    const measured = 60 * 1024 * 1024;
    const ceiling = 50 * 1024 * 1024;
    const fetchImpl = vi.fn(async () =>
      jsonResponse(
        {
          error_code: 'PayloadTooLarge',
          message:
            'artifact payload 62914560 bytes exceeds 52428800 byte ceiling',
          measured_bytes: measured,
          ceiling_bytes: ceiling,
        },
        413,
      ),
    );

    const { result } = renderHook(() =>
      useSnapshotExport(fetchImpl as unknown as typeof fetch),
    );

    await act(async () => {
      await result.current.exportSnapshot({
        run_id: 'run-4',
        destination: 'C:/tmp/run-4.zip',
      });
    });

    expect(result.current.state.loading).toBe(false);
    expect(result.current.state.result).toBeUndefined();
    expect(result.current.state.error?.errorCode).toBe('PayloadTooLarge');
    expect(result.current.state.error?.measuredBytes).toBe(measured);
    expect(result.current.state.error?.ceilingBytes).toBe(ceiling);
    expect(result.current.state.error?.message).toContain('52428800');
  });
});

// ---------------------------------------------------------------------------
// Generic 4xx / 5xx
// ---------------------------------------------------------------------------

describe('useSnapshotExport — generic errors', () => {
  it('uses the body error_code for non-409/413 status codes', async () => {
    const fetchImpl = vi.fn(async () =>
      jsonResponse(
        { error_code: 'ForbiddenSpawn', message: 'Rscript spawn rejected' },
        403,
      ),
    );

    const { result } = renderHook(() =>
      useSnapshotExport(fetchImpl as unknown as typeof fetch),
    );

    await act(async () => {
      await result.current.exportSnapshot({
        run_id: 'run-5',
        destination: 'C:/tmp/run-5.zip',
      });
    });

    expect(result.current.state.error?.errorCode).toBe('ForbiddenSpawn');
    expect(result.current.state.error?.message).toBe('Rscript spawn rejected');
  });

  it('falls back to a synthetic HTTP_<status> token when the body has no error_code', async () => {
    const fetchImpl = vi.fn(async () =>
      new Response('boom', { status: 500 }),
    );

    const { result } = renderHook(() =>
      useSnapshotExport(fetchImpl as unknown as typeof fetch),
    );

    await act(async () => {
      await result.current.exportSnapshot({
        run_id: 'run-6',
        destination: 'C:/tmp/run-6.zip',
      });
    });

    expect(result.current.state.error?.errorCode).toBe('HTTP_500');
    // 中文兜底文案；errorCode 仍保留 HTTP_500 便于排障。
    expect(result.current.state.error?.message).toMatch(/导出失败|服务端|后端/);
  });

  it('reports a NetworkError when fetch itself rejects', async () => {
    const fetchImpl = vi.fn(async () => {
      throw new Error('connection refused');
    });

    const { result } = renderHook(() =>
      useSnapshotExport(fetchImpl as unknown as typeof fetch),
    );

    await act(async () => {
      await result.current.exportSnapshot({
        run_id: 'run-7',
        destination: 'C:/tmp/run-7.zip',
      });
    });

    expect(result.current.state.error?.errorCode).toBe('NetworkError');
    expect(result.current.state.error?.message).toContain('connection refused');
  });
});

// ---------------------------------------------------------------------------
// Loading flag transition (defence in depth around the async path)
// ---------------------------------------------------------------------------

describe('useSnapshotExport — loading flag', () => {
  it('flips loading to true while in flight and back to false on settle', async () => {
    let resolveFetch: ((value: Response) => void) | null = null;
    const fetchImpl = vi.fn(
      () =>
        new Promise<Response>((resolve) => {
          resolveFetch = resolve;
        }),
    );

    const { result } = renderHook(() =>
      useSnapshotExport(fetchImpl as unknown as typeof fetch),
    );

    let pending: Promise<void> | null = null;
    act(() => {
      pending = result.current.exportSnapshot({
        run_id: 'run-8',
        destination: 'C:/tmp/run-8.zip',
      });
    });

    await waitFor(() => {
      expect(result.current.state.loading).toBe(true);
    });

    await act(async () => {
      resolveFetch?.(
        jsonResponse({
          snapshot_path: 'C:/tmp/run-8.zip',
          sha256: 'b'.repeat(64),
        }),
      );
      await pending;
    });

    expect(result.current.state.loading).toBe(false);
    expect(result.current.state.result?.snapshot_path).toBe(
      'C:/tmp/run-8.zip',
    );
  });
});
