/**
 * ExportSnapshotButton — SPA control that drives `POST /api/snapshot/export`.
 *
 * The button is the user-facing trigger of Requirement 7.1 ("Export Audit
 * Snapshot" on a `completed` run). The visible disable rule is a UX gate
 * only: the agent-server is the authoritative authority for the run-status
 * (Requirement 7.8) and 50 MB ceiling (Requirement 7.7) checks. The button
 * therefore:
 *
 *   - is `disabled` whenever `runStatus !== 'completed'`, so the common case
 *     (a still-running analysis) cannot fire a request that the server is
 *     guaranteed to refuse;
 *   - fires `useSnapshotExport().exportSnapshot({ run_id, destination })`
 *     on click and shows an in-flight indicator while the promise is
 *     unsettled;
 *   - on success renders an inline `role="status"` toast that names the
 *     resulting snapshot path (Requirement 7.1 — the user-visible feedback
 *     of a completed export);
 *   - on `RunNotCompleted` renders an inline `role="alert"` toast that
 *     identifies the actual run status (Requirement 7.8 — the refusal must
 *     identify the actual status);
 *   - on `PayloadTooLarge` renders an inline `role="alert"` toast that
 *     identifies the measured bytes and the 50 MB ceiling (Requirement
 *     7.7 — the refusal must identify the measured payload and the ceiling);
 *   - on any other error code renders an inline `role="alert"` toast that
 *     surfaces the server's error code and message verbatim.
 *
 * The toasts are rendered in-DOM rather than through a global notification
 * portal so the component's tests can assert on them deterministically and
 * so the Stats Code SPA does not pull in a notification provider for one
 * site. The visual style is intentionally minimal; the analysis-result view
 * styles `.export-snapshot-toast-*` selectors.
 *
 * Validates: Requirements 7.1, 7.7, 7.8
 */

import { useCallback, type ReactElement } from 'react';

import {
  useSnapshotExport,
  type SnapshotExportError,
} from '../hooks/useSnapshotExport';

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

export interface ExportSnapshotButtonProps {
  /** Identifier of the analysis run to export. */
  runId: string;
  /** User-selected destination path for the resulting `.zip`. */
  destination: string;
  /**
   * Status of the run as known to the SPA. The button is enabled only
   * when this is exactly `"completed"`; any other value puts it in the
   * disabled UX gate.
   */
  runStatus: string;
  /**
   * Optional injected `fetch` for tests. Production callers omit it and
   * the hook uses the global `fetch`.
   */
  fetchImpl?: typeof fetch;
}

// ---------------------------------------------------------------------------
// Toast helpers
// ---------------------------------------------------------------------------

/**
 * Render the human-readable refusal message for a given typed error.
 *
 * For `PayloadTooLarge` the message must identify both the measured byte
 * count and the 50 MB ceiling (Requirement 7.7); for `RunNotCompleted` it
 * must identify the actual status (Requirement 7.8). When the server omits
 * a structured field (defensive against schema drift), we fall back to the
 * server's `message` so the user still sees a meaningful refusal rather
 * than a blank toast.
 */
function describeError(error: SnapshotExportError): string {
  if (error.errorCode === 'PayloadTooLarge') {
    if (
      error.measuredBytes !== undefined &&
      error.ceilingBytes !== undefined
    ) {
      return `Snapshot too large: measured ${error.measuredBytes} bytes, ceiling ${error.ceilingBytes} bytes.`;
    }
    return error.message;
  }
  if (error.errorCode === 'RunNotCompleted') {
    if (error.actualStatus !== undefined) {
      return `Cannot export: run status is "${error.actualStatus}", expected "completed".`;
    }
    return error.message;
  }
  return `${error.errorCode}: ${error.message}`;
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function ExportSnapshotButton(
  props: ExportSnapshotButtonProps,
): ReactElement {
  const { runId, destination, runStatus, fetchImpl } = props;

  const { state, exportSnapshot } = useSnapshotExport(fetchImpl);

  // Client-side UX gate (Requirement 7.1 / 7.8): the server is still
  // authoritative, but a non-completed run guaranteeably yields a 409, so
  // it is friendlier to disable the button.
  const isCompleted = runStatus === 'completed';
  const buttonDisabled = !isCompleted || state.loading;

  const handleClick = useCallback(() => {
    if (buttonDisabled) return;
    // Fire-and-forget: React's onClick does not await the promise.
    void exportSnapshot({ run_id: runId, destination });
  }, [buttonDisabled, exportSnapshot, runId, destination]);

  const buttonLabel = state.loading ? '导出中…' : '导出审计快照';

  return (
    <div
      className="export-snapshot-button"
      data-testid="export-snapshot-button-root"
    >
      <button
        type="button"
        className="export-snapshot-button-control"
        data-testid="export-snapshot-button"
        disabled={buttonDisabled}
        aria-busy={state.loading}
        onClick={handleClick}
      >
        {buttonLabel}
      </button>

      {state.result !== undefined && (
        <div
          className="export-snapshot-toast export-snapshot-toast-success"
          data-testid="export-snapshot-toast-success"
          role="status"
          aria-live="polite"
        >
          快照已保存到 {state.result.snapshot_path}
        </div>
      )}

      {state.error !== undefined && (
        <div
          className={`export-snapshot-toast export-snapshot-toast-error export-snapshot-toast-error-${state.error.errorCode}`}
          data-testid="export-snapshot-toast-error"
          data-error-code={state.error.errorCode}
          role="alert"
        >
          {describeError(state.error)}
        </div>
      )}
    </div>
  );
}

export default ExportSnapshotButton;
