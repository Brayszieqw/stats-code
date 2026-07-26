// spawn_policy.ts — Forbidden_Runtime sentinel (Phase 1, task 3.5).
//
// Transcribed from crates/stats-code/src/spawn_policy.rs. Blocks any spawn of
// an external statistical runtime during the sidecar / SPA-render / snapshot
// pipelines (Requirement 8). In Node this is enforced by routing all process
// creation through `guardedSpawn`, which, within its scope, monkey-patches
// child_process.{spawn,exec,execFile,spawnSync,execSync,execFileSync} to throw
// a structured ForbiddenSpawnError when the normalized target matches the
// blocklist. The launcher's browser invocation lives OUTSIDE any guarded scope
// (Requirement 8.5).
//
// Invariants (mirrors the Rust contract):
//  - matching is pure: basename + strip a known executable suffix
//    (.exe/.bat/.cmd/.com — on every platform), then compare;
//  - Windows comparison is case-insensitive, Unix is case-sensitive;
//  - forbidden shared libraries are blocked too (Requirement 8.4).

import childProcess from 'node:child_process';

/** Requirement 8 / 10.1 forbidden runtime command names. */
export const FORBIDDEN_RUNTIMES: readonly string[] = [
  'Rscript',
  'R',
  'python',
  'python3',
  'pythonw',
  'sas',
  'spss',
  'pspp',
  'pspp-cli',
  'statistics',
  'stats',
];

/** Forbidden shared libraries (Requirement 8.4). */
export const FORBIDDEN_LIBRARIES: readonly string[] = [
  'libR.so',
  'libR.dylib',
  'R.dll',
  'libpython3.so',
  'libpython3.dylib',
  'python3.dll',
  'python.dll',
];

const IS_WINDOWS = process.platform === 'win32';

export class ForbiddenSpawnError extends Error {
  readonly code = 'FORBIDDEN_SPAWN';
  constructor(
    public readonly target: string,
    public readonly reason: string = 'external statistical runtime',
  ) {
    super(`forbidden spawn '${target}': ${reason}`);
    this.name = 'ForbiddenSpawnError';
  }
}

/** Return the basename of a path, stripping trailing separators first. */
export function basename(s: string): string {
  let end = s.length;
  while (end > 0 && (s[end - 1] === '/' || s[end - 1] === '\\')) {
    end -= 1;
  }
  const trimmed = s.slice(0, end);
  let lastSep = -1;
  for (let i = trimmed.length - 1; i >= 0; i -= 1) {
    if (trimmed[i] === '/' || trimmed[i] === '\\') {
      lastSep = i;
      break;
    }
  }
  return lastSep === -1 ? trimmed : trimmed.slice(lastSep + 1);
}

/** Executable suffixes stripped during normalization, on every platform. */
const STRIPPED_EXECUTABLE_SUFFIXES = ['.exe', '.bat', '.cmd', '.com'] as const;

/**
 * Normalize a command: basename, then strip a known executable suffix
 * (case-insensitive). Stripping is deliberately platform-independent: a
 * Windows-style `python.exe` must not slip past the sentinel just because
 * the process happens to run on Linux/macOS (S2).
 */
export function normalizeCommand(s: string): string {
  const name = basename(s);
  const lower = name.toLowerCase();
  for (const suffix of STRIPPED_EXECUTABLE_SUFFIXES) {
    if (lower.length > suffix.length && lower.endsWith(suffix)) {
      return name.slice(0, name.length - suffix.length);
    }
  }
  return name;
}

function namesMatch(actual: string, forbidden: string): boolean {
  return IS_WINDOWS
    ? actual.toLowerCase() === forbidden.toLowerCase()
    : actual === forbidden;
}

/**
 * Check whether `command` resolves to a forbidden runtime. Returns the matched
 * blocklist entry, or null if allowed. Pure.
 */
export function matchForbiddenCommand(command: string): string | null {
  const normalized = normalizeCommand(command);
  for (const entry of FORBIDDEN_RUNTIMES) {
    if (namesMatch(normalized, entry)) {
      return entry;
    }
  }
  return null;
}

