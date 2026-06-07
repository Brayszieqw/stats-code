// launcher/supervisor.ts — child-process supervision + guaranteed cleanup
// (Phase 1, task 3.7). Transcribed in spirit from
// crates/stats-code/src/launcher/process_guard.rs.
//
// The Rust backend used a Win32 Job Object with KILL_ON_JOB_CLOSE for
// kernel-enforced teardown. Node has no built-in equivalent, so the design
// specifies a layered strategy (design.md §"Process lifecycle"):
//
//   1. Primary — Win32 Job Object via a koffi FFI shim (pure FFI, no compiled
//      addon, SEA-compatible). Created lazily; if koffi or the Job Object is
//      unavailable, we log a warning and fall back to (2).
//   2. Signal-based fallback — on SIGINT/SIGTERM/beforeExit, terminate all
//      tracked children depth-first before the parent exits. Always installed.
//   3. Stale-ownership detection — children spawned detached:false, tracked in
//      a registry; the lock-file pid-liveness probe reuses the same idea.
//
// This module owns the registry + signal fallback. The Job Object shim is
// loaded on demand from ./job_object.js and is purely additive.

import type { ChildProcess } from 'node:child_process';

interface TrackedChild {
  child: ChildProcess;
  /** Insertion order; used to terminate depth-first (last-spawned first). */
  order: number;
}

export interface JobObjectHandle {
  /** Assign a child pid to the kernel job; returns false if unsupported. */
  assign(pid: number): boolean;
  /** Close the job, triggering kill-on-close for all assigned processes. */
  close(): void;
}

/** Attempt to create a Win32 Job Object handle; null if unavailable. */
export type JobObjectFactory = () => JobObjectHandle | null;

export class ChildSupervisor {
  private readonly children = new Map<number, TrackedChild>();
  private counter = 0;
  private signalsInstalled = false;
  private job: JobObjectHandle | null = null;
  private jobAttempted = false;
  private disposed = false;

  constructor(
    private readonly opts: {
      /** Factory for the optional Job Object layer. */
      jobFactory?: JobObjectFactory;
      /** Sink for the degrade-to-fallback warning. */
      warn?: (msg: string) => void;
    } = {},
  ) {}

  /** Lazily create the Job Object once; degrade with a warning if unavailable. */
  private ensureJob(): JobObjectHandle | null {
    if (this.jobAttempted) {
      return this.job;
    }
    this.jobAttempted = true;
    const factory = this.opts.jobFactory;
    if (!factory) {
      return null;
    }
    try {
      this.job = factory();
      if (this.job === null) {
        this.warn('Win32 Job Object unavailable; using signal-based child cleanup fallback.');
      }
    } catch (err) {
      this.job = null;
      this.warn(
        `Win32 Job Object init failed (${(err as Error).message}); using signal-based fallback.`,
      );
    }
    return this.job;
  }

  private warn(msg: string): void {
    (this.opts.warn ?? ((m: string) => process.stderr.write(`[supervisor] ${m}\n`)))(msg);
  }

  /**
   * Register a spawned child for supervision. Assigns it to the Job Object
   * when available (kernel-enforced kill-on-close), and always tracks it for
   * the signal-based fallback. Auto-installs the signal handlers on first use.
   */
  track(child: ChildProcess): void {
    if (this.disposed) {
      throw new Error('ChildSupervisor has been disposed.');
    }
    if (typeof child.pid !== 'number') {
      // Failed spawn — nothing to track.
      return;
    }
    this.installSignals();
    const order = this.counter;
    this.counter += 1;
    this.children.set(child.pid, { child, order });

    // Best-effort kernel assignment.
    const job = this.ensureJob();
    if (job) {
      job.assign(child.pid);
    }

    // Untrack on natural exit to avoid double-kill / leaks.
    child.once('exit', () => {
      if (child.pid !== undefined) {
        this.children.delete(child.pid);
      }
    });
  }

  /** Number of currently tracked (live) children. */
  get trackedCount(): number {
    return this.children.size;
  }

  /** Install SIGINT/SIGTERM/beforeExit handlers exactly once. */
  private installSignals(): void {
    if (this.signalsInstalled) {
      return;
    }
    this.signalsInstalled = true;
    const onSignal = (signal: NodeJS.Signals): void => {
      this.terminateAll();
      // Re-raise default behavior so the process actually exits.
      process.removeListener(signal, onSignal as never);
      process.kill(process.pid, signal);
    };
    process.once('SIGINT', onSignal);
    process.once('SIGTERM', onSignal);
    process.once('beforeExit', () => this.terminateAll());
  }

  /**
   * Terminate every tracked child depth-first (last-spawned first), then close
   * the Job Object (kill-on-close covers any survivors and grandchildren).
   * Idempotent.
   */
  terminateAll(): void {
    const ordered = [...this.children.values()].sort((a, b) => b.order - a.order);
    for (const { child } of ordered) {
      try {
        child.kill('SIGTERM');
      } catch {
        // already gone
      }
    }
    this.children.clear();

    if (this.job) {
      try {
        this.job.close();
      } catch {
        // best-effort
      }
      this.job = null;
    }
  }

  /** Tear down the supervisor (used by tests / clean shutdown). */
  dispose(): void {
    this.terminateAll();
    this.disposed = true;
  }
}
