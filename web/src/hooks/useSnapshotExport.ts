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
  /**
   * When true (SPA 默认), the server streams the zip back so the browser can
   * download it. JSON metadata still arrives via response headers.
   */
  download?: boolean;
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
  /**
   * true：浏览器已触发 zip 下载；
   * false：仅服务端落盘（JSON 响应，无 blob）；
   * undefined：尚无成功结果。
   */
  browserDownloaded?: boolean;
  /** 浏览器下载使用的文件名（安全策略下无法读取真实本机绝对路径）。 */
  downloadFilename?: string;
}

export interface UseSnapshotExportApi {
  state: UseSnapshotExportState;
  exportSnapshot: (req: SnapshotExportRequest) => Promise<void>;
  /** Clear success / error feedback (e.g. Alert closable). */
  clearFeedback: () => void;
  /** Re-trigger browser download from the last successful zip blob (if any). */
  redownload: () => void;
}

/**
 * Trigger a browser download and keep a short-lived object URL so the user can
 * re-download from the success panel without re-exporting.
 */
function triggerBrowserDownload(blob: Blob, filename: string): string | null {
  if (typeof document === 'undefined' || typeof URL === 'undefined') return null;
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  a.rel = 'noopener';
  a.style.display = 'none';
  document.body.appendChild(a);
  a.click();
  a.remove();
  return url;
}

