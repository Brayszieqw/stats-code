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

export {
  type ScanRange,
  DEFAULT_RANGE,
  scanFirstBindable,
  isPortReachable,
  AllPortsBusyError,
} from './port.js';

export {
  LOCK_SCHEMA_VERSION,
  type LockFileV1,
  type AcquireOutcome,
  LockHandle,
  LockParseStaleError,
  newLockRecord,
  serializeLock,
  parseLock,
  isLockAlive,
  tryAcquire,
  isPidAlive,
} from './lock.js';

export {
  APP_DIR_NAME,
  LOCK_FILE_NAME,
  CONFIG_FILE_NAME,
  AppDataUnavailableError,
  configBaseDir,
  appDataDir,
  appDataDirIn,
  lockFilePath,
  configFilePath,
  ensureAppDataDir,
} from './paths.js';

export {
  ChildSupervisor,
  type JobObjectHandle,
  type JobObjectFactory,
} from './supervisor.js';

export { createJobObject } from './job_object.js';
