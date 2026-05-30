/**
 * Unit tests for `useSidecar`.
 *
 * Covers:
 *   - `enabled = false` → no fetch, hook stays at `{ loading: false }`.
 *   - `enabled = true` → POSTs once, exposes the snippet.
 *   - Same inputs revisited → cached, no second fetch.
 *   - Different inputs → triggers a new fetch.
 *   - Non-2xx response → `state.error` populated.
 *   - Mid-flight key change → stale response does not clobber the new state.
 *
 * Validates: Requirements 1.3
 */

import { describe, it, expect, vi } from 'vitest';
import { renderHook, waitFor, act } from '@testing-library/react';

import {
  useSidecar,
  type SidecarColumn,
  type SidecarParams,
  type SidecarSnippet,
} from './useSidecar';

// ---------------------------------------------------------------------------
// Sample fixtures
// ---------------------------------------------------------------------------

const SHA = 'a'.repeat(64);

const COLUMNS: SidecarColumn[] = [
  { name: 'group', dtype: 'categorical' },
  { name: 'age', dtype: 'numeric' },
];

/** Build the params bag with sensible defaults; override per test. */
function params(overrides: Partial<SidecarParams> = {}): SidecarParams {
  return {
    algorithmId: 'tableone',
    software: 'R',
    datasetSha256: SHA,
    columns: COLUMNS,
    ...overrides,
  };
}

function snippet(overrides: Partial<SidecarSnippet> = {}): SidecarSnippet {
  return {
    algorithm_id: 'tableone',
    software: 'R',
    coverage_value: 'live',
    text: '# header\nlibrary(tableone)\n',
    sha256_of_dataset: SHA,
    release_version: '0.5.0',
    ...overrides,
  };
}

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });
}

// ---------------------------------------------------------------------------
// enabled = false
// ---------------------------------------------------------------------------

describe('useSidecar — enabled = false', () => {
  it('does not call fetch and stays at { loading: false }', async () => {
    const fetchImpl = vi.fn();

    const { result } = renderHook(() =>
      useSidecar(
        params({ enabled: false }),
        fetchImpl as unknown as typeof fetch,
      ),
    );

    // Allow any pending microtasks to resolve before asserting silence.
    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });

    expect(fetchImpl).not.toHaveBeenCalled();
    expect(result.current.snippet).toBeUndefined();
    expect(result.current.error).toBeUndefined();
  });
});

// ---------------------------------------------------------------------------
// enabled = true → fetch once
// ---------------------------------------------------------------------------

describe('useSidecar — enabled = true', () => {
  it('POSTs the sidecar render request exactly once and exposes the snippet', async () => {
    const expected = snippet();
    const fetchImpl = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = typeof input === 'string' ? input : input.toString();
      // The algorithm id is a path segment; the rest travels in the body.
      expect(url).toBe('/api/sidecar/tableone');
      expect(init?.method).toBe('POST');
      const body = JSON.parse(String(init?.body)) as {
        software: string;
        dataset_sha256: string;
        columns: SidecarColumn[];
      };
      expect(body.software).toBe('R');
      expect(body.dataset_sha256).toBe(SHA);
      expect(body.columns).toEqual(COLUMNS);
      return jsonResponse(expected);
    });

    const { result } = renderHook(() =>
      useSidecar(params(), fetchImpl as unknown as typeof fetch),
    );

    expect(result.current.loading).toBe(true);

    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });

    expect(fetchImpl).toHaveBeenCalledTimes(1);
    expect(result.current.snippet).toEqual(expected);
    expect(result.current.error).toBeUndefined();
  });
});

// ---------------------------------------------------------------------------
// Cache hit on identical key
// ---------------------------------------------------------------------------

