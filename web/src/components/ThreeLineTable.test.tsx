import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { ThreeLineTable } from './ThreeLineTable';
import type { SkillResult } from '../api/types';

describe('ThreeLineTable engine tableone payload', () => {
  it('renders continuous and categorical rows from payload.groups', () => {
    const skillResult: SkillResult = {
      schema_version: '1.0',
      payload: {
        strata: null,
        continuous: ['age', 'bmi'],
        categorical: ['sex'],
        groups: [
          {
            label: 'Overall',
            n: 240,
            continuous: [
              {
                variable: 'age',
                type: 'continuous',
                n: 240,
                missing: 0,
                mean: 49.86,
                sd: 12.95,
                median: 50,
                q1: 39,
                q3: 61,
              },
              {
                variable: 'bmi',
                type: 'continuous',
                n: 240,
                missing: 0,
                mean: 26.9,
                sd: 3.47,
                median: 26.85,
                q1: 23.9,
                q3: 29.9,
              },
            ],
            categorical: [
              {
                variable: 'sex',
                type: 'categorical',
                n: 240,
                missing: 0,
                levels: [
                  { level: 'female', count: 120, percent: 50 },
                  { level: 'male', count: 120, percent: 50 },
                ],
              },
            ],
          },
        ],
      },
      risk_signals: [],
    };

    render(<ThreeLineTable skillResult={skillResult} />);
    expect(screen.getByLabelText('Table One 三线表')).toBeInTheDocument();
    expect(screen.getByText('Overall (N=240)')).toBeInTheDocument();
    expect(screen.getByText('age')).toBeInTheDocument();
    expect(screen.getByText('bmi')).toBeInTheDocument();
    expect(screen.getByText('sex')).toBeInTheDocument();
    expect(screen.getByText('female')).toBeInTheDocument();
    expect(screen.getAllByText(/120 \(50\.0%\)/).length).toBeGreaterThanOrEqual(1);
    expect(screen.getByText(/49\.860/)).toBeInTheDocument();
  });

  it('labels regression coefficients from analysis params when term is missing', () => {
    const skillResult: SkillResult = {
      schema_version: '1.0',
      payload: {
        coefficients: [
          { index: 0, estimate: 26.7, stdError: 0.9, pValue: 0.001, ciLower: 24, ciUpper: 28 },
          { index: 1, estimate: 0.003, stdError: 0.017, pValue: 0.86, ciLower: -0.03, ciUpper: 0.04 },
        ],
      },
      risk_signals: [],
      analysis: {
        algorithm_id: 'linear',
        dataset_id: 'ds-1',
        dataset_sha256: null,
        columns: [],
        params: { outcome: 'bmi', predictors: ['age'] },
        run_id: 'run-1',
        run_status: 'completed',
      },
    };

    render(<ThreeLineTable skillResult={skillResult} />);
    expect(screen.getByText('(Intercept)')).toBeInTheDocument();
    expect(screen.getByText('age')).toBeInTheDocument();
  });
});
