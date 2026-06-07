// spawn_policy.ts — Forbidden_Runtime sentinel (Phase 1, task 3.5).

export const FORBIDDEN_RUNTIMES: readonly string[] = [
  'rscript',
  'r',
  'python',
  'python3',
  'pythonw',
  'sas',
  'spss',
  'pspp',
  'pspp-cli',
];

export class ForbiddenSpawnError extends Error {
  readonly code = 'FORBIDDEN_SPAWN';
  constructor(public readonly target: string) {
    super(`Forbidden runtime spawn blocked: ${target}`);
    this.name = 'ForbiddenSpawnError';
  }
}

/** Placeholder; implemented in task 3.5. */
export function guardedSpawn<T>(fn: () => T): T {
  return fn();
}
