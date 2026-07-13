import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { ResearchProtocolInput } from '../api/types';
import { ResearchProtocolDrawer } from './ResearchProtocolDrawer';

describe('ResearchProtocolDrawer', () => {
  it('loads the demo protocol and submits an approved 15-field card', async () => {
    const onSave = vi.fn(async (_input: ResearchProtocolInput) => {});
    render(
      <ResearchProtocolDrawer
        open
        protocol={null}
        onClose={vi.fn()}
        onSave={onSave}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: '加载演示协议' }));
    fireEvent.click(screen.getByRole('button', { name: '审批协议' }));

    await waitFor(() => expect(onSave).toHaveBeenCalledTimes(1));
    expect(onSave.mock.calls[0]?.[0]).toMatchObject({
      status: 'Approved',
      study_design: 'cross_sectional',
      outcome: 'disease（二分类疾病结局）',
      time_zero: '基线调查时点',
    });
    expect(Object.keys(onSave.mock.calls[0]?.[0] ?? {})).toHaveLength(16);
  });
});
