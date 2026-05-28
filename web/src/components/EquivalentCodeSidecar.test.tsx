/**
 * Tests for `<EquivalentCodeSidecar>`.
 *
 * Covers:
 *   - Default active tab is R (Requirement 1.2 / 6.3).
 *   - Clicking a tab switches the active tab (Requirement 1.3).
 *   - `none` cell renders the placeholder + disabled copy button, no snippet
 *     text, no fetch (Requirement 1.5 / 1.6 / 1.8).
 *   - `sidecar_only` renders snippet + inline notice (Requirement 6.4).
 *   - `live` renders snippet only (Requirement 1.3).
 *   - Footer renders SHA256 + version on every tab/state (Requirement 1.7).
 *   - Only the active tab's snippet is fetched.
 *
 * Validates: Requirements 1.1, 1.2, 1.3, 1.5, 1.6, 1.8, 6.3, 6.4
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, act } from '@testing-library/react';
import type { ReactElement } from 'react';

// -- Hoisted spy for the useSidecar mock so individual tests can inspect or
//    override its behaviour. `vi.hoisted` ensures the spy exists before the
//    `vi.mock` factory runs.
const { useSidecarSpy } = vi.hoisted(() => ({
  useSidecarSpy: vi.fn(),
}));

vi.mock('../hooks/useSidecar', () => ({
  useSidecar: useSidecarSpy,
}));

import { EquivalentCodeSidecar } from './EquivalentCodeSidecar';
import { CoverageMatrixProvider } from '../lib/coverageMatrixContext';
import type {
  CoverageMatrix,
  CoverageState,
} from '../lib/coverageMatrix';
import type { SidecarSnippet } from '../hooks/useSidecar';

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const SAMPLE_SHA =
  'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855';
const SAMPLE_VERSION = '0.5.0';

function buildMatrix(
  cellByAlgorithmId: Record<string, Record<'R' | 'SAS' | 'Python' | 'SPSS', CoverageState>>,
): CoverageMatrix {
  return {
    schema_version: 1,
    release_version: SAMPLE_VERSION,
    algorithms: Object.entries(cellByAlgorithmId).map(([id, coverage]) => ({
      id,
      display_name: id,
      iterative: false,
      coverage,
      reference: {
        R: { callable: 'fn', package: 'pkg', version: '1.0' },
        SAS: { callable: 'PROC X', version: '9.4' },
        Python: { callable: 'fn', package: 'pkg', version: '1.0' },
        SPSS: { callable: 'PROC X', version: '29.0' },
      },
    })),
  };
}

function snippet(
  software: 'R' | 'SAS' | 'Python' | 'SPSS',
  text: string,
  coverageValue: CoverageState = 'live',
): SidecarSnippet {
  return {
    algorithm_id: 'tableone',
    software,
    coverage_value: coverageValue,
    text,
    sha256_of_dataset: SAMPLE_SHA,
    release_version: SAMPLE_VERSION,
  };
}

/**
 * Build a matrix-fetch stub and wrap children in `CoverageMatrixProvider`.
 * The provider resolves the matrix asynchronously so tests must `await
 * waitFor(...)` for it to land before asserting on tab content.
 */
function renderWithMatrix(
  matrix: CoverageMatrix,
  ui: ReactElement,
): ReturnType<typeof render> {
  const fetchImpl = vi.fn(
    async () =>
      new Response(JSON.stringify(matrix), {
        status: 200,
        headers: { 'Content-Type': 'application/json' },
      }),
  );
  return render(
    <CoverageMatrixProvider fetchImpl={fetchImpl as unknown as typeof fetch}>
      {ui}
    </CoverageMatrixProvider>,
  );
}

// ---------------------------------------------------------------------------
// Setup: default `useSidecar` behaviour returns a deterministic snippet
// per software. Individual tests override this with `mockImplementation`.
// ---------------------------------------------------------------------------

beforeEach(() => {
  useSidecarSpy.mockReset();
  useSidecarSpy.mockImplementation(
    ({
      software,
      enabled = true,
    }: {
      algorithmId: string;
      software: 'R' | 'SAS' | 'Python' | 'SPSS';
      runId: string;
      enabled?: boolean;
    }) => {
      if (!enabled) {
        return { loading: false };
      }
      return {
        loading: false,
        snippet: snippet(software, `# ${software} snippet body\n`),
      };
    },
  );
});

