/**
 * Property-based test for CodePanel — Property 6: 无结果不挂载代码.
 *
 * When the latest result carries no `analysis`, CodePanel must NOT mount
 * EquivalentCodeSidecar (shows a placeholder), must NOT invoke the sidecar
 * hook (no fetch), and the 运行 button must be disabled.
 *
 * Validates: Requirements 7.5
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import * as fc from 'fast-check';

const { useSidecarSpy, useCoverageMatrixSpy } = vi.hoisted(() => ({
  useSidecarSpy: vi.fn(),
  useCoverageMatrixSpy: vi.fn(),
}));

vi.mock('../../hooks/useSidecar', () => ({
  useSidecar: useSidecarSpy,
}));

vi.mock('../../lib/coverageMatrixContext', () => ({
  useCoverageMatrix: useCoverageMatrixSpy,
}));

import { CodePanel } from './CodePanel';
import type { AnalysisResultMeta, DatasetSummary } from '../../api/types';

beforeEach(() => {
  vi.clearAllMocks();
  useSidecarSpy.mockReturnValue({ snippet: undefined, loading: false, error: undefined });
  useCoverageMatrixSpy.mockReturnValue({
    matrix: { schema_version: 1, release_version: '1.0.0', algorithms: [] },
    loading: false,
    error: undefined,
  });
});

afterEach(() => {
  vi.unstubAllGlobals();
});

const dataset: DatasetSummary = {
  dataset_id: 'ds-1',
  file_name: 'cohort.csv',
  size_bytes: 1024,
  encoding: 'Utf8',
  row_count: 100,
  columns: [{ name: 'age', inferred_type: 'Numeric', missing_count: 0 }],
  uploaded_at: '2026-01-01T00:00:00Z',
  sha256: null,
};

describe('Property 6: 无结果不挂载代码 (Requirement 7.5)', () => {
  it(
    'for any null/undefined analysis, the sidecar is not mounted, no fetch, run disabled',
    () => {
      fc.assert(
        fc.property(fc.constantFrom(null, undefined), (analysis) => {
          useSidecarSpy.mockClear();
          const { unmount } = render(
            <CodePanel sessionId="s1" analysis={analysis as AnalysisResultMeta | null | undefined} />,
          );
          // Sidecar component is not mounted.
          expect(screen.queryByTestId('equivalent-code-sidecar')).toBeNull();
          // The sidecar hook is never invoked → no network request.
          expect(useSidecarSpy).not.toHaveBeenCalled();
          // The 运行 button is disabled.
          expect(screen.getByLabelText('运行')).toBeDisabled();
          unmount();
        }),
        { numRuns: 10 },
      );
    },
    15_000,
  );


  it('mounts the sidecar when analysis is present', () => {
    const analysis: AnalysisResultMeta = {
      algorithm_id: 'model_linear',
      dataset_id: 'ds-1',
      dataset_sha256: 'a'.repeat(64),
      columns: [],
      params: {},
      run_id: 'run-1',
      run_status: 'completed',
    };
    render(<CodePanel sessionId="s1" analysis={analysis} />);
    expect(screen.getByTestId('equivalent-code-sidecar')).toBeInTheDocument();
  });

  it('does not claim reproducible code when the dataset fingerprint is missing', () => {
    const analysis: AnalysisResultMeta = {
      algorithm_id: 'model_linear',
      dataset_id: 'ds-1',
      dataset_sha256: null,
      columns: [],
      params: {},
      run_id: 'run-without-sha',
      run_status: 'completed',
    };

    render(<CodePanel sessionId="s1" analysis={analysis} dataset={dataset} />);
    expect(screen.getByText('数据指纹缺失')).toBeInTheDocument();
    expect(screen.getByText('缺少数据指纹，无法生成可验证的等价代码')).toBeInTheDocument();
    expect(screen.queryByTestId('equivalent-code-sidecar')).not.toBeInTheDocument();
    expect(useSidecarSpy).not.toHaveBeenCalled();
  });

  it('uses a compact dataset context and lifts a server-approved rerun', async () => {
    const analysis: AnalysisResultMeta = {
      algorithm_id: 'model_linear',
      dataset_id: 'ds-1',
      dataset_sha256: 'a'.repeat(64),
      columns: [],
      params: {},
      run_id: 'run-1',
      run_status: 'completed',
      plan_id: 'plan-approved-1',
    };
    const result = { schema_version: '1.0', payload: {}, risk_signals: [] };
    const onRunComplete = vi.fn();
    vi.stubGlobal('fetch', vi.fn(async () => ({
      ok: true,
      status: 200,
      statusText: 'ok',
      json: async () => result,
    } as Response)));

    render(
      <CodePanel
        sessionId="s1"
        analysis={analysis}
        dataset={dataset}
        onRunComplete={onRunComplete}
      />,
    );

    expect(screen.getByText('100 行 × 1 列')).toBeInTheDocument();
    fireEvent.click(screen.getByLabelText('运行'));
    await waitFor(() => expect(onRunComplete).toHaveBeenCalledWith(result));
  });
});
