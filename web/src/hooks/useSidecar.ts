/**
 * Lazy fetcher for the Equivalent Code Sidecar snippet of one
 * `(algorithm, software)` cell.
 *
 * The hook is invoked once per active sidecar tab in `<EquivalentCodeSidecar>`.
 * It must:
 *
 *   - issue at most one HTTP request per `(algorithmId, software, runId)` key,
 *     so that re-mounting or revisiting an already-fetched tab does not refetch
 *     (cache lives in a `useRef<Map>` so it survives re-renders without
 *     triggering them);
 *   - skip the network entirely when `enabled === false` (the parent passes
 *     `enabled = isActiveTab` so background tabs stay quiet);
 *   - cancel in-flight requests with `AbortController` when the key changes
 *     mid-fetch, and refuse to clobber the new state with a stale response;
 *   - surface non-2xx HTTP responses as `Error` instances on `state.error`,
 *     not as thrown exceptions.
 *
 * The DTO shape mirrors `crates/api/src/sidecar.rs::SidecarSnippetDto`. Field
 * names match the Rust serde rename rules byte-for-byte (`coverage_value`,
 * `sha256_of_dataset`, `release_version`).
 *
 * Validates: Requirements 1.3
 */

import { useEffect, useRef, useState } from 'react';

import type {
  CoverageState,
  ReferenceSoftware,
} from '../lib/coverageMatrix';

// ---------------------------------------------------------------------------
// Wire types
// ---------------------------------------------------------------------------

/**
 * JSON shape of `GET /api/sidecar/{algorithm_id}?software=...&run_id=...`.
 *
 * Mirrors `crates/api/src/sidecar.rs::SidecarSnippetDto`. The `text` field is
 * absent (Rust serde `skip_serializing_if = "Option::is_none"`) when
 * `coverage_value === 'none'`.
 */
export interface SidecarSnippet {
  algorithm_id: string;
  software: ReferenceSoftware;
  coverage_value: CoverageState;
  /** UTF-8 snippet body with LF line endings. Absent for `coverage_value = 'none'`. */
  text?: string;
  /** 64-character lowercase hexadecimal SHA256 of the input dataset. */
  sha256_of_dataset: string;
  /** Stats Code release version that emitted this snippet. */
  release_version: string;
}

// ---------------------------------------------------------------------------
// Hook surface
// ---------------------------------------------------------------------------

export interface SidecarParams {
  algorithmId: string;
  software: ReferenceSoftware;
  runId: string;
  /**
   * When `false`, the hook is dormant: no fetch is started, no cache is
   * touched, and the returned state is `{ loading: false }`. Defaults to
   * `true` so callers that always want to fetch can omit the prop.
   */
  enabled?: boolean;
}

export interface SidecarState {
  snippet?: SidecarSnippet;
  loading: boolean;
  error?: Error;
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

function cacheKey(p: SidecarParams): string {
  return `${p.algorithmId}::${p.software}::${p.runId}`;
}

function buildUrl(p: SidecarParams): string {
  // `algorithmId` is a path segment, `software` and `run_id` are query params.
  // `encodeURIComponent` keeps the contract stable even if a future algorithm
  // id ever contains a reserved character.
  const path = `/api/sidecar/${encodeURIComponent(p.algorithmId)}`;
  const query = new URLSearchParams({
    software: p.software,
    run_id: p.runId,
  });
  return `${path}?${query.toString()}`;
}

// ---------------------------------------------------------------------------
// Hook
// ---------------------------------------------------------------------------

/**
 * Fetch one sidecar snippet on demand.
 *
 * `fetchImpl` is injectable so unit tests can pass a stub `fetch`; in
 * production callers omit it to use the global.
 */
export function useSidecar(
  params: SidecarParams,
  fetchImpl: typeof fetch = fetch,
): SidecarState {
  const { algorithmId, software, runId, enabled = true } = params;

  // Cache lives in a ref so revisiting a tab is a synchronous lookup that
  // does not trigger a re-render. The cache key composes all three inputs;
  // the design's "cache by (algorithm, software)" wording is sharpened here
  // to also include `run_id` because the snippet's dataset SHA and release
  // version legitimately differ across runs.
  const cacheRef = useRef<Map<string, SidecarSnippet>>(new Map());

  const [state, setState] = useState<SidecarState>(() => {
    if (enabled === false) {
      return { loading: false };
    }
    const cached = cacheRef.current.get(cacheKey(params));
    if (cached !== undefined) {
      return { snippet: cached, loading: false };
    }
    return { loading: true };
  });

  useEffect(() => {
    if (enabled === false) {
      // Disabled tab: drop any prior loading / error state and stay quiet.
      setState({ loading: false });
      return;
    }

    const key = `${algorithmId}::${software}::${runId}`;
    const cached = cacheRef.current.get(key);
    if (cached !== undefined) {
      setState({ snippet: cached, loading: false });
      return;
    }

    // Mark this fetch as the current one. If the key changes before the
    // promise settles, `cancelled` flips to `true` and the response is
    // discarded so it cannot clobber the new state.
    let cancelled = false;
    const controller = new AbortController();

    setState({ loading: true });

    fetchImpl(buildUrl({ algorithmId, software, runId, enabled: true }), {
      signal: controller.signal,
    })
      .then(async (res) => {
        if (cancelled) return;
        if (!res.ok) {
          throw new Error(`sidecar HTTP ${res.status}`);
        }
        const body = (await res.json()) as SidecarSnippet;
        if (cancelled) return;
        cacheRef.current.set(key, body);
        setState({ snippet: body, loading: false });
      })
      .catch((err: unknown) => {
        if (cancelled) return;
        // AbortError surfaces as a DOMException whose `name` is 'AbortError'.
        // Treat it as a silent cancellation rather than a user-visible error.
        if (
          err instanceof DOMException &&
          err.name === 'AbortError'
        ) {
          return;
        }
        const error = err instanceof Error ? err : new Error(String(err));
        setState({ loading: false, error });
      });

    return () => {
      cancelled = true;
      controller.abort();
    };
  }, [algorithmId, software, runId, enabled, fetchImpl]);

  return state;
}