// ---------------------------------------------------------------------------
// Default-tab tests
// ---------------------------------------------------------------------------

describe('EquivalentCodeSidecar — default tab', () => {
  it('activates the R tab on first render', async () => {
    const matrix = buildMatrix({
      tableone: { R: 'live', SAS: 'recorded', Python: 'live', SPSS: 'recorded' },
    });

    renderWithMatrix(
      matrix,
      <EquivalentCodeSidecar
        algorithmId="tableone"
        runId="run-1"
        datasetSha256={SAMPLE_SHA}
        releaseVersion={SAMPLE_VERSION}
      />,
    );

    // R tab is selected from the very first render — assert before the
    // matrix even resolves.
    expect(screen.getByTestId('sidecar-tab-R')).toHaveAttribute(
      'aria-selected',
      'true',
    );
    expect(screen.getByTestId('sidecar-tab-SAS')).toHaveAttribute(
      'aria-selected',
      'false',
    );
    expect(screen.getByTestId('sidecar-tab-Python')).toHaveAttribute(
      'aria-selected',
      'false',
    );
    expect(screen.getByTestId('sidecar-tab-SPSS')).toHaveAttribute(
      'aria-selected',
      'false',
    );

    // Once the matrix lands, the R panel shows R's snippet.
    await waitFor(() => {
      expect(screen.getByTestId('sidecar-snippet')).toHaveTextContent(
        '# R snippet body',
      );
    });
  });

  it('renders tabs in the fixed order R / SAS / Python / SPSS', async () => {
    const matrix = buildMatrix({
      tableone: { R: 'live', SAS: 'recorded', Python: 'live', SPSS: 'recorded' },
    });

    renderWithMatrix(
      matrix,
      <EquivalentCodeSidecar
        algorithmId="tableone"
        runId="run-1"
        datasetSha256={SAMPLE_SHA}
        releaseVersion={SAMPLE_VERSION}
      />,
    );

    const tabs = screen.getAllByRole('tab');
    expect(tabs.map((t) => t.textContent)).toEqual([
      'R',
      'SAS',
      'Python',
      'SPSS',
    ]);

    // Let the matrix fetch resolve so the trailing state update is
    // observed inside an `act` window (silences the React 19 warning).
    await waitFor(() => {
      expect(screen.getByTestId('sidecar-snippet')).toBeInTheDocument();
    });
  });
});

// ---------------------------------------------------------------------------
// Tab-switch test
// ---------------------------------------------------------------------------

describe('EquivalentCodeSidecar — tab switching', () => {
  it('switches the active tab on click', async () => {
    const matrix = buildMatrix({
      tableone: { R: 'live', SAS: 'recorded', Python: 'live', SPSS: 'recorded' },
    });

    renderWithMatrix(
      matrix,
      <EquivalentCodeSidecar
        algorithmId="tableone"
        runId="run-1"
        datasetSha256={SAMPLE_SHA}
        releaseVersion={SAMPLE_VERSION}
      />,
    );

    await waitFor(() => {
      expect(screen.getByTestId('sidecar-snippet')).toHaveTextContent(
        '# R snippet body',
      );
    });

    act(() => {
      screen.getByTestId('sidecar-tab-Python').click();
    });

    expect(screen.getByTestId('sidecar-tab-Python')).toHaveAttribute(
      'aria-selected',
      'true',
    );
    expect(screen.getByTestId('sidecar-tab-R')).toHaveAttribute(
      'aria-selected',
      'false',
    );

    await waitFor(() => {
      expect(screen.getByTestId('sidecar-snippet')).toHaveTextContent(
        '# Python snippet body',
      );
    });
  });
});

// ---------------------------------------------------------------------------
// `none` cell behaviour
// ---------------------------------------------------------------------------