/** Throw if `command` is a forbidden runtime. */
export function checkSpawn(command: string): void {
  if (matchForbiddenCommand(command) !== null) {
    throw new ForbiddenSpawnError(command, 'external statistical runtime');
  }
}

/** Throw if `name` is a forbidden shared library (basename-compared). */
export function checkLibraryLoad(name: string): void {
  const normalized = basename(name);
  for (const entry of FORBIDDEN_LIBRARIES) {
    const matched = IS_WINDOWS
      ? normalized.toLowerCase() === entry.toLowerCase()
      : normalized === entry;
    if (matched) {
      throw new ForbiddenSpawnError(name, 'external statistical runtime shared library');
    }
  }
}

// ---------------------------------------------------------------------------
// Guarded scope: monkey-patch child_process within the scope (Requirement 8.1).
// ---------------------------------------------------------------------------

type SpawnFns = Pick<
  typeof childProcess,
  'spawn' | 'exec' | 'execFile' | 'spawnSync' | 'execSync' | 'execFileSync'
>;

let guardDepth = 0;
let originals: SpawnFns | null = null;

/** The patched functions extract the command (first arg) and check it first. */
function installPatches(): void {
  const cp = childProcess as unknown as Record<string, unknown>;
  originals = {
    spawn: childProcess.spawn,
    exec: childProcess.exec,
    execFile: childProcess.execFile,
    spawnSync: childProcess.spawnSync,
    execSync: childProcess.execSync,
    execFileSync: childProcess.execFileSync,
  };

  const orig = originals;

  // spawn / spawnSync / execFile / execFileSync take the command as arg 0.
  const guardCommandFirst =
    <A extends unknown[], R>(fn: (...args: A) => R) =>
    (...args: A): R => {
      const command = String(args[0]);
      checkSpawn(command);
      return fn(...args);
    };

  // exec / execSync take a full shell command line; guard the first token.
  const guardShellLine =
    <A extends unknown[], R>(fn: (...args: A) => R) =>
    (...args: A): R => {
      const line = String(args[0]).trim();
      const firstToken = line.split(/\s+/)[0] ?? '';
      checkSpawn(firstToken);
      return fn(...args);
    };

  cp['spawn'] = guardCommandFirst(orig.spawn.bind(childProcess));
  cp['spawnSync'] = guardCommandFirst(orig.spawnSync.bind(childProcess));
  cp['execFile'] = guardCommandFirst(orig.execFile.bind(childProcess));
  cp['execFileSync'] = guardCommandFirst(orig.execFileSync.bind(childProcess));
  cp['exec'] = guardShellLine(orig.exec.bind(childProcess));
  cp['execSync'] = guardShellLine(orig.execSync.bind(childProcess));
}

function restorePatches(): void {
  if (!originals) {
    return;
  }
  const cp = childProcess as unknown as Record<string, unknown>;
  cp['spawn'] = originals.spawn;
  cp['spawnSync'] = originals.spawnSync;
  cp['execFile'] = originals.execFile;
  cp['execFileSync'] = originals.execFileSync;
  cp['exec'] = originals.exec;
  cp['execSync'] = originals.execSync;
  originals = null;
}

/**
 * Run `fn` inside a guarded scope. Within the scope every child_process spawn
 * is checked against the Forbidden_Runtime list and throws ForbiddenSpawnError
 * on a match. Nested guarded scopes are supported (ref-counted). Patches are
 * always restored when the outermost scope exits, even on throw.
 *
 * Supports sync and async closures.
 */
export function guardedSpawn<T>(fn: () => T): T {
  if (guardDepth === 0) {
    installPatches();
  }
  guardDepth += 1;

  const release = (): void => {
    guardDepth -= 1;
    if (guardDepth === 0) {
      restorePatches();
    }
  };

  let result: T;
  try {
    result = fn();
  } catch (err) {
    release();
    throw err;
  }

  if (result instanceof Promise) {
    return result.finally(release) as unknown as T;
  }
  release();
  return result;
}

/** True while at least one guarded scope is active. */
export function isGuardActive(): boolean {
  return guardDepth > 0;
}
