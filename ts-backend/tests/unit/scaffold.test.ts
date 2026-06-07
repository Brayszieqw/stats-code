import { describe, it, expect } from 'vitest';
import { ALGORITHM_IDS, FORBIDDEN_RUNTIMES } from '@stats-code/engine';

describe('scaffold', () => {
  it('exposes all 17 Output_Level_Algorithm ids', () => {
    expect(ALGORITHM_IDS).toHaveLength(17);
  });

  it('declares the forbidden runtimes', () => {
    expect(FORBIDDEN_RUNTIMES).toContain('rscript');
    expect(FORBIDDEN_RUNTIMES).toContain('python');
  });
});