describe('EquivalentCodeSidecar — none coverage', () => {
  it('renders the placeholder, disables copy, and does not fetch the snippet', async () => {
    const matrix = buildMatrix({
      logistic: { R: 'none', SAS: 'recorded', Python: 'live', SPSS: 'none' },
    });

    renderWithMatrix(
      matrix,
      <EquivalentCodeSidecar
        algorithmId="logistic"
        runId="run-1"
        datasetSha256={SAMPLE_SHA}
        releaseVersion={SAMPLE_VERSION}
      />,
    );

    // Placeholder lands once the matrix has resolved.
    await waitFor(() => {
      expect(screen.getByTestId('sidecar-placeholder')).toBeInTheDocument();
    });

    // No snippet body is rendered (Requirement 1.6).
    expect(screen.queryByTestId('sidecar-snippet')).not.toBeInTheDocument();

    // Placeholder must name algorithm + software (Requirement 1.5).
    const placeholder = screen.getByTestId('sidecar-placeholder');
    expect(placeholder.textContent).toContain('logistic');
    expect(placeholder.textContent).toContain('R');
    expect(placeholder.textContent).toContain('none');

    // Copy button is disabled (Requirement 1.8).
    expect(screen.getByTestId('copy-to-clipboard-button')).toBeDisabled();

    // useSidecar must have been called only with `enabled: false`
    // for the active `none` cell (Requirement 1.5: "Don't fetch the
    // snippet").
    const enabledCalls = useSidecarSpy.mock.calls.filter(
      ([params]) =>
        params.algorithmId === 'logistic' &&
        params.software === 'R' &&
        params.enabled === true,
    );
    expect(enabledCalls).toHaveLength(0);
  });
});

// ---------------------------------------------------------------------------
// `sidecar_only` cell behaviour
// ---------------------------------------------------------------------------

describe('EquivalentCodeSidecar — sidecar_only coverage', () => {
  it('renders the snippet plus an inline notice', async () => {
    useSidecarSpy.mockImplementation(
      ({
        software,
        enabled = true,
      }: {
        algorithmId: string;
        software: 'R' | 'SAS' | 'Python' | 'SPSS';
        runId: string;
        enabled?: boolean;
      }) => {
        if (!enabled) return { loading: false };
        return {
          loading: false,
          snippet: snippet(
            software,
            `# ${software} sidecar-only body\n`,
            'sidecar_only',
          ),
        };
      },
    );

    const matrix = buildMatrix({
      power: {
        R: 'sidecar_only',
        SAS: 'sidecar_only',
        Python: 'sidecar_only',
        SPSS: 'sidecar_only',
      },
    });

    renderWithMatrix(
      matrix,
      <EquivalentCodeSidecar
        algorithmId="power"
        runId="run-1"
        datasetSha256={SAMPLE_SHA}
        releaseVersion={SAMPLE_VERSION}
      />,
    );

    await waitFor(() => {
      expect(screen.getByTestId('sidecar-snippet')).toHaveTextContent(
        '# R sidecar-only body',
      );
    });

    const notice = screen.getByTestId('sidecar-notice');
    // Notice names the active software (Requirement 6.4).
    expect(notice.textContent).toContain('R');
    expect(notice.textContent?.toLowerCase()).toContain('parity');

    // Copy is enabled because there is snippet text to copy.
    expect(screen.getByTestId('copy-to-clipboard-button')).not.toBeDisabled();
  });
});

// ---------------------------------------------------------------------------
// `live` cell behaviour
// ---------------------------------------------------------------------------

describe('EquivalentCodeSidecar — live coverage', () => {
  it('renders the snippet only, with no inline notice', async () => {
    const matrix = buildMatrix({
      tableone: { R: 'live', SAS: 'recorded', Python: 'live', SPSS: 'recorded' },
    });

    renderWithMatrix(
      matrix,
      <EquivalentCodeSidecar
        algorithmId="tableone"
        runId="run-1"
        datasetSha256={SAMPLE_SHA}
        releaseVersion={SAMPLE_VERSION}
      />,
    );

    await waitFor(() => {
      expect(screen.getByTestId('sidecar-snippet')).toHaveTextContent(
        '# R snippet body',
      );
    });

    expect(screen.queryByTestId('sidecar-notice')).not.toBeInTheDocument();
    expect(screen.queryByTestId('sidecar-placeholder')).not.toBeInTheDocument();
    expect(screen.getByTestId('copy-to-clipboard-button')).not.toBeDisabled();
  });
});

// ---------------------------------------------------------------------------
// Footer is unconditional
// ---------------------------------------------------------------------------

