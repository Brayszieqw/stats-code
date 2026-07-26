/**
 * Tests for SimpleSidebar.
 *
 * Validates: Requirements 2.1, 2.2, 9.6
 */

import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { SimpleSidebar } from './SimpleSidebar';
import { ApiError } from '../../api/client';
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

  it('deletes a history item through the wired delete action', async () => {
    const onDelete = vi.fn();
    const onSelect = vi.fn();
    render(
      <SimpleSidebar
        sessionList={makeList({ sessions: [summary] })}
        onNewSession={() => {}}
        onSelectSession={onSelect}
        onDeleteSession={onDelete}
      />,
    );
    const deleteButton = screen.getByLabelText('删除会话: 血压回归');
    fireEvent.click(deleteButton);
    expect(onDelete).toHaveBeenCalledWith('hist-1');
    expect(onSelect).not.toHaveBeenCalled();
    await waitFor(() => expect(deleteButton).not.toBeDisabled());
  });

  it('surfaces the structured API reason and reconciles the list when deletion fails', async () => {
    const refresh = vi.fn(async () => {});
    const onDelete = vi.fn(async () => {
      throw new ApiError(409, {
        error_code: 'SessionArchived',
        message: '会话已归档，仅支持只读访问',
      });
    });
    render(
      <SimpleSidebar
        sessionList={makeList({ sessions: [summary], refresh })}
        onNewSession={() => {}}
        onSelectSession={() => {}}
        onDeleteSession={onDelete}
      />,
    );

    fireEvent.click(screen.getByLabelText('删除会话: 血压回归'));

    expect(await screen.findByRole('alert')).toHaveTextContent('会话已归档，仅支持只读访问');
    await waitFor(() => expect(refresh).toHaveBeenCalledTimes(1));
  });

  it('uses a readable fallback for a non-Error rejection', async () => {
    const onDelete = vi.fn(() => Promise.reject('boom'));
    render(
      <SimpleSidebar
        sessionList={makeList({ sessions: [summary] })}
        onNewSession={() => {}}
        onSelectSession={() => {}}
        onDeleteSession={onDelete}
      />,
    );

    fireEvent.click(screen.getByLabelText('删除会话: 血压回归'));

    expect(await screen.findByRole('alert')).toHaveTextContent('删除会话失败，请稍后重试');
  });

  it('releases the deletion lock after failure so the user can retry', async () => {
    const onDelete = vi
      .fn<(_: string) => Promise<void>>()
      .mockRejectedValueOnce(new Error('暂时无法删除'))
      .mockResolvedValueOnce(undefined);
    render(
      <SimpleSidebar
        sessionList={makeList({ sessions: [summary] })}
        onNewSession={() => {}}
        onSelectSession={() => {}}
        onDeleteSession={onDelete}
      />,
    );

    const deleteButton = screen.getByLabelText('删除会话: 血压回归');
    fireEvent.click(deleteButton);
    expect(await screen.findByRole('alert')).toHaveTextContent('暂时无法删除');
    await waitFor(() => expect(deleteButton).not.toBeDisabled());

    fireEvent.click(deleteButton);
    await waitFor(() => expect(onDelete).toHaveBeenCalledTimes(2));
  });

  it('does not show an error after a successful deletion', async () => {
    const onDelete = vi.fn(async () => {});
    render(
      <SimpleSidebar
        sessionList={makeList({ sessions: [summary] })}
        onNewSession={() => {}}
        onSelectSession={() => {}}
        onDeleteSession={onDelete}
      />,
    );

    fireEvent.click(screen.getByLabelText('删除会话: 血压回归'));
    await waitFor(() => expect(onDelete).toHaveBeenCalledTimes(1));
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  });

  it('lets the user dismiss a deletion error', async () => {
    const onDelete = vi.fn(async () => {
      throw new Error('删除失败');
    });
    render(
      <SimpleSidebar
        sessionList={makeList({ sessions: [summary] })}
        onNewSession={() => {}}
        onSelectSession={() => {}}
        onDeleteSession={onDelete}
      />,
    );

    fireEvent.click(screen.getByLabelText('删除会话: 血压回归'));
    expect(await screen.findByRole('alert')).toHaveTextContent('删除失败');
    fireEvent.click(screen.getByRole('button', { name: /close/i }));
    await waitFor(() => expect(screen.queryByRole('alert')).not.toBeInTheDocument());
  });

  it('shows the same deletion feedback in the search-history drawer without duplicating alerts', async () => {
    const onDelete = vi.fn(async () => {
      throw new Error('搜索结果删除失败');
    });
    render(
      <SimpleSidebar
        sessionList={makeList({ sessions: [summary] })}
        onNewSession={() => {}}
        onSelectSession={() => {}}
        onDeleteSession={onDelete}
      />,
    );

    fireEvent.click(screen.getByLabelText('搜索'));
    const deleteButtons = screen.getAllByLabelText('删除会话: 血压回归');
    fireEvent.click(deleteButtons[deleteButtons.length - 1]!);

    const alerts = await screen.findAllByRole('alert');
    expect(alerts).toHaveLength(1);
    expect(alerts[0]!).toHaveTextContent('搜索结果删除失败');
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

  it('opens search as a real history drawer and selects a result', () => {
    const onSelect = vi.fn();
    render(
      <SimpleSidebar
        sessionList={makeList({ sessions: [summary] })}
        onNewSession={() => {}}
        onSelectSession={onSelect}
      />,
    );

    fireEvent.click(screen.getByLabelText('搜索'));
    expect(screen.getByPlaceholderText('输入会话标题关键词')).toBeInTheDocument();
    fireEvent.click(screen.getByLabelText(`搜索结果: ${summary.title}`));
    expect(onSelect).toHaveBeenCalledWith(summary.id);
  });

  it('opens plugin actions and delegates dataset upload', () => {
    const onOpenDatasetUpload = vi.fn();
    render(
      <SimpleSidebar
        sessionList={makeList()}
        onNewSession={() => {}}
        onSelectSession={() => {}}
        onOpenDatasetUpload={onOpenDatasetUpload}
      />,
    );

    fireEvent.click(screen.getByLabelText('插件'));
    fireEvent.click(screen.getByText('数据集上传与选择'));
    expect(onOpenDatasetUpload).toHaveBeenCalled();
  });

  it('opens analysis templates and sends a selected prompt', () => {
    const onUseTemplate = vi.fn();
    render(
      <SimpleSidebar
        sessionList={makeList()}
        onNewSession={() => {}}
        onSelectSession={() => {}}
        onUseTemplate={onUseTemplate}
      />,
    );

    fireEvent.click(screen.getByLabelText('分析模板'));
    fireEvent.click(screen.getByText('线性回归'));
    expect(onUseTemplate).toHaveBeenCalledWith(expect.stringContaining('线性关系'));
  });

  it('opens the product capability boundary from the persistent footer', () => {
    render(<SimpleSidebar sessionList={makeList()} onNewSession={() => {}} onSelectSession={() => {}} />);

    fireEvent.click(screen.getByRole('button', { name: '关于与能力边界' }));
    expect(screen.getByRole('article', { name: '能力边界' })).toBeInTheDocument();
    expect(screen.getByText(/当前不支持：PSM/)).toBeInTheDocument();
  });
});
