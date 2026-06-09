/**
 * Property-based test for CodePanel — Property 6: 无结果不挂载代码.
 *
 * When the latest result carries no `analysis`, CodePanel must NOT mount
 * EquivalentCodeSidecar (shows a placeholder), must NOT invoke the sidecar
 * hook (no fetch), and the 运行 button must be disabled.
 *
 * Validates: Requirements 7.5
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
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
import type { AnalysisResultMeta } from '../../api/types';

beforeEach(() => {
  vi.clearAllMocks();
  useSidecarSpy.mockReturnValue({ snippet: undefined, loading: false, error: undefined });
  useCoverageMatrixSpy.mockReturnValue({
    matrix: { schema_version: 1, release_version: '1.0.0', algorithms: [] },
    loading: false,
    error: undefined,
  });
});

describe('Property 6: 无结果不挂载代码 (Requirement 7.5)', () => {
  it('for any null/undefined analysis, the sidecar is not mounted, no fetch, run disabled', () => {
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
  });

  it('mounts the sidecar when analysis is present', () => {
    const analysis: AnalysisResultMeta = {
      algorithm_id: 'model_linear',
      dataset_id: 'ds-1',
      dataset_sha256: null,
      columns: [],
      params: {},
      run_id: 'run-1',
      run_status: 'completed',
    };
    render(<CodePanel sessionId="s1" analysis={analysis} />);
    expect(screen.getByTestId('equivalent-code-sidecar')).toBeInTheDocument();
  });
});
