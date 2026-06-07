// launcher/lock.ts — single-instance lock file (task 3.6).
// Transcribed from crates/stats-code/src/launcher/lock.rs.
//
// Lock state machine (Requirement 9.2-9.5):
//   - file absent                                  → Acquired
//   - parse fails (corrupt / schema mismatch)      → delete, Acquired
//   - parsed & alive (pid alive AND port reachable)→ Existing { url, pid }
//   - parsed & stale (pid dead OR port unreachable)→ delete, Acquired
//
// "Alive" requires BOTH evidences (pid alive AND port reachable). The pid/port
// probes are injected so tests need no real OS processes or sockets. The probe
// is short-circuited: the port is probed only when the pid is alive.

import { readFileSync, writeFileSync, rmSync } from 'node:fs';

export const LOCK_SCHEMA_VERSION = 1;

export interface LockFileV1 {
  schemaVersion: number;
  pid: number;
  url: string;
  startedAt: string;
  mode: 'prod' | 'dev';
}

export class LockParseStaleError extends Error {
  readonly kind = 'parse_stale';
  constructor(reason: string) {
    super(`stale lock file: ${reason}`);
    this.name = 'LockParseStaleError';
  }
}

/** Build a lock record with the current schema version. */
export function newLockRecord(
  pid: number,
  url: string,
  startedAt: string,
  mode: 'prod' | 'dev',
): LockFileV1 {
  return { schemaVersion: LOCK_SCHEMA_VERSION, pid, url, startedAt, mode };
}

/**
 * Serialize to the on-disk JSON shape. The Rust schema keys are snake_case
 * (schema_version, pid, url, started_at, mode), so we map explicitly to keep
 * the lock file format byte-compatible with the Rust launcher.
 */
export function serializeLock(record: LockFileV1): string {
  return JSON.stringify({
    schema_version: record.schemaVersion,
    pid: record.pid,
    url: record.url,
    started_at: record.startedAt,
    mode: record.mode,
  });
}

/** Parse a lock file; throws LockParseStaleError on corruption or version mismatch. */
export function parseLock(text: string): LockFileV1 {
  let raw: unknown;
  try {
    raw = JSON.parse(text);
  } catch (err) {
    throw new LockParseStaleError(`malformed lock file: ${(err as Error).message}`);
  }
  const obj = raw as Record<string, unknown>;
  const schemaVersion = obj['schema_version'];
  if (schemaVersion !== LOCK_SCHEMA_VERSION) {
    throw new LockParseStaleError(
      `unsupported lock schema_version ${String(schemaVersion)} (expected ${LOCK_SCHEMA_VERSION})`,
    );
  }
  const mode = obj['mode'] === 'dev' ? 'dev' : 'prod';
  return {
    schemaVersion: LOCK_SCHEMA_VERSION,
    pid: Number(obj['pid']),
    url: String(obj['url']),
    startedAt: String(obj['started_at']),
    mode,
  };
}

/** A live lock requires BOTH evidences; the port probe runs only if pid is alive. */
export async function isLockAlive(
  pidAlive: () => boolean | Promise<boolean>,
  portReachable: () => boolean | Promise<boolean>,
): Promise<boolean> {
  if (!(await pidAlive())) {
    return false;
  }
  return await portReachable();
}

export type AcquireOutcome =
  | { kind: 'acquired'; handle: LockHandle }
  | { kind: 'existing'; url: string; pid: number };

/**
 * RAII-style lock handle. After acquiring, call writeRunning to persist this
 * instance's pid/url, then release() on shutdown to delete the file. Only
 * deletes the file if this handle actually wrote it.
 */
export class LockHandle {
  private written = false;
  constructor(public readonly path: string) {}

  writeRunning(record: LockFileV1): void {
    writeFileSync(this.path, serializeLock(record));
    this.written = true;
  }

  /** Delete the lock file if this handle wrote it. Idempotent. */
  release(): void {
    if (this.written) {
      try {
        rmSync(this.path, { force: true });
      } catch {
        // best-effort cleanup
      }
      this.written = false;
    }
  }
}

/**
 * Attempt to acquire the single-instance lock at `path`.
 *
 * Injectable probes:
 *   - pidAlive(pid): is the recorded process still running?
 *   - portReachable(url): does the recorded URL accept a TCP connection?
 */
export async function tryAcquire(
  path: string,
  pidAlive: (pid: number) => boolean | Promise<boolean>,
  portReachable: (url: string) => boolean | Promise<boolean>,
): Promise<AcquireOutcome> {
  // 1. file absent → acquire
  let content: string;
  try {
    content = readFileSync(path, 'utf8');
  } catch (err) {
    if ((err as NodeJS.ErrnoException).code === 'ENOENT') {
      return { kind: 'acquired', handle: new LockHandle(path) };
    }
    throw err;
  }

  // 2. parse failure → stale, delete and acquire
  let parsed: LockFileV1;
  try {
    parsed = parseLock(content);
  } catch (err) {
    if (err instanceof LockParseStaleError) {
      rmSync(path, { force: true });
      return { kind: 'acquired', handle: new LockHandle(path) };
    }
    throw err;
  }

  // 3. liveness: pid alive AND port reachable
  const alive = await isLockAlive(
    () => pidAlive(parsed.pid),
    () => portReachable(parsed.url),
  );

  if (alive) {
    return { kind: 'existing', url: parsed.url, pid: parsed.pid };
  }

  // 4. stale → delete and acquire
  rmSync(path, { force: true });
  return { kind: 'acquired', handle: new LockHandle(path) };
}

/** Probe whether a process with the given pid is alive (signal 0). */
export function isPidAlive(pid: number): boolean {
  if (!Number.isInteger(pid) || pid <= 0) {
    return false;
  }
  try {
    process.kill(pid, 0);
    return true;
  } catch (err) {
    // EPERM means the process exists but we cannot signal it → still alive.
    return (err as NodeJS.ErrnoException).code === 'EPERM';
  }
}
