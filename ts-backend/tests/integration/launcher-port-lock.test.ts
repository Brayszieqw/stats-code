import { describe, it, expect, afterEach } from 'vitest';
import net from 'node:net';
import { mkdtempSync, writeFileSync, existsSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { launcher } from '@stats-code/engine';

const {
  scanFirstBindable,
  isPortReachable,
  AllPortsBusyError,
  tryAcquire,
  parseLock,
  serializeLock,
  newLockRecord,
  isLockAlive,
  isPidAlive,
  LockParseStaleError,
} = launcher;

const servers: net.Server[] = [];
const tmpDirs: string[] = [];

afterEach(() => {
  for (const s of servers.splice(0)) {
    try {
      s.close();
    } catch {
      /* ignore */
    }
  }
  for (const d of tmpDirs.splice(0)) {
    rmSync(d, { recursive: true, force: true });
  }
});

function freshTmp(): string {
  const d = mkdtempSync(join(tmpdir(), 'sc-lock-'));
  tmpDirs.push(d);
  return d;
}

describe('port scan', () => {
  it('binds the first free port and skips occupied ones', async () => {
    // Occupy a port, then scan a small range starting there.
    const occupied = await scanFirstBindable({ start: 8080, endExclusive: 8200 });
    servers.push(occupied);
    const base = (occupied.address() as net.AddressInfo).port;

    const next = await scanFirstBindable({ start: base, endExclusive: base + 20 });
    servers.push(next);
    const got = (next.address() as net.AddressInfo).port;

    expect(got).toBeGreaterThan(base);
    expect((next.address() as net.AddressInfo).address).toBe('127.0.0.1');
  });

  it('throws AllPortsBusy on an empty range', async () => {
    await expect(scanFirstBindable({ start: 8080, endExclusive: 8080 })).rejects.toBeInstanceOf(
      AllPortsBusyError,
    );
  });

  it('isPortReachable is true for a bound port and false for a closed one', async () => {
    const server = await scanFirstBindable({ start: 8090, endExclusive: 8200 });
    servers.push(server);
    const port = (server.address() as net.AddressInfo).port;
    expect(await isPortReachable(`http://127.0.0.1:${port}/`)).toBe(true);

    server.close();
    servers.splice(servers.indexOf(server), 1);
    // a port in the ephemeral-unused space should be unreachable
    expect(await isPortReachable('http://127.0.0.1:1/')).toBe(false);
  });
});

describe('lock parsing', () => {
  it('round-trips a lock record through serialize/parse', () => {
    const rec = newLockRecord(18432, 'http://127.0.0.1:8080/', '2025-01-15T10:23:11Z', 'prod');
    const back = parseLock(serializeLock(rec));
    expect(back).toEqual(rec);
  });

  it('serializes the Rust snake_case schema keys', () => {
    const rec = newLockRecord(1, 'http://127.0.0.1:8080/', 't', 'dev');
    const json = serializeLock(rec);
    for (const key of ['schema_version', 'pid', 'url', 'started_at', 'mode']) {
      expect(json).toContain(`"${key}"`);
    }
  });

  it('rejects a schema-version mismatch as stale', () => {
    const payload = JSON.stringify({ schema_version: 999, pid: 1, url: 'x', started_at: 't', mode: 'prod' });
    expect(() => parseLock(payload)).toThrow(LockParseStaleError);
  });

  it('rejects malformed JSON as stale', () => {
    expect(() => parseLock('{not json')).toThrow(LockParseStaleError);
  });
});

describe('isLockAlive', () => {
  it('requires both pid alive AND port reachable', async () => {
    expect(await isLockAlive(() => true, () => true)).toBe(true);
    expect(await isLockAlive(() => true, () => false)).toBe(false);
    expect(await isLockAlive(() => false, () => true)).toBe(false);
  });

  it('short-circuits the port probe when the pid is dead', async () => {
    let portProbed = false;
    const alive = await isLockAlive(
      () => false,
      () => {
        portProbed = true;
        return true;
      },
    );
    expect(alive).toBe(false);
    expect(portProbed).toBe(false);
  });
});

describe('tryAcquire state machine', () => {
  it('acquires when no lock file exists', async () => {
    const path = join(freshTmp(), 'running.lock');
    const outcome = await tryAcquire(path, () => true, () => true);
    expect(outcome.kind).toBe('acquired');
  });

  it('deletes a malformed lock file and acquires', async () => {
    const path = join(freshTmp(), 'running.lock');
    writeFileSync(path, 'not valid json');
    const outcome = await tryAcquire(path, () => true, () => true);
    expect(outcome.kind).toBe('acquired');
    expect(existsSync(path)).toBe(false);
  });

  it('returns existing when the recorded instance is alive', async () => {
    const path = join(freshTmp(), 'running.lock');
    const rec = newLockRecord(9999, 'http://127.0.0.1:8080/', 't', 'prod');
    writeFileSync(path, serializeLock(rec));
    const outcome = await tryAcquire(path, (pid) => pid === 9999, () => true);
    expect(outcome).toMatchObject({ kind: 'existing', pid: 9999, url: 'http://127.0.0.1:8080/' });
    expect(existsSync(path)).toBe(true); // alive lock preserved
  });

  it('treats pid-dead as stale, deletes, acquires (Req 9.5)', async () => {
    const path = join(freshTmp(), 'running.lock');
    writeFileSync(path, serializeLock(newLockRecord(9999, 'http://127.0.0.1:8080/', 't', 'prod')));
    const outcome = await tryAcquire(path, () => false, () => true);
    expect(outcome.kind).toBe('acquired');
    expect(existsSync(path)).toBe(false);
  });

  it('treats port-unreachable as stale even when pid alive (Req 9.5)', async () => {
    const path = join(freshTmp(), 'running.lock');
    writeFileSync(path, serializeLock(newLockRecord(9999, 'http://127.0.0.1:8080/', 't', 'prod')));
    const outcome = await tryAcquire(path, () => true, () => false);
    expect(outcome.kind).toBe('acquired');
    expect(existsSync(path)).toBe(false);
  });

  it('LockHandle.writeRunning persists and release() deletes', async () => {
    const path = join(freshTmp(), 'running.lock');
    const outcome = await tryAcquire(path, () => false, () => false);
    expect(outcome.kind).toBe('acquired');
    if (outcome.kind !== 'acquired') return;
    const rec = newLockRecord(42, 'http://127.0.0.1:8081/', '2025-06-01T12:00:00Z', 'dev');
    outcome.handle.writeRunning(rec);
    expect(existsSync(path)).toBe(true);
    expect(parseLock(readFileSync(path, 'utf8'))).toEqual(rec);
    outcome.handle.release();
    expect(existsSync(path)).toBe(false);
  });
});

describe('isPidAlive', () => {
  it('reports the current process as alive', () => {
    expect(isPidAlive(process.pid)).toBe(true);
  });
  it('reports an unused high pid as dead', () => {
    expect(isPidAlive(2 ** 30)).toBe(false);
  });
  it('rejects invalid pids', () => {
    expect(isPidAlive(0)).toBe(false);
    expect(isPidAlive(-1)).toBe(false);
  });
});
