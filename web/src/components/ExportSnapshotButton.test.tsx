/**
 * Tests for `<ExportSnapshotButton>`.
 *
 * Covers:
 *   - Disabled when `runStatus !== 'completed'` (UX gate; server is
 *     authoritative — Requirement 7.1).
 *   - Clicking a completed-run button issues `POST /api/snapshot/export` and
 *     renders a success toast with the snapshot path (Requirement 7.1).
 *   - On 409 (`RunNotCompleted`) the error toast names the actual run
 *     status (Requirement 7.8).
 *   - On 413 (`PayloadTooLarge`) the error toast names both the measured
 *     bytes and the 50 MB ceiling (Requirement 7.7).
 *   - On a generic 5xx the error toast surfaces the server-supplied
 *     `error_code` token.
 *
 * Validates: Requirements 7.1, 7.7, 7.8
 */

import {
  describe,
  it,
  expect,
  vi,
  beforeEach,
  afterEach,
} from 'vitest';
import {
  render,
  screen,
  act,
  cleanup,
  waitFor,
} from '@testing-library/react';

import { ExportSnapshotButton } from './ExportSnapshotButton';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });
}

afterEach(() => {
  cleanup();
});

// ---------------------------------------------------------------------------
// Disabled UX gate
// ---------------------------------------------------------------------------

describe('ExportSnapshotButton — disabled UX gate', () => {
  it('is disabled when runStatus is "running"', () => {
    const fetchImpl = vi.fn();
    render(
      <ExportSnapshotButton
        runId="run-1"
        destination="C:/tmp/run-1.zip"
        runStatus="running"
        fetchImpl={fetchImpl as unknown as typeof fetch}
      />,
    );
    expect(screen.getByTestId('export-snapshot-button')).toBeDisabled();
    expect(fetchImpl).not.toHaveBeenCalled();
  });

  it('is disabled when runStatus is "failed"', () => {
    const fetchImpl = vi.fn();
    render(
      <ExportSnapshotButton
        runId="run-1"
        destination="C:/tmp/run-1.zip"
        runStatus="failed"
        fetchImpl={fetchImpl as unknown as typeof fetch}
      />,
    );
    expect(screen.getByTestId('export-snapshot-button')).toBeDisabled();
  });

  it('is enabled when runStatus is exactly "completed"', () => {
    const fetchImpl = vi.fn();
    render(
      <ExportSnapshotButton
        runId="run-1"
        destination="C:/tmp/run-1.zip"
        runStatus="completed"
        fetchImpl={fetchImpl as unknown as typeof fetch}
      />,
    );
    expect(screen.getByTestId('export-snapshot-button')).not.toBeDisabled();
  });
});

// ---------------------------------------------------------------------------
// Success path
// ---------------------------------------------------------------------------

describe('ExportSnapshotButton — success', () => {
  it('issues the request and renders a success toast naming the snapshot path', async () => {
    const fetchImpl = vi.fn(
      async (_input: RequestInfo | URL, init?: RequestInit) => {
        const decoded = JSON.parse(init?.body as string) as Record<
          string,
          unknown
        >;
        expect(decoded).toEqual({
          run_id: 'run-9',
          destination: 'C:/exports/run-9.zip',
        });
        return jsonResponse({
          snapshot_path: 'C:/exports/run-9.zip',
          sha256: 'a'.repeat(64),
        });
      },
    );

    render(
      <ExportSnapshotButton
        runId="run-9"
        destination="C:/exports/run-9.zip"
        runStatus="completed"
        fetchImpl={fetchImpl as unknown as typeof fetch}
      />,
    );

    await act(async () => {
      screen.getByTestId('export-snapshot-button').click();
    });

    await waitFor(() => {
      expect(
        screen.getByTestId('export-snapshot-toast-success'),
      ).toBeInTheDocument();
    });

    const toast = screen.getByTestId('export-snapshot-toast-success');
    expect(toast.textContent).toContain('C:/exports/run-9.zip');
    expect(toast.getAttribute('role')).toBe('status');
  });
});

// ---------------------------------------------------------------------------
// 409 — RunNotCompleted (Requirement 7.8)
// ---------------------------------------------------------------------------

