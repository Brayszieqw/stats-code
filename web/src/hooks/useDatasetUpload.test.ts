import { describe, expect, it } from 'vitest';
import { ACCEPTED_EXTENSIONS, isAcceptedFile } from './useDatasetUpload';

describe('useDatasetUpload file gates', () => {
  it('accepts backend-supported text datasets only', () => {
    expect(ACCEPTED_EXTENSIONS.has('.csv')).toBe(true);
    expect(ACCEPTED_EXTENSIONS.has('.tsv')).toBe(true);
    expect(isAcceptedFile(new File(['a,b\n1,2\n'], 'data.csv'))).toBe(true);
    expect(isAcceptedFile(new File(['a\tb\n1\t2\n'], 'data.tsv'))).toBe(true);
    expect(isAcceptedFile(new File(['x'], 'workbook.xlsx'))).toBe(false);
    expect(isAcceptedFile(new File(['x'], 'legacy.xls'))).toBe(false);
  });
});
