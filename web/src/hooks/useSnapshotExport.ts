/**
 * `useSnapshotExport` — imperative hook driving `POST /api/snapshot/export`.
 *
 * The Snapshot Exporter (Requirement 7.1) is the only path that produces an
 * Audit Snapshot, and the agent-server is the authoritative gate for both
 * the run-status check (Requirement 7.8) and the 50 MB payload ceiling
 * (Requirement 7.7). This hook is therefore a thin client that:
 *
 *   - issues the request when the caller invokes `exportSnapshot`,
 *   - tracks one in-flight request at a time (overlapping calls are coalesced
 *     onto a fresh attempt; the prior result/error is cleared so stale
 *     responses cannot resurface),
 *   - decodes the structured server-side error variants the
 *     `crates/agent-server/src/handlers/snapshot.rs` handler emits — namely
 *     `RunNotCompleted` (HTTP 409, carries `actual_status`) and
 *     `PayloadTooLarge` (HTTP 413, carries `measured_bytes` and
 *     `ceiling_bytes`) — into the typed `SnapshotExportError` shape the SPA
 *     toasts can consume verbatim,
 *   - falls back to the `error_code` field of the JSON body for any other
 *     4xx / 5xx, and to a synthesized `Unknown` token if the body is not
 *     JSON-shaped.
 *
 * The public surface mirrors `crates/api/src/sidecar.rs::SnapshotExportRequest`
 * and `SnapshotExportResponse` byte-for-byte (snake_case field names) so the
 * wire shape is stable across the boundary.
 *
 * Validates: Requirements 7.1, 7.7, 7.8
 */

import { useCallback, useRef, useState } from 'react';

// ---------------------------------------------------------------------------
// Wire types — mirrored from `crates/api/src/sidecar.rs`
// ---------------------------------------------------------------------------

/**
 * Request body of `POST /api/snapshot/export`.
 *
 * Mirrors `crates/api/src/sidecar.rs::SnapshotExportRequest` exactly. The
 * `destination` is a server-resolved filesystem path; the SPA passes through
 * whatever the user picked and the exporter writes to `<destination>.tmp`
 * before atomic rename.
 */
export interface SnapshotExportRequest {
  run_id: string;
  destination: string;
}

/**
 * Response body of `POST /api/snapshot/export` on the 200 path.
 *
 * Mirrors `crates/api/src/sidecar.rs::SnapshotExportResponse`.
 */
export interface SnapshotExportResponse {
  /** Final path of the produced `.zip` Audit Snapshot. */
  snapshot_path: string;
  /** 64-character lowercase hexadecimal SHA256 of the produced `.zip`. */
  sha256: string;
}

/**
 * Decoded error envelope emitted by `handlers/snapshot.rs`.
 *
 * The wire body always carries an `error_code` token; the two domain-specific
 * variants additionally carry the structured fields the SPA toast renders
 * (Requirements 7.7 / 7.8 require the user-visible refusal to identify the
 * measured payload + ceiling for `PayloadTooLarge`, and the actual run
 * status for `RunNotCompleted`).
 */
export interface SnapshotExportError {
  /** Stable token; e.g. `"RunNotCompleted"`, `"PayloadTooLarge"`. */
  errorCode: string;
  /** Human-readable message from the server, or a synthesized fallback. */
  message: string;
  /** Set on `errorCode === "RunNotCompleted"`. */
  actualStatus?: string;
  /** Set on `errorCode === "PayloadTooLarge"`. */
  measuredBytes?: number;
  /** Set on `errorCode === "PayloadTooLarge"`. */
  ceilingBytes?: number;
}

// ---------------------------------------------------------------------------
// Hook surface
// ---------------------------------------------------------------------------

export interface UseSnapshotExportState {
  loading: boolean;
  result?: SnapshotExportResponse;
  error?: SnapshotExportError;
}