describe('ExportSnapshotButton — 409 RunNotCompleted toast', () => {
  it('renders an error toast that names the actual run status', async () => {
    // Use a non-"completed" runStatus is impossible here because the button
    // would be disabled. To exercise the 7.8 path we pretend the SPA briefly
    // believes the run is completed (e.g. status flipped in flight) and the
    // server is the authoritative arbiter that refuses with HTTP 409.
    const fetchImpl = vi.fn(async () =>
      jsonResponse(
        {
          error_code: 'RunNotCompleted',
          message:
            'run status is running; snapshot export requires completed',
          actual_status: 'running',
        },
        409,
      ),
    );

    render(
      <ExportSnapshotButton
        runId="run-10"
        destination="C:/exports/run-10.zip"
        runStatus="completed"
        fetchImpl={fetchImpl as unknown as typeof fetch}
      />,
    );

    await act(async () => {
      screen.getByTestId('export-snapshot-button').click();
    });

    await waitFor(() => {
      expect(
        screen.getByTestId('export-snapshot-toast-error'),
      ).toBeInTheDocument();
    });

    const toast = screen.getByTestId('export-snapshot-toast-error');
    expect(toast.getAttribute('data-error-code')).toBe('RunNotCompleted');
    expect(toast.textContent).toContain('running');
    expect(toast.getAttribute('role')).toBe('alert');
  });
});

// ---------------------------------------------------------------------------
// 413 — PayloadTooLarge (Requirement 7.7)
// ---------------------------------------------------------------------------

describe('ExportSnapshotButton — 413 PayloadTooLarge toast', () => {
  it('renders an error toast that names both measured bytes and the ceiling', async () => {
    const measured = 60 * 1024 * 1024;
    const ceiling = 50 * 1024 * 1024;
    const fetchImpl = vi.fn(async () =>
      jsonResponse(
        {
          error_code: 'PayloadTooLarge',
          message:
            'artifact payload 62914560 bytes exceeds 52428800 byte ceiling',
          measured_bytes: measured,
          ceiling_bytes: ceiling,
        },
        413,
      ),
    );

    render(
      <ExportSnapshotButton
        runId="run-11"
        destination="C:/exports/run-11.zip"
        runStatus="completed"
        fetchImpl={fetchImpl as unknown as typeof fetch}
      />,
    );

    await act(async () => {
      screen.getByTestId('export-snapshot-button').click();
    });

    await waitFor(() => {
      expect(
        screen.getByTestId('export-snapshot-toast-error'),
      ).toBeInTheDocument();
    });

    const toast = screen.getByTestId('export-snapshot-toast-error');
    expect(toast.getAttribute('data-error-code')).toBe('PayloadTooLarge');
    // Both numbers must appear in the user-facing copy.
    expect(toast.textContent).toContain(String(measured));
    expect(toast.textContent).toContain(String(ceiling));
  });
});

// ---------------------------------------------------------------------------
// Generic error
// ---------------------------------------------------------------------------

describe('ExportSnapshotButton — generic error toast', () => {
  it('shows the server-supplied error_code on a 5xx body', async () => {
    const fetchImpl = vi.fn(async () =>
      jsonResponse(
        { error_code: 'InternalError', message: 'manifest hash mismatch' },
        500,
      ),
    );

    render(
      <ExportSnapshotButton
        runId="run-12"
        destination="C:/exports/run-12.zip"
        runStatus="completed"
        fetchImpl={fetchImpl as unknown as typeof fetch}
      />,
    );

    await act(async () => {
      screen.getByTestId('export-snapshot-button').click();
    });

    await waitFor(() => {
      expect(
        screen.getByTestId('export-snapshot-toast-error'),
      ).toBeInTheDocument();
    });

    const toast = screen.getByTestId('export-snapshot-toast-error');
    expect(toast.getAttribute('data-error-code')).toBe('InternalError');
    expect(toast.textContent).toContain('InternalError');
    expect(toast.textContent).toContain('manifest hash mismatch');
  });
});

// ---------------------------------------------------------------------------
// Loading flag (defence in depth)
// ---------------------------------------------------------------------------

describe('ExportSnapshotButton — loading flag', () => {
  beforeEach(() => {
    vi.useRealTimers();
  });

  it('disables the button while the request is in flight, then re-enables it', async () => {
    let resolveFetch: ((value: Response) => void) | null = null;
    const fetchImpl = vi.fn(
      () =>
        new Promise<Response>((resolve) => {
          resolveFetch = resolve;
        }),
    );

    render(
      <ExportSnapshotButton
        runId="run-13"
        destination="C:/exports/run-13.zip"
        runStatus="completed"
        fetchImpl={fetchImpl as unknown as typeof fetch}
      />,
    );

    const button = screen.getByTestId('export-snapshot-button');
    expect(button).not.toBeDisabled();

    act(() => {
      button.click();
    });

    await waitFor(() => {
      expect(button).toBeDisabled();
    });
    expect(button.textContent).toMatch(/exporting/i);

    await act(async () => {
      resolveFetch?.(
        jsonResponse({
          snapshot_path: 'C:/exports/run-13.zip',
          sha256: 'b'.repeat(64),
        }),
      );
      await Promise.resolve();
    });

    await waitFor(() => {
      expect(button).not.toBeDisabled();
    });
  });
});
