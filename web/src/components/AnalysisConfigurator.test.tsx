import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { DatasetSummary } from '../api/types';
import { AnalysisConfigurator } from './AnalysisConfigurator';

const summary: DatasetSummary = {
  dataset_id: 'dataset-1',
  file_name: 'demo_cohort.csv',
  size_bytes: 100,
  encoding: 'Utf8',
  row_count: 240,
  uploaded_at: '2026-07-12T00:00:00Z',
  sha256: 'abc',
  columns: [
    { name: 'disease', inferred_type: 'Numeric', missing_count: 0 },
    { name: 'age', inferred_type: 'Numeric', missing_count: 0 },
    { name: 'sex', inferred_type: 'String', missing_count: 0 },
  ],
};

describe('AnalysisConfigurator', () => {
  it(
    'allows numeric binary fields to be selected as grouping variables',
    async () => {
      render(<AnalysisConfigurator summary={summary} onSubmit={vi.fn()} />);

      const groupField = await screen.findByRole('combobox', { name: /分组比较变量/ });
      fireEvent.mouseDown(groupField);

      expect(await screen.findByText('disease (Numeric)')).toBeInTheDocument();
    },
    15_000,
  );
});