export interface UseSnapshotExportApi {
  state: UseSnapshotExportState;
  exportSnapshot: (req: SnapshotExportRequest) => Promise<void>;
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/** Attempt to parse the response body as JSON; fall back to `null` on failure. */
async function parseJsonOrNull(res: Response): Promise<unknown> {
  try {
    return await res.json();
  } catch {
    return null;
  }
}

/** Read a string field from an unknown JSON value. */
function readString(body: unknown, key: string): string | undefined {
  if (body === null || typeof body !== 'object') return undefined;
  const v = (body as Record<string, unknown>)[key];
  return typeof v === 'string' ? v : undefined;
}

/** Read a finite number field from an unknown JSON value. */
function readNumber(body: unknown, key: string): number | undefined {
  if (body === null || typeof body !== 'object') return undefined;
  const v = (body as Record<string, unknown>)[key];
  return typeof v === 'number' && Number.isFinite(v) ? v : undefined;
}

/**
 * Translate an HTTP failure (status + parsed body) into the typed error
 * envelope the SPA toasts consume. The mapping mirrors `handlers/snapshot.rs`:
 *
 *   - HTTP 409 → `RunNotCompleted { actualStatus }` (Requirement 7.8).
 *   - HTTP 413 → `PayloadTooLarge { measuredBytes, ceilingBytes }`
 *     (Requirement 7.7).
 *   - Anything else → use the body's `error_code` if present, otherwise
 *     synthesize an `HTTP_<status>` token so the caller can still tell the
 *     classes apart.
 */
function buildError(status: number, body: unknown): SnapshotExportError {
  const fallbackMessage = readString(body, 'message');

  if (status === 409) {
    return {
      errorCode: 'RunNotCompleted',
      message:
        fallbackMessage ??
        'Snapshot export refused: the run is not in completed status.',
      actualStatus: readString(body, 'actual_status'),
    };
  }

  if (status === 413) {
    return {
      errorCode: 'PayloadTooLarge',
      message:
        fallbackMessage ??
        'Snapshot export refused: artifact payload exceeds the 50 MB ceiling.',
      measuredBytes: readNumber(body, 'measured_bytes'),
      ceilingBytes: readNumber(body, 'ceiling_bytes'),
    };
  }

  // Any other 4xx / 5xx: pull `error_code` from the body if present;
  // otherwise synthesize a token so the caller still sees a stable string.
  const code = readString(body, 'error_code') ?? `HTTP_${status}`;
  return {
    errorCode: code,
    message: fallbackMessage ?? `Snapshot export failed with HTTP ${status}.`,
  };
}

/**
 * Validate that the 200 body has the shape `crates/api/src/sidecar.rs`
 * advertises. A malformed 200 (missing fields) is folded into a
 * `MalformedResponse` error rather than silently surfacing a half-built
 * `result`.
 */
function decodeSuccess(body: unknown): SnapshotExportResponse | null {
  const snapshotPath = readString(body, 'snapshot_path');
  const sha256 = readString(body, 'sha256');
  if (snapshotPath === undefined || sha256 === undefined) {
    return null;
  }
  return { snapshot_path: snapshotPath, sha256 };
}

// ---------------------------------------------------------------------------
// Hook
// ---------------------------------------------------------------------------

/**
 * React hook exposing an imperative `exportSnapshot` action plus the
 * tri-state `{ loading, result?, error? }` envelope.
 *
 * `fetchImpl` is injectable so the unit tests in `useSnapshotExport.test.ts`
 * can pass a stub; in production callers omit it and the hook captures the
 * global `fetch`. (The default is read at first use rather than at module
 * load to play nicely with environments that set `fetch` after import.)
 */
export function useSnapshotExport(
  fetchImpl?: typeof fetch,
): UseSnapshotExportApi {
  const [state, setState] = useState<UseSnapshotExportState>({
    loading: false,
  });

  // Each call gets a monotonically increasing token. Only the most recent
  // token is allowed to commit state, so a slow first request that resolves
  // after a second click cannot clobber the second request's outcome.
  const callTokenRef = useRef(0);

  const exportSnapshot = useCallback(
    async (req: SnapshotExportRequest): Promise<void> => {
      const token = callTokenRef.current + 1;
      callTokenRef.current = token;

      setState({ loading: true });

      const fx = fetchImpl ?? (typeof fetch === 'function' ? fetch : undefined);
      if (fx === undefined) {
        if (callTokenRef.current === token) {
          setState({
            loading: false,
            error: {
              errorCode: 'FetchUnavailable',
              message: 'fetch API is not available in this environment',
            },
          });
        }
        return;
      }

      let res: Response;
      try {
        res = await fx('/api/snapshot/export', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify(req),
        });
      } catch (err: unknown) {
        if (callTokenRef.current !== token) return;
        const message =
          err instanceof Error && err.message.length > 0
            ? err.message
            : 'network request failed';
        setState({
          loading: false,
          error: { errorCode: 'NetworkError', message },
        });
        return;
      }

      // The server may emit a structured body for any status code; parse it
      // once and dispatch on `res.ok` afterwards so the success and failure
      // branches share the same decoder.
      const body = await parseJsonOrNull(res);
      if (callTokenRef.current !== token) return;

      if (res.ok) {
        const decoded = decodeSuccess(body);
        if (decoded === null) {
          setState({
            loading: false,
            error: {
              errorCode: 'MalformedResponse',
              message:
                'Snapshot export response was missing snapshot_path or sha256.',
            },
          });
          return;
        }
        setState({ loading: false, result: decoded });
        return;
      }

      setState({ loading: false, error: buildError(res.status, body) });
    },
    [fetchImpl],
  );

  return { state, exportSnapshot };
}

export default useSnapshotExport;
