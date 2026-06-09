/**
 * Tests for SimpleSidebar.
 *
 * Validates: Requirements 2.1, 2.2, 9.6
 */

import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { SimpleSidebar } from './SimpleSidebar';
import type { UseSessionListReturn } from '../../hooks/useSessionList';
import type { SessionSummary } from '../../api/types';

function makeList(overrides: Partial<UseSessionListReturn> = {}): UseSessionListReturn {
  return {
    sessions: [],
    loading: false,
    error: null,
    refresh: vi.fn(async () => {}),
    ...overrides,
  };
}

const summary: SessionSummary = {
  id: 'hist-1',
  status: 'Active',
  created_at: '2026-01-01T00:00:00Z',
  last_active_at: '2026-01-02T00:00:00Z',
  message_count: 4,
  title: '血压回归',
  dataset_count: 1,
};

describe('SimpleSidebar (Requirements 2.1, 2.2, 9.6)', () => {
  it('renders the entry buttons (new/search/plugins/automation)', () => {
    render(<SimpleSidebar sessionList={makeList()} onNewSession={() => {}} onSelectSession={() => {}} />);
    expect(screen.getByLabelText('新对话')).toBeInTheDocument();
    expect(screen.getByLabelText('搜索')).toBeInTheDocument();
    expect(screen.getByLabelText('插件')).toBeInTheDocument();
    expect(screen.getByLabelText('自动化')).toBeInTheDocument();
  });

  it('renders history items and selects one on click (R9.6)', () => {
    const onSelect = vi.fn();
    render(
      <SimpleSidebar sessionList={makeList({ sessions: [summary] })} onNewSession={() => {}} onSelectSession={onSelect} />,
    );
    fireEvent.click(screen.getByLabelText('历史会话: 血压回归'));
    expect(onSelect).toHaveBeenCalledWith('hist-1');
  });

  it('shows a non-blocking placeholder when history fails to load', () => {
    render(
      <SimpleSidebar sessionList={makeList({ error: '网络异常' })} onNewSession={() => {}} onSelectSession={() => {}} />,
    );
    expect(screen.getByText(/历史会话加载失败/)).toBeInTheDocument();
  });

  it('fires onNewSession when the new-conversation button is clicked', () => {
    const onNew = vi.fn();
    render(<SimpleSidebar sessionList={makeList()} onNewSession={onNew} onSelectSession={() => {}} />);
    fireEvent.click(screen.getByLabelText('新对话'));
    expect(onNew).toHaveBeenCalled();
  });
});
