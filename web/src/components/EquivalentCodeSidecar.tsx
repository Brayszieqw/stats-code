/**
 * EquivalentCodeSidecar — four-tab panel that renders R / SAS / Python /
 * SPSS reference snippets next to an analysis result.
 *
 * Tab order is fixed (Requirement 1.2 / 6.3):
 *
 *   R | SAS | Python | SPSS
 *
 * On first render the active tab is `R` (Requirement 1.2 / 6.3). Each tab's
 * content is driven by the `(algorithm, software)` cell of the Algorithm
 * Coverage Matrix:
 *
 *   - `none`         → `<SidecarPlaceholder>`. No fetch, no snippet text,
 *                      copy button disabled (Requirement 1.5 / 1.6 / 1.8).
 *   - `sidecar_only` → snippet body + an inline notice that names the
 *                      software and states there is no automated parity
 *                      coverage (Requirement 6.4).
 *   - `live` /       → snippet body only (Requirement 1.3).
 *     `recorded`
 *
 * Lazy fetching: `useSidecar` is invoked exactly once per render with the
 * active tab's `software`. Inactive tabs are not fetched. The hook also
 * receives `enabled = false` whenever the cell is `none`, so the
 * `none`-coverage tab never makes a network request.
 *
 * Provenance footer: `<SidecarFooter>` is rendered at the section level,
 * outside the per-tab content branches, so the dataset SHA256 and release
 * version are visible on every tab regardless of state (Requirement 1.7).
 *
 * Copy control: a plain `<button>` placeholder with a no-op `onClick` is
 * rendered for now. Task 14.5 will replace it with the real
 * `<CopyToClipboard>` component (Requirements 1.4 / 1.8 / 1.9). The
 * placeholder still honours the disabled-on-`none` rule so the eventual
 * swap-in keeps that behaviour.
 *
 * Validates: Requirements 1.1, 1.2, 1.3, 1.5, 1.6, 1.8, 6.3, 6.4
 */

import { useState, type ReactElement } from 'react';

import {
  coverageOf,
  type CoverageState,
  type ReferenceSoftware,
} from '../lib/coverageMatrix';
import { useCoverageMatrix } from '../lib/coverageMatrixContext';
import { useSidecar } from '../hooks/useSidecar';
import { CopyToClipboard } from './CopyToClipboard';
import { SidecarFooter } from './SidecarFooter';

// ---------------------------------------------------------------------------
// Tab order — fixed by Requirement 1.2.
// ---------------------------------------------------------------------------

const TAB_ORDER: readonly ReferenceSoftware[] = [
  'R',
  'SAS',
  'Python',
  'SPSS',
] as const;

// ---------------------------------------------------------------------------
// Placeholder subcomponent (Requirement 1.5 / 1.6 / 1.8)
// ---------------------------------------------------------------------------

export interface SidecarPlaceholderProps {
  algorithmId: string;
  software: ReferenceSoftware;
}

/**
 * Rendered when the `(algorithm, software)` matrix cell is `none`.
 *
 * Per Requirement 1.5 the placeholder must name the algorithm, name the
 * software, and state that the matrix value for the cell is `none`. Per
 * Requirement 1.6 it must NOT render any snippet text.
 */
