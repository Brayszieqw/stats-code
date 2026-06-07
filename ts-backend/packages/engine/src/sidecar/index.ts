// sidecar/ — deterministic equivalent-code snippet generator (Phase 6, task 13.3).

export interface Snippet {
  language: 'R' | 'SAS' | 'Python' | 'SPSS';
  body: string;
  copyable: boolean;
}