function filenameFromDisposition(header: string | null, fallback: string): string {
  if (!header) return fallback;
  const utf = /filename\*=UTF-8''([^;]+)/i.exec(header);
  if (utf?.[1]) {
    try {
      return decodeURIComponent(utf[1].trim());
    } catch {
      /* fall through */
    }
  }
  const plain = /filename="?([^";]+)"?/i.exec(header);
  return plain?.[1]?.trim() || fallback;
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

  if (status === 404 || readString(body, 'error_code') === 'RunNotFound') {
    return {
      errorCode: 'RunNotFound',
      message:
        fallbackMessage
        ?? '找不到该次分析的导出记录（后端重启后会清空）。请重新运行分析后再导出。',
    };
  }

  if (status === 503) {
    return {
      errorCode: readString(body, 'error_code') ?? 'SnapshotUnavailable',
      message: fallbackMessage ?? '审计导出服务未就绪，请确认后端已启动。',
    };
  }

  // Any other 4xx / 5xx: pull `error_code` from the body if present;
  // otherwise synthesize a token so the caller still sees a stable string.
  const code = readString(body, 'error_code') ?? `HTTP_${status}`;
  return {
    errorCode: code,
    message:
      fallbackMessage
      ?? (status === 500
        ? '导出失败：服务端内部错误。请确认后端在线，并重新运行分析后再试。'
        : `导出失败（HTTP ${status}）。`),
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
  /** Last successful zip kept for「再次下载」— browsers never expose the real disk path. */
  const lastBlobRef = useRef<{ blob: Blob; filename: string; objectUrl: string | null } | null>(null);

  const releaseLastBlob = useCallback(() => {
    const prev = lastBlobRef.current;
    if (prev?.objectUrl) {
      try {
        URL.revokeObjectURL(prev.objectUrl);
      } catch {
        /* ignore */
      }
    }
    lastBlobRef.current = null;
  }, []);

  const clearFeedback = useCallback(() => {
    releaseLastBlob();
    setState((prev) => ({ loading: prev.loading }));
  }, [releaseLastBlob]);

  const redownload = useCallback(() => {
    const kept = lastBlobRef.current;
    if (!kept) return;
    // Fresh object URL each time; keep the blob itself.
    const url = URL.createObjectURL(kept.blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = kept.filename;
    a.rel = 'noopener';
    a.style.display = 'none';
    document.body.appendChild(a);
    a.click();
    a.remove();
    window.setTimeout(() => {
      try {
        URL.revokeObjectURL(url);
      } catch {
        /* ignore */
      }
    }, 2_000);
  }, []);

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

      // SPA 下载：走 GET /api/snapshot/files/:runId（Content-Length 完整 zip），
      // 避免 POST 二进制经 Vite 代理时被截断 → Chrome ERR_CONNECTION_CLOSED。
      // 契约测试 / 桌面：download 非 true 时仍用 POST JSON。
      const wantDownload = req.download === true;

      try {
        if (wantDownload) {
          const res = await fx(
            `/api/snapshot/files/${encodeURIComponent(req.run_id)}`,
            {
              method: 'GET',
              headers: { Accept: 'application/zip' },
            },
          );
          if (callTokenRef.current !== token) return;

          if (!res.ok) {
            const errBody = await parseJsonOrNull(res);
            setState({ loading: false, error: buildError(res.status, errBody) });
            return;
          }

          const contentType = res.headers.get('content-type') ?? '';
          if (!contentType.includes('application/zip') && !contentType.includes('octet-stream')) {
            setState({
              loading: false,
              error: {
                errorCode: 'MalformedResponse',
                message: `下载响应类型异常（${contentType || 'unknown'}），未拿到 zip。`,
              },
            });
            return;
          }

          const sha256 = res.headers.get('x-snapshot-sha256') ?? '';
          const snapshotPath =
            res.headers.get('x-snapshot-path') ?? req.destination;
          const expectedLen = Number(res.headers.get('content-length') || '0');
          const blob = await res.blob();
          if (callTokenRef.current !== token) return;

          if (expectedLen > 0 && blob.size > 0 && blob.size !== expectedLen) {
            setState({
              loading: false,
              error: {
                errorCode: 'MalformedResponse',
                message: `下载不完整：期望 ${expectedLen} 字节，实际 ${blob.size} 字节。请重试。`,
              },
            });
            return;
          }
          if (blob.size < 22) {
            // empty/corrupt zip local header is at least ~22 bytes
            setState({
              loading: false,
              error: {
                errorCode: 'MalformedResponse',
                message: '下载的审计包为空或已损坏，请重试。',
              },
            });
            return;
          }
          if (!/^[0-9a-f]{64}$/i.test(sha256)) {
            setState({
              loading: false,
              error: {
                errorCode: 'MalformedResponse',
                message: '导出响应缺少有效的 SHA-256 校验头。',
              },
            });
            return;
          }

          const filename = filenameFromDisposition(
            res.headers.get('content-disposition'),
            snapshotPath.replace(/\\/g, '/').split('/').pop() || 'audit-snapshot.zip',
          );
          releaseLastBlob();
          let objectUrl: string | null = null;
          let downloaded = false;
          try {
            objectUrl = triggerBrowserDownload(blob, filename);
            downloaded = objectUrl !== null || typeof document !== 'undefined';
          } catch {
            // jsdom / restricted environments may lack createObjectURL; keep blob for redownload.
            downloaded = false;
          }
          lastBlobRef.current = { blob, filename, objectUrl };
          if (objectUrl) {
            window.setTimeout(() => {
              try {
                URL.revokeObjectURL(objectUrl);
              } catch {
                /* ignore */
              }
              if (lastBlobRef.current?.objectUrl === objectUrl) {
                lastBlobRef.current = { blob, filename, objectUrl: null };
              }
            }, 120_000);
          }
          setState({
            loading: false,
            result: {
              snapshot_path: snapshotPath,
              sha256: sha256.toLowerCase(),
            },
            // Blob 已完整到手即视为可本机保存；createObjectURL 失败仍允许「再次下载」。
            browserDownloaded: downloaded || blob.size > 0,
            downloadFilename: filename,
          });
          return;
        }

        // JSON path (legacy / contract tests / download=false).
        const res = await fx('/api/snapshot/export', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({
            run_id: req.run_id,
            destination: req.destination,
          }),
        });
        if (callTokenRef.current !== token) return;

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
          setState({
            loading: false,
            result: decoded,
            browserDownloaded: false,
          });
          return;
        }

        setState({ loading: false, error: buildError(res.status, body) });
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
      }
    },
    [fetchImpl, releaseLastBlob],
  );

  return { state, exportSnapshot, clearFeedback, redownload };
}

export default useSnapshotExport;