export function SidecarPlaceholder({
  algorithmId,
  software,
}: SidecarPlaceholderProps): ReactElement {
  return (
    <div
      className="sidecar-placeholder"
      data-testid="sidecar-placeholder"
      role="note"
    >
      <p>
        No reference snippet available for{' '}
        <strong>{algorithmId}</strong> in <strong>{software}</strong>.
      </p>
      <p className="sidecar-placeholder-coverage">
        Algorithm Coverage Matrix value: <code>none</code>.
      </p>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Main component
// ---------------------------------------------------------------------------

export interface EquivalentCodeSidecarProps {
  /** Output-Level Algorithm identifier; must match a row in the matrix. */
  algorithmId: string;
  /** Stable id of the analysis run that produced the result. */
  runId: string;
  /** 64-character lowercase hexadecimal SHA256 of the input dataset. */
  datasetSha256: string;
  /** Stats Code release version that emitted the snippet. */
  releaseVersion: string;
}

/**
 * Resolve the coverage state for the active tab, treating "matrix not yet
 * loaded" or "algorithm id unknown" as if the cell were `none`. The latter
 * keeps the SPA from rendering snippet text it cannot back with a coverage
 * cell, which would violate Requirement 1.5.
 */
function resolveCellState(
  matrixLoaded: boolean,
  cell: CoverageState | undefined,
): CoverageState {
  if (!matrixLoaded || cell === undefined) {
    return 'none';
  }
  return cell;
}

export function EquivalentCodeSidecar({
  algorithmId,
  runId,
  datasetSha256,
  releaseVersion,
}: EquivalentCodeSidecarProps): ReactElement {
  // R is the default active tab on first render (Requirement 1.2 / 6.3).
  const [activeTab, setActiveTab] = useState<ReferenceSoftware>('R');

  const {
    matrix,
    loading: matrixLoading,
    error: matrixError,
  } = useCoverageMatrix();

  const cellState: CoverageState = resolveCellState(
    !matrixLoading && matrix !== undefined,
    matrix ? coverageOf(matrix, algorithmId, activeTab) : undefined,
  );

  // The hook is called unconditionally to satisfy the rules of hooks.
  // `enabled: false` makes the hook a no-op when:
  //   - the matrix has not loaded yet,
  //   - the matrix failed to load, or
  //   - the active cell is `none` (Requirement 1.5: "no fetch").
  const sidecar = useSidecar({
    algorithmId,
    software: activeTab,
    runId,
    enabled:
      !matrixLoading &&
      matrixError === undefined &&
      matrix !== undefined &&
      cellState !== 'none',
  });

  // The copy control is delegated to `<CopyToClipboard>`. It treats
  // `text === undefined` and `text === ''` as equivalent to disabled
  // (Requirement 1.8), so the loading / error / matrix-pending states are
  // covered automatically — `snippetText` is `undefined` whenever the
  // body has not landed. The explicit `disabled={cellState === 'none'}`
  // satisfies Requirement 1.8 for the `none` cell.
  const snippetText = sidecar.snippet?.text;

  return (
    <section
      className="equivalent-code-sidecar"
      data-testid="equivalent-code-sidecar"
      aria-label="Equivalent code sidecar"
    >
      <div role="tablist" aria-label="Reference software" className="sidecar-tabs">
        {TAB_ORDER.map((sw) => {
          const isActive = sw === activeTab;
          return (
            <button
              key={sw}
              type="button"
              role="tab"
              aria-selected={isActive}
              aria-controls={`sidecar-panel-${sw}`}
              id={`sidecar-tab-${sw}`}
              data-testid={`sidecar-tab-${sw}`}
              tabIndex={isActive ? 0 : -1}
              className={
                isActive ? 'sidecar-tab sidecar-tab-active' : 'sidecar-tab'
              }
              onClick={() => setActiveTab(sw)}
            >
              {sw}
            </button>
          );
        })}
      </div>

      <div
        role="tabpanel"
        id={`sidecar-panel-${activeTab}`}
        aria-labelledby={`sidecar-tab-${activeTab}`}
        data-testid={`sidecar-panel-${activeTab}`}
        className="sidecar-tabpanel"
      >
        {matrixLoading && (
          <div
            role="status"
            aria-label="Loading coverage matrix"
            data-testid="sidecar-matrix-spinner"
            className="sidecar-spinner"
          >
            Loading…
          </div>
        )}

        {matrixError !== undefined && (
          <div
            role="alert"
            data-testid="sidecar-matrix-error"
            className="sidecar-error"
          >
            Failed to load coverage matrix: {matrixError.message}
          </div>
        )}

        {!matrixLoading && matrixError === undefined && (
          <>
            {cellState === 'none' && (
              <SidecarPlaceholder
                algorithmId={algorithmId}
                software={activeTab}
              />
            )}

            {cellState !== 'none' && sidecar.loading && (
              <div
                role="status"
                aria-label="Loading snippet"
                data-testid="sidecar-snippet-spinner"
                className="sidecar-spinner"
              >
                Loading snippet…
              </div>
            )}

            {cellState !== 'none' &&
              !sidecar.loading &&
              sidecar.error !== undefined && (
                <div
                  role="alert"
                  data-testid="sidecar-snippet-error"
                  className="sidecar-error"
                >
                  Failed to load snippet: {sidecar.error.message}
                </div>
              )}

            {cellState !== 'none' &&
              !sidecar.loading &&
              sidecar.error === undefined &&
              snippetText !== undefined && (
                <>
                  <pre
                    className="sidecar-snippet"
                    data-testid="sidecar-snippet"
                  >
                    <code>{snippetText}</code>
                  </pre>
                  {cellState === 'sidecar_only' && (
                    <div
                      role="note"
                      data-testid="sidecar-notice"
                      className="sidecar-notice"
                    >
                      This algorithm has no automated parity coverage for{' '}
                      <strong>{activeTab}</strong>.
                    </div>
                  )}
                </>
              )}
          </>
        )}

        <CopyToClipboard
          text={snippetText}
          disabled={cellState === 'none'}
        />
      </div>

      <SidecarFooter
        datasetSha256={datasetSha256}
        releaseVersion={releaseVersion}
      />
    </section>
  );
}

export default EquivalentCodeSidecar;