describe('useSidecar — cache', () => {
  it('does not refetch when the same inputs are revisited within the same hook instance', async () => {
    const expected = snippet();
    const fetchImpl = vi.fn(async () => jsonResponse(expected));

    const { result, rerender } = renderHook(
      (p: SidecarParams) => useSidecar(p, fetchImpl as unknown as typeof fetch),
      { initialProps: params() },
    );

    await waitFor(() => {
      expect(result.current.snippet).toEqual(expected);
    });
    expect(fetchImpl).toHaveBeenCalledTimes(1);

    // Move to a different software — triggers a new fetch.
    rerender(params({ software: 'Python' }));
    await waitFor(() => {
      expect(fetchImpl).toHaveBeenCalledTimes(2);
    });

    // Now revisit the original inputs — the cache must absorb it without
    // calling fetch a third time.
    rerender(params({ software: 'R' }));
    await waitFor(() => {
      expect(result.current.snippet?.software).toBe('R');
    });
    expect(fetchImpl).toHaveBeenCalledTimes(2);
  });

  it('treats different software as a new fetch', async () => {
    const r = snippet({ software: 'R', text: '# R\n' });
    const py = snippet({ software: 'Python', text: '# Python\n' });

    const fetchImpl = vi.fn(async (_input: RequestInfo | URL, init?: RequestInit) => {
      const body = JSON.parse(String(init?.body)) as { software: string };
      if (body.software === 'Python') return jsonResponse(py);
      return jsonResponse(r);
    });

    const { result, rerender } = renderHook(
      (p: SidecarParams) => useSidecar(p, fetchImpl as unknown as typeof fetch),
      { initialProps: params() },
    );

    await waitFor(() => {
      expect(result.current.snippet?.software).toBe('R');
    });

    rerender(params({ software: 'Python' }));

    await waitFor(() => {
      expect(result.current.snippet?.software).toBe('Python');
    });
    expect(fetchImpl).toHaveBeenCalledTimes(2);
  });
});

// ---------------------------------------------------------------------------
// Error path
// ---------------------------------------------------------------------------

describe('useSidecar — errors', () => {
  it('populates state.error on a non-2xx response', async () => {
    const fetchImpl = vi.fn(async () =>
      new Response('boom', { status: 500 }),
    );

    const { result } = renderHook(() =>
      useSidecar(params(), fetchImpl as unknown as typeof fetch),
    );

    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });

    expect(result.current.error).toBeInstanceOf(Error);
    expect(result.current.error?.message).toContain('500');
    expect(result.current.snippet).toBeUndefined();
  });
});

// ---------------------------------------------------------------------------
// Cancellation: stale response from a prior key must not clobber state
// ---------------------------------------------------------------------------

describe('useSidecar — cancellation', () => {
  it('discards an in-flight response when the inputs change mid-flight', async () => {
    // Hold the first request open until we explicitly resolve it.
    let releaseFirst: ((value: Response) => void) | null = null;
    const firstResponse = snippet({ software: 'R', text: '# stale R\n' });
    const secondResponse = snippet({
      software: 'Python',
      text: '# fresh Python\n',
    });

    const fetchImpl = vi.fn(
      async (_input: RequestInfo | URL, init?: RequestInit) => {
        const body = JSON.parse(String(init?.body)) as { software: string };
        if (body.software === 'R') {
          return new Promise<Response>((resolve, reject) => {
            releaseFirst = resolve;
            const signal = init?.signal;
            if (signal) {
              signal.addEventListener('abort', () => {
                reject(
                  typeof DOMException !== 'undefined'
                    ? new DOMException('aborted', 'AbortError')
                    : Object.assign(new Error('aborted'), {
                        name: 'AbortError',
                      }),
                );
              });
            }
          });
        }
        return jsonResponse(secondResponse);
      },
    );

    const { result, rerender } = renderHook(
      (p: SidecarParams) => useSidecar(p, fetchImpl as unknown as typeof fetch),
      { initialProps: params() },
    );

    expect(result.current.loading).toBe(true);

    // Switch to a different software while the first fetch is still pending.
    rerender(params({ software: 'Python' }));

    await waitFor(() => {
      expect(result.current.snippet?.software).toBe('Python');
    });

    // Now deliver the stale R response. The hook must ignore it.
    await act(async () => {
      releaseFirst?.(jsonResponse(firstResponse));
      // Yield so any microtask scheduled by the ignored response runs.
      await Promise.resolve();
    });

    expect(result.current.snippet?.software).toBe('Python');
    expect(result.current.snippet?.text).toBe('# fresh Python\n');
  });
});