describe('EquivalentCodeSidecar — footer', () => {
  it('renders SHA256 + release version on every tab and every state', async () => {
    const matrix = buildMatrix({
      logistic: {
        R: 'none',
        SAS: 'recorded',
        Python: 'live',
        SPSS: 'sidecar_only',
      },
    });

    useSidecarSpy.mockImplementation(
      ({
        software,
        enabled = true,
      }: {
        algorithmId: string;
        software: 'R' | 'SAS' | 'Python' | 'SPSS';
        runId: string;
        enabled?: boolean;
      }) => {
        if (!enabled) return { loading: false };
        const cv: CoverageState =
          software === 'SPSS' ? 'sidecar_only' : 'live';
        return {
          loading: false,
          snippet: snippet(software, `# ${software} body\n`, cv),
        };
      },
    );

    renderWithMatrix(
      matrix,
      <EquivalentCodeSidecar
        algorithmId="logistic"
        runId="run-1"
        datasetSha256={SAMPLE_SHA}
        releaseVersion={SAMPLE_VERSION}
      />,
    );

    // R is `none` → placeholder. Footer must still be present.
    await waitFor(() => {
      expect(screen.getByTestId('sidecar-placeholder')).toBeInTheDocument();
    });
    {
      const footer = screen.getByTestId('sidecar-footer');
      expect(footer.textContent).toContain(SAMPLE_SHA);
      expect(footer.textContent).toContain(SAMPLE_VERSION);
    }

    // SAS → recorded snippet. Footer still present.
    act(() => {
      screen.getByTestId('sidecar-tab-SAS').click();
    });
    await waitFor(() => {
      expect(screen.getByTestId('sidecar-snippet')).toHaveTextContent(
        '# SAS body',
      );
    });
    {
      const footer = screen.getByTestId('sidecar-footer');
      expect(footer.textContent).toContain(SAMPLE_SHA);
      expect(footer.textContent).toContain(SAMPLE_VERSION);
    }

    // SPSS → sidecar_only. Footer still present.
    act(() => {
      screen.getByTestId('sidecar-tab-SPSS').click();
    });
    await waitFor(() => {
      expect(screen.getByTestId('sidecar-notice')).toBeInTheDocument();
    });
    {
      const footer = screen.getByTestId('sidecar-footer');
      expect(footer.textContent).toContain(SAMPLE_SHA);
      expect(footer.textContent).toContain(SAMPLE_VERSION);
    }
  });
});

// ---------------------------------------------------------------------------
// Lazy fetching: only the active tab's snippet is fetched.
// ---------------------------------------------------------------------------

describe('EquivalentCodeSidecar — lazy fetching', () => {
  it('only enables useSidecar for the active tab', async () => {
    const matrix = buildMatrix({
      tableone: { R: 'live', SAS: 'recorded', Python: 'live', SPSS: 'recorded' },
    });

    renderWithMatrix(
      matrix,
      <EquivalentCodeSidecar
        algorithmId="tableone"
        runId="run-1"
        datasetSha256={SAMPLE_SHA}
        releaseVersion={SAMPLE_VERSION}
      />,
    );

    await waitFor(() => {
      expect(screen.getByTestId('sidecar-snippet')).toHaveTextContent(
        '# R snippet body',
      );
    });

    // After the matrix has loaded, every call must be for the active
    // software, and only one software at a time should ever have
    // `enabled: true`.
    const enabledCalls = useSidecarSpy.mock.calls.filter(
      ([params]) => params.enabled === true,
    );
    const enabledSoftwares = new Set(
      enabledCalls.map(([params]) => params.software),
    );
    expect(enabledSoftwares.has('R')).toBe(true);
    expect(enabledSoftwares.has('SAS')).toBe(false);
    expect(enabledSoftwares.has('Python')).toBe(false);
    expect(enabledSoftwares.has('SPSS')).toBe(false);

    // Switch to Python. Now only Python should ever be enabled going
    // forward; previous-call totals for R are fine, but no NEW R calls
    // with `enabled: true` should appear after the switch.
    const callsBefore = useSidecarSpy.mock.calls.length;
    act(() => {
      screen.getByTestId('sidecar-tab-Python').click();
    });

    await waitFor(() => {
      expect(screen.getByTestId('sidecar-snippet')).toHaveTextContent(
        '# Python snippet body',
      );
    });

    const newCalls = useSidecarSpy.mock.calls.slice(callsBefore);
    for (const [params] of newCalls) {
      if (params.enabled === true) {
        expect(params.software).toBe('Python');
      }
    }
  });
});
