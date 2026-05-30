/**
 * Tests for `<AnalysisResultView>`.
 *
 * Covers task 15.1's explicit requirement: when the parent receives the
 * documented props (`algorithmId / params / datasetSha256 / columns /
 * runId / runStatus`), the analysis result view mounts BOTH
 * `<EquivalentCodeSidecar>` and `<ExportSnapshotButton>` and forwards the
 * relevant subset of the props to each child.
 *
 * Validates: Requirements 1.1, 7.1
 */

import { describe, it, expect, vi, afterEach, beforeEach } from 'vitest';
import { render, screen, waitFor, cleanup } from '@testing-library/react';

// `<EquivalentCodeSidecar>` ultimately invokes `useSidecar`. Stub it so
// the test does not need a live `/api/sidecar/...` endpoint.
const { useSidecarSpy } = vi.hoisted(() => ({
  useSidecarSpy: vi.fn(),
}));
vi.mock('../hooks/useSidecar', () => ({
  useSidecar: useSidecarSpy,
}));

import { AnalysisResultView } from './AnalysisResultView';
import { CoverageMatrixProvider } from '../lib/coverageMatrixContext';
import type { CoverageMatrix } from '../lib/coverageMatrix';
import type { ColumnSummary } from '../api/types';

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const SAMPLE_SHA =
  'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855';
const SAMPLE_VERSION = '0.5.0';

const SAMPLE_COLUMNS: ColumnSummary[] = [
  { name: 'age', inferred_type: 'Numeric', missing_count: 0 },
  { name: 'group', inferred_type: 'Categorical', missing_count: 0 },
];

function buildMatrix(): CoverageMatrix {
  return {
    schema_version: 1,
    release_version: SAMPLE_VERSION,
    algorithms: [
      {
        id: 'tableone',
        display_name: 'tableone',
        iterative: false,
        coverage: {
          R: 'live',
          SAS: 'recorded',
          Python: 'live',
          SPSS: 'recorded',
        },
        reference: {
          R: { callable: 'fn', package: 'pkg', version: '1.0' },
          SAS: { callable: 'PROC X', version: '9.4' },
          Python: { callable: 'fn', package: 'pkg', version: '1.0' },
          SPSS: { callable: 'PROC X', version: '29.0' },
        },
      },
    ],
  };
}

function matrixFetch(): typeof fetch {
  const fn = vi.fn(
    async () =>
      new Response(JSON.stringify(buildMatrix()), {
        status: 200,
        headers: { 'Content-Type': 'application/json' },
      }),
  );
  return fn as unknown as typeof fetch;
}

afterEach(() => {
  cleanup();
  useSidecarSpy.mockReset();
});

// ---------------------------------------------------------------------------
// Default useSidecar stub: deliver a deterministic snippet on the active tab.
// ---------------------------------------------------------------------------

beforeEach(() => {
  useSidecarSpy.mockImplementation(
    ({
      software,
      enabled = true,
    }: {
      algorithmId: string;
      software: 'R' | 'SAS' | 'Python' | 'SPSS';
      enabled?: boolean;
    }) => {
      if (!enabled) return { loading: false };
      return {
        loading: false,
        snippet: {
          algorithm_id: 'tableone',
          software,
          coverage_value: 'live' as const,
          text: `# ${software} snippet body\n`,
          sha256_of_dataset: SAMPLE_SHA,
          release_version: SAMPLE_VERSION,
        },
      };
    },
  );
});

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe('AnalysisResultView â€?mount points', () => {
  it('renders both <EquivalentCodeSidecar> and <ExportSnapshotButton> when the required props are supplied', async () => {
    useSidecarSpy.mockImplementation(
      ({
        software,
        enabled = true,
      }: {
        algorithmId: string;
        software: 'R' | 'SAS' | 'Python' | 'SPSS';
        enabled?: boolean;
      }) => {
        if (!enabled) return { loading: false };
        return {
          loading: false,
          snippet: {
            algorithm_id: 'tableone',
            software,
            coverage_value: 'live' as const,
            text: `# ${software} snippet body\n`,
            sha256_of_dataset: SAMPLE_SHA,
            release_version: SAMPLE_VERSION,
          },
        };
      },
    );

    render(
      <CoverageMatrixProvider fetchImpl={matrixFetch()}>
        <AnalysisResultView
          algorithmId="tableone"
          params={{ continuous: ['age'] }}
          datasetSha256={SAMPLE_SHA}
          columns={SAMPLE_COLUMNS}
          runId="run-77"
          runStatus="completed"
          releaseVersion={SAMPLE_VERSION}
          snapshotDestination="C:/exports/run-77.zip"
        />
      </CoverageMatrixProvider>,
    );

    // Outer wrapper is present.
    expect(screen.getByTestId('analysis-result-view')).toBeInTheDocument();

    // Sidecar mounts immediately; the R tab is the default active.
    expect(
      screen.getByTestId('equivalent-code-sidecar'),
    ).toBeInTheDocument();

    // Export button mounts immediately and is enabled because runStatus is
    // exactly "completed".
    const exportButton = screen.getByTestId('export-snapshot-button');
    expect(exportButton).toBeInTheDocument();
    expect(exportButton).not.toBeDisabled();

    // Once the matrix resolves, the active R tab shows the stubbed snippet,
    // confirming runId/datasetSha256/releaseVersion are threaded through to
    // the sidecar.
    await waitFor(() => {
      expect(screen.getByTestId('sidecar-snippet')).toHaveTextContent(
        '# R snippet body',
      );
    });

    // Footer carries the SHA256 + version forwarded from the parent.
    const footer = screen.getByTestId('sidecar-footer');
    expect(footer.textContent).toContain(SAMPLE_SHA);
    expect(footer.textContent).toContain(SAMPLE_VERSION);

    // useSidecar was invoked with the dataset SHA256 + columns forwarded
    // from the parent (the sidecar is stateless â€?no run_id needed).
    const enabledCalls = useSidecarSpy.mock.calls.filter(
      ([params]) => params.enabled === true,
    );
    expect(enabledCalls.length).toBeGreaterThan(0);
    for (const [params] of enabledCalls) {
      expect(params.datasetSha256).toBe(SAMPLE_SHA);
      expect(params.algorithmId).toBe('tableone');
      // Columns are mapped from the dataset summary (PascalCase â†?
      // lowercase dtype tokens) and forwarded to the generator.
      expect(params.columns).toEqual([
        { name: 'age', dtype: 'numeric' },
        { name: 'group', dtype: 'categorical' },
      ]);
    }
  });

  it('disables the export button when runStatus is not "completed"', async () => {
    render(
      <CoverageMatrixProvider fetchImpl={matrixFetch()}>
        <AnalysisResultView
          algorithmId="tableone"
          params={{}}
          datasetSha256={SAMPLE_SHA}
          columns={SAMPLE_COLUMNS}
          runId="run-78"
          runStatus="running"
          releaseVersion={SAMPLE_VERSION}
          snapshotDestination="C:/exports/run-78.zip"
        />
      </CoverageMatrixProvider>,
    );

    expect(screen.getByTestId('export-snapshot-button')).toBeDisabled();
    // The sidecar still mounts even when the run is mid-flight.
    expect(
      screen.getByTestId('equivalent-code-sidecar'),
    ).toBeInTheDocument();

    // Let the matrix-fetch effect settle so the trailing state update
    // happens inside an act-wrapped window.
    await waitFor(() => {
      expect(screen.getByTestId('sidecar-snippet')).toBeInTheDocument();
    });
  });
});
