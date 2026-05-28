/**
 * AnalysisResultView — mount point for the trust-credential layer next to
 * an analysis result.
 *
 * This component is the "analysis result view" referenced by the
 * parity-and-multilang-sidecar feature: it accepts the run's identifying
 * inputs and forwards them to `<EquivalentCodeSidecar>` (Requirement 1.1)
 * and `<ExportSnapshotButton>` (Requirement 7.1) without introducing a new
 * route. The parent surface (`WorkflowPage` step 4, `MessageList` skill
 * result bubble, etc.) renders the existing analysis result first and then
 * mounts this view alongside it.
 *
 * Prop forwarding contract (per task 15.1):
 *
 *   - `algorithmId` / `runId` / `datasetSha256` → `<EquivalentCodeSidecar>`
 *   - `runId` / `runStatus`                     → `<ExportSnapshotButton>`
 *   - `params` / `columns` are accepted at this boundary even though the
 *     current sidecar fetch path resolves snippet bodies server-side via
 *     `runId`; carrying them through keeps the prop shape stable for the
 *     follow-up tasks that thread params/columns into snippet rendering.
 *
 * The component is purely presentational. It performs no I/O; both child
 * components own their own fetch lifecycles. Any styling lives on the
 * parent surface.
 *
 * Validates: Requirements 1.1, 7.1
 */

import type { ReactElement } from 'react';

import type { ColumnSummary } from '../api/types';
import { EquivalentCodeSidecar } from './EquivalentCodeSidecar';
import { ExportSnapshotButton } from './ExportSnapshotButton';

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

/**
 * Analysis parameter bag forwarded from the configurator. The exporter and
 * the snippet renderer are both algorithm-aware on the server side, so the
 * SPA only needs to carry the bag verbatim.
 */
export type AnalysisParams = Record<string, unknown>;

export interface AnalysisResultViewProps {
  /** Output-Level Algorithm identifier; must match a row in the matrix. */
  algorithmId: string;
  /** Algorithm parameters as submitted by the configurator. */
  params: AnalysisParams;
  /** 64-character lowercase hexadecimal SHA256 of the input dataset. */
  datasetSha256: string;
  /** Column metadata (names + inferred types) of the input dataset. */
  columns: ColumnSummary[];
  /** Stable id of the analysis run that produced the result. */
  runId: string;
  /**
   * Status of the run as known to the SPA. The export button is enabled
   * only when this is exactly `"completed"`; the agent-server is still
   * the authoritative gate (Requirement 7.8).
   */
  runStatus: string;
  /** Stats Code release version that emitted the snippet (Requirement 1.7). */
  releaseVersion: string;
  /** User-selected destination path for the resulting `.zip`. */
  snapshotDestination: string;
  /** Optional injected `fetch` for tests; production omits it. */
  fetchImpl?: typeof fetch;
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function AnalysisResultView(
  props: AnalysisResultViewProps,
): ReactElement {
  const {
    algorithmId,
    datasetSha256,
    runId,
    runStatus,
    releaseVersion,
    snapshotDestination,
    fetchImpl,
  } = props;

  return (
    <section
      className="analysis-result-view"
      data-testid="analysis-result-view"
      aria-label="Analysis result trust-credential layer"
    >
      <EquivalentCodeSidecar
        algorithmId={algorithmId}
        runId={runId}
        datasetSha256={datasetSha256}
        releaseVersion={releaseVersion}
      />
      <ExportSnapshotButton
        runId={runId}
        destination={snapshotDestination}
        runStatus={runStatus}
        fetchImpl={fetchImpl}
      />
    </section>
  );
}

export default AnalysisResultView;
