import { describe, it, expect, afterEach } from 'vitest';
import { spawn } from 'node:child_process';
import { launcher } from '@stats-code/engine';

const { ChildSupervisor } = launcher;

const supervisors: InstanceType<typeof ChildSupervisor>[] = [];

afterEach(() => {
  for (const s of supervisors.splice(0)) {
    s.dispose();
  }
});

function makeSupervisor(opts?: ConstructorParameters<typeof ChildSupervisor>[0]) {
  const s = new ChildSupervisor(opts);
  supervisors.push(s);
  return s;
}

/** Spawn a node child that sleeps, so we can verify termination. */
function spawnSleeper() {
  return spawn(process.execPath, ['-e', 'setTimeout(() => {}, 60000)'], {
    stdio: 'ignore',
  });
}

describe('ChildSupervisor', () => {
  it('tracks a spawned child and reports the count', () => {
    const sup = makeSupervisor();
    const child = spawnSleeper();
    sup.track(child);
    expect(sup.trackedCount).toBe(1);
  });

  it('untracks a child when it exits naturally', async () => {
    const sup = makeSupervisor();
    const child = spawn(process.execPath, ['-e', 'process.exit(0)'], { stdio: 'ignore' });
    sup.track(child);
    await new Promise<void>((resolve) => child.once('exit', () => setTimeout(resolve, 20)));
    expect(sup.trackedCount).toBe(0);
  });

  it('terminateAll kills tracked children (signal fallback)', async () => {
    const sup = makeSupervisor();
    const child = spawnSleeper();
    sup.track(child);
    const exited = new Promise<void>((resolve) => child.once('exit', () => resolve()));
    sup.terminateAll();
    await exited;
    expect(child.killed || child.exitCode !== null || child.signalCode !== null).toBe(true);
    expect(sup.trackedCount).toBe(0);
  });

  it('assigns children to the Job Object when a factory is provided', () => {
    const assigned: number[] = [];
    let closed = false;
    const sup = makeSupervisor({
      jobFactory: () => ({
        assign: (pid: number) => {
          assigned.push(pid);
          return true;
        },
        close: () => {
          closed = true;
        },
      }),
    });
    const child = spawnSleeper();
    sup.track(child);
    expect(assigned).toContain(child.pid);
    sup.terminateAll();
    expect(closed).toBe(true);
  });

  it('degrades to the signal fallback with a warning when the job factory returns null', () => {
    const warnings: string[] = [];
    const sup = makeSupervisor({
      jobFactory: () => null,
      warn: (m) => warnings.push(m),
    });
    const child = spawnSleeper();
    sup.track(child);
    expect(warnings.some((w) => w.includes('fallback'))).toBe(true);
    sup.terminateAll();
  });

  it('degrades with a warning when the job factory throws', () => {
    const warnings: string[] = [];
    const sup = makeSupervisor({
      jobFactory: () => {
        throw new Error('FFI unavailable');
      },
      warn: (m) => warnings.push(m),
    });
    const child = spawnSleeper();
    sup.track(child);
    expect(warnings.some((w) => w.includes('FFI unavailable'))).toBe(true);
    sup.terminateAll();
  });

  it('terminateAll is idempotent', () => {
    const sup = makeSupervisor();
    const child = spawnSleeper();
    sup.track(child);
    sup.terminateAll();
    expect(() => sup.terminateAll()).not.toThrow();
    expect(sup.trackedCount).toBe(0);
  });
});
