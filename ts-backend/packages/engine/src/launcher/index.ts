// launcher/ — port scan, lock file, child supervision, browser, llm config
// (Phase 1, tasks 3.6, 3.7).

export interface LockFile {
  schemaVersion: number;
  pid: number;
  url: string;
  startedAt: string;
  mode: 'prod' | 'dev';
}

export const PORT_RANGE = { min: 8080, max: 8200 } as const;
export const LOOPBACK = '127.0.0.1' as const;
