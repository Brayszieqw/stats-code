import { describe, it, expect } from 'vitest';
import childProcess from 'node:child_process';
import {
  matchForbiddenCommand,
  checkSpawn,
  checkLibraryLoad,
  normalizeCommand,
  basename,
  guardedSpawn,
  isGuardActive,
  ForbiddenSpawnError,
  FORBIDDEN_RUNTIMES,
} from '@stats-code/engine';

const IS_WINDOWS = process.platform === 'win32';

describe('normalization helpers', () => {
  it('basename strips directory segments and trailing separators', () => {
    expect(basename('Rscript')).toBe('Rscript');
    expect(basename('/usr/bin/Rscript')).toBe('Rscript');
    expect(basename('C:\\bin\\Rscript.exe')).toBe('Rscript.exe');
    expect(basename('/usr/bin/')).toBe('bin');
  });

  it('normalizeCommand strips executable suffixes on every platform (S2)', () => {
    expect(normalizeCommand('/usr/bin/python3')).toBe('python3');
    expect(normalizeCommand('Rscript.exe')).toBe('Rscript');
    expect(normalizeCommand('Rscript.EXE')).toBe('Rscript');
    expect(normalizeCommand('C:\\Python311\\python.exe')).toBe('python');
    expect(normalizeCommand('python.bat')).toBe('python');
    expect(normalizeCommand('python.cmd')).toBe('python');
    expect(normalizeCommand('spss.com')).toBe('spss');
  });
});

describe('matchForbiddenCommand', () => {
  it('matches every canonical blocklist entry', () => {
    for (const entry of FORBIDDEN_RUNTIMES) {
      expect(matchForbiddenCommand(entry)).not.toBeNull();
    }
  });

  it('allows unrelated commands', () => {
    expect(matchForbiddenCommand('ls')).toBeNull();
    expect(matchForbiddenCommand('git')).toBeNull();
    expect(matchForbiddenCommand('node')).toBeNull();
  });

  it('matches blocklisted commands with directory prefixes', () => {
    expect(matchForbiddenCommand('/usr/local/bin/python3')).not.toBeNull();
    expect(matchForbiddenCommand('C:\\Python311\\python.exe')).not.toBeNull();
  });

  it('honors the platform case-sensitivity contract', () => {
    if (IS_WINDOWS) {
      expect(matchForbiddenCommand('RSCRIPT')).not.toBeNull();
      expect(matchForbiddenCommand('rscript')).not.toBeNull();
    } else {
      expect(matchForbiddenCommand('RSCRIPT')).toBeNull();
      expect(matchForbiddenCommand('python')).not.toBeNull();
    }
  });
});

describe('checkLibraryLoad', () => {
  it('blocks forbidden runtime shared libraries', () => {
    expect(() => checkLibraryLoad('libR.so')).toThrow(ForbiddenSpawnError);
    expect(() => checkLibraryLoad('python3.dll')).toThrow(ForbiddenSpawnError);
  });
  it('allows unrelated libraries', () => {
    expect(() => checkLibraryLoad('libssl.so')).not.toThrow();
  });
});

describe('guardedSpawn', () => {
  it('blocks forbidden spawns inside the scope and restores afterwards', () => {
    expect(isGuardActive()).toBe(false);
    expect(() =>
      guardedSpawn(() => {
        // python is forbidden — spawnSync must throw before executing.
        childProcess.spawnSync('python', ['--version']);
      }),
    ).toThrow(ForbiddenSpawnError);
    // patches restored after the scope unwinds on throw.
    expect(isGuardActive()).toBe(false);
  });

  it('allows non-forbidden spawns inside the scope', () => {
    const out = guardedSpawn(() => {
      const r = childProcess.spawnSync(process.execPath, ['-e', 'process.stdout.write("ok")']);
      return r.stdout?.toString() ?? '';
    });
    expect(out).toContain('ok');
    expect(isGuardActive()).toBe(false);
  });

  it('outside a guarded scope, forbidden spawns are NOT blocked by the sentinel', () => {
    // The browser-invocation path lives outside guarded scopes (Req 8.5).
    // We do not actually spawn python; we just assert the patch is inactive,
    // i.e. checkSpawn is the only gate and it is not installed here.
    expect(isGuardActive()).toBe(false);
  });

  it('supports nested guarded scopes (ref-counted)', () => {
    guardedSpawn(() => {
      expect(isGuardActive()).toBe(true);
      guardedSpawn(() => {
        expect(isGuardActive()).toBe(true);
      });
      // still active after the inner scope closes
      expect(isGuardActive()).toBe(true);
    });
    expect(isGuardActive()).toBe(false);
  });

  it('restores patches for async closures', async () => {
    await guardedSpawn(async () => {
      expect(isGuardActive()).toBe(true);
      await Promise.resolve();
    });
    expect(isGuardActive()).toBe(false);
  });

  it('checkSpawn throws a structured error preserving the raw target', () => {
    try {
      checkSpawn('Rscript');
      expect.unreachable();
    } catch (e) {
      expect(e).toBeInstanceOf(ForbiddenSpawnError);
      expect((e as ForbiddenSpawnError).target).toBe('Rscript');
      expect((e as ForbiddenSpawnError).code).toBe('FORBIDDEN_SPAWN');
    }
  });
});
