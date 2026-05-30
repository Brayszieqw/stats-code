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
 * One input column posted in the sidecar render request. `dtype` must be
 * one of the four lowercase tokens the generator understands
 * (`numeric | categorical | date | string`).
 *
 * Mirrors `crates/api/src/sidecar.rs::SidecarColumnDto`.
 */
export interface SidecarColumn {
  name: string;
  dtype: string;
}

/**
 * JSON shape of the `POST /api/sidecar/{algorithm_id}` response.
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
  /** 64-character lowercase hexadecimal SHA256 of the input dataset. */
  datasetSha256: string;
  /** Input column metadata in dataset order. */
  columns: SidecarColumn[];
  /** Algorithm parameters as `{{params.<key>}}` string substitutions. */
  params?: Record<string, string>;
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

/**
 * Cache key composes every field that changes the rendered snippet:
 * algorithm, software, dataset hash, the column shape, and the params. Two
 * requests with identical inputs share one fetch; any change busts it.
 */
function cacheKey(p: SidecarParams): string {
  const cols = p.columns.map((c) => `${c.name}:${c.dtype}`).join(',');
  const params = JSON.stringify(p.params ?? {});
  return `${p.algorithmId}::${p.software}::${p.datasetSha256}::${cols}::${params}`;
}

function requestUrl(algorithmId: string): string {
  return `/api/sidecar/${encodeURIComponent(algorithmId)}`;
}

function requestBody(p: SidecarParams): string {
  return JSON.stringify({
    software: p.software,
    dataset_sha256: p.datasetSha256,
    columns: p.columns,
    params: p.params ?? {},
  });
}

// ---------------------------------------------------------------------------
// Hook
// ---------------------------------------------------------------------------

/**
 * Fetch one sidecar snippet on demand via `POST /api/sidecar/{id}`.
 *
 * The Equivalent Code Sidecar is a pure function of its inputs, so the hook
 * posts the column metadata, dataset SHA256, and params directly — there is
 * no server-side run state to look up. `fetchImpl` is injectable so unit
 * tests can pass a stub `fetch`; production callers omit it.
 */
export function useSidecar(
  params: SidecarParams,
  fetchImpl: typeof fetch = fetch,
): SidecarState {
  const { algorithmId, software, datasetSha256, columns, enabled = true } =
    params;

  // Cache lives in a ref so revisiting a tab is a synchronous lookup that
  // does not trigger a re-render.
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

  // Serialize the cache key into the dependency array so the effect re-runs
  // whenever any snippet-affecting input changes.
  const key = cacheKey(params);

  useEffect(() => {
    if (enabled === false) {
      // Disabled tab: drop any prior loading / error state and stay quiet.
      setState({ loading: false });
      return;
    }

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

    fetchImpl(requestUrl(algorithmId), {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: requestBody({
        algorithmId,
        software,
        datasetSha256,
        columns,
        params: params.params,
        enabled: true,
      }),
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
        if (err instanceof DOMException && err.name === 'AbortError') {
          return;
        }
        const error = err instanceof Error ? err : new Error(String(err));
        setState({ loading: false, error });
      });

    return () => {
      cancelled = true;
      controller.abort();
    };
    // `key` already encodes algorithmId/software/sha/columns/params, so it
    // is the single source of truth for "inputs changed".
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key, enabled, fetchImpl]);

  return state;
}
