/**
 * Tests for ExplorerPanel.
 *
 * Validates: Requirements 5.1, 5.4, 5.5
 */

import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { ExplorerPanel } from './ExplorerPanel';
import type { DatasetSummary } from '../../api/types';

function makeDataset(overrides: Partial<DatasetSummary> = {}): DatasetSummary {
  return {
    dataset_id: 'ds-1',
    file_name: 'cohort.csv',
    size_bytes: 1024,
    encoding: 'Utf8',
    row_count: 120,
    columns: [
      { name: 'age', inferred_type: 'Numeric', missing_count: 0 },
      { name: 'group', inferred_type: 'Categorical', missing_count: 0 },
    ],
    uploaded_at: '2026-01-01T00:00:00Z',
    sha256: null,
    ...overrides,
  };
}

describe('ExplorerPanel (Requirements 5.1, 5.4, 5.5)', () => {
  it('renders an empty state when there are no datasets (R5.5)', () => {
    render(
      <ExplorerPanel
        datasets={[]}
        sessionId="s1"
        selectedDatasetId={null}
        onSelect={() => {}}
        onUploadComplete={() => {}}
      />,
    );
    expect(screen.getByText('暂无数据集')).toBeInTheDocument();
  });

  it('lists datasets with file name and row/column counts (R5.1)', () => {
    render(
      <ExplorerPanel
        datasets={[makeDataset()]}
        sessionId="s1"
        selectedDatasetId={null}
        onSelect={() => {}}
        onUploadComplete={() => {}}
      />,
    );
    expect(screen.getByText('cohort.csv')).toBeInTheDocument();
    expect(screen.getByText('120 行')).toBeInTheDocument();
    expect(screen.getByText('2 列')).toBeInTheDocument();
  });

  it('selects a dataset on click and deselects when clicking the active one (R5.4)', () => {
    const onSelect = vi.fn();
    const ds = makeDataset();
    const { rerender } = render(
      <ExplorerPanel
        datasets={[ds]}
        sessionId="s1"
        selectedDatasetId={null}
        onSelect={onSelect}
        onUploadComplete={() => {}}
      />,
    );
    fireEvent.click(screen.getByLabelText('数据集: cohort.csv'));
    expect(onSelect).toHaveBeenCalledWith(ds);

    // When already selected, clicking deselects (passes null).
    rerender(
      <ExplorerPanel
        datasets={[ds]}
        sessionId="s1"
        selectedDatasetId="ds-1"
        onSelect={onSelect}
        onUploadComplete={() => {}}
      />,
    );
    fireEvent.click(screen.getByLabelText('数据集: cohort.csv'));
    expect(onSelect).toHaveBeenLastCalledWith(null);
  });

  it('disables selection and upload when archived (R9.3)', () => {
    const onSelect = vi.fn();
    render(
      <ExplorerPanel
        datasets={[makeDataset()]}
        sessionId="s1"
        selectedDatasetId={null}
        onSelect={onSelect}
        onUploadComplete={() => {}}
        disabled
      />,
    );
    expect(screen.getByLabelText('上传数据集')).toBeDisabled();
    fireEvent.click(screen.getByLabelText('数据集: cohort.csv'));
    expect(onSelect).not.toHaveBeenCalled();
  });
});
