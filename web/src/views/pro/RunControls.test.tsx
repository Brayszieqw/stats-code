/**
 * Tests for RunControls.
 *
 * Validates: Requirements 7.4, 12.7
 */

import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { RunControls } from './RunControls';
import type { AnalysisResultMeta } from '../../api/types';

afterEach(() => {
  vi.unstubAllGlobals();
});

function stubFetch(impl: () => Promise<Response>) {
  vi.stubGlobal('fetch', vi.fn(impl));
}

const okResponse = (body: unknown) =>
  ({ ok: true, status: 200, statusText: 'ok', json: async () => body }) as Response;
const errResponse = (status: number, body: unknown) =>
  ({ ok: false, status, statusText: 'err', json: async () => body }) as Response;

const analysis: AnalysisResultMeta = {
  algorithm_id: 'model_linear',
  dataset_id: 'ds-1',
  dataset_sha256: null,
  columns: [],
  params: { outcome: 'y' },
  run_id: 'run-1',
  run_status: 'completed',
};

describe('RunControls (Requirements 7.4, 12.7)', () => {
  it('disables 运行 when there is no analysis (R7.5 / no runnable target)', () => {
    render(<RunControls sessionId="s1" analysis={null} />);
    expect(screen.getByLabelText('运行')).toBeDisabled();
  });

  it('disables 运行 when the session is read-only even with analysis (R9.3)', () => {
    render(<RunControls sessionId="s1" analysis={analysis} disabled />);
    expect(screen.getByLabelText('运行')).toBeDisabled();
  });

  it('runs and shows the success state (R12.7)', async () => {
    const result = { schema_version: '1.0', payload: {}, risk_signals: [] };
    const onRunComplete = vi.fn();
    stubFetch(async () => okResponse(result));
    render(<RunControls sessionId="s1" analysis={analysis} onRunComplete={onRunComplete} />);
    fireEvent.click(screen.getByLabelText('运行'));
    await waitFor(() => expect(screen.getByText('运行完成')).toBeInTheDocument());
    expect(onRunComplete).toHaveBeenCalledWith(result);
  });

  it('shows the error code/message on failure (R12.7)', async () => {
    stubFetch(async () => errResponse(422, { error_code: 'SkillInvalidArgs', message: '缺少参数' }));
    render(<RunControls sessionId="s1" analysis={analysis} />);
    fireEvent.click(screen.getByLabelText('运行'));
    await waitFor(() => expect(screen.getByText('SkillInvalidArgs')).toBeInTheDocument());
    expect(screen.getByText('缺少参数')).toBeInTheDocument();
  });

  it('hands a completed backend run to the workspace even if the code tab unmounts', async () => {
    let resolveFetch!: (response: Response) => void;
    const result = { schema_version: '1.0', payload: {}, risk_signals: [] };
    stubFetch(() => new Promise<Response>((resolve) => { resolveFetch = resolve; }));
    const onRunComplete = vi.fn();
    const { unmount } = render(
      <RunControls sessionId="s1" analysis={analysis} onRunComplete={onRunComplete} />,
    );

    fireEvent.click(screen.getByLabelText('运行'));
    unmount();
    resolveFetch(okResponse(result));

    await waitFor(() => expect(onRunComplete).toHaveBeenCalledWith(result));
  });
});
