// tests/property/forbidden-spawn.property.test.ts — Property 21 (task 3.10).
//
// Property 21: Forbidden spawn. For ALL spawn targets that normalize to a
// Forbidden_Runtime, a guarded spawn throws ForbiddenSpawnError; for ALL
// targets that do not, the sentinel does not block them. Matching is pure
// (basename + Windows .exe strip + platform casing).
//
// Validates: Requirements 8.1, 8.2, 8.3

import { describe, it, expect } from 'vitest';
import fc from 'fast-check';
import childProcess from 'node:child_process';
import {
  matchForbiddenCommand,
  checkSpawn,
  normalizeCommand,
  guardedSpawn,
  isGuardActive,
  ForbiddenSpawnError,
  FORBIDDEN_RUNTIMES,
} from '@stats-code/engine';

const IS_WINDOWS = process.platform === 'win32';

// Directory prefixes the matcher must see through (basename normalization).
const dirPrefixArb = fc.constantFrom('', '/usr/bin/', '/usr/local/bin/', 'C:\\tools\\', './');

/** Build a path-decorated forbidden target from a canonical blocklist name. */
const forbiddenTargetArb = fc
  .record({
    base: fc.constantFrom(...FORBIDDEN_RUNTIMES),
    prefix: dirPrefixArb,
    // .exe suffix is only normalized away on Windows.
    exe: fc.boolean(),
    // Case variation; only flips matching on Windows (case-insensitive).
    upper: fc.boolean(),
  })
  .map(({ base, prefix, exe, upper }) => {
    let name = base;
    if (upper) name = name.toUpperCase();
    if (exe) name = `${name}.exe`;
    return { target: `${prefix}${name}`, base, exe, upper };
  });

// Clearly-allowed command names (not in the blocklist under any normalization).
const allowedNameArb = fc
  .constantFrom('node', 'git', 'ls', 'cargo', 'pwsh', 'bash', 'curl', 'tar', 'rg', 'code')
  .chain((name) => dirPrefixArb.map((prefix) => `${prefix}${name}`));

describe('Property 21: forbidden spawn (Requirements 8.1, 8.2, 8.3)', () => {
  it('every forbidden target (any path/exe/casing) is matched when it should be', () => {
    fc.assert(
      fc.property(forbiddenTargetArb, ({ target, exe, upper }) => {
        const matched = matchForbiddenCommand(target) !== null;
        // On Unix: .exe suffix is NOT stripped, so "<name>.exe" won't match;
        // uppercase won't match either (case-sensitive). On Windows both the
        // .exe strip and case-insensitive compare apply, so it always matches.
        const expected = IS_WINDOWS ? true : !exe && !upper;
        expect(matched).toBe(expected);
      }),
      { numRuns: 400 },
    );
  });

  it('checkSpawn throws ForbiddenSpawnError exactly when the target matches', () => {
    fc.assert(
      fc.property(forbiddenTargetArb, ({ target }) => {
        const shouldThrow = matchForbiddenCommand(target) !== null;
        if (shouldThrow) {
          expect(() => checkSpawn(target)).toThrow(ForbiddenSpawnError);
        } else {
          expect(() => checkSpawn(target)).not.toThrow();
        }
      }),
      { numRuns: 400 },
    );
  });

  it('guarded child_process.spawnSync is blocked for forbidden targets', () => {
    fc.assert(
      fc.property(forbiddenTargetArb, ({ target }) => {
        // Only meaningful when the target actually normalizes to a forbidden
        // runtime on this platform.
        fc.pre(matchForbiddenCommand(target) !== null);
        let threw = false;
        try {
          guardedSpawn(() => {
            childProcess.spawnSync(target, ['--version']);
          });
        } catch (e) {
          threw = e instanceof ForbiddenSpawnError;
        }
        // The guard must always be restored, even on throw.
        expect(isGuardActive()).toBe(false);
        return threw;
      }),
      { numRuns: 150 },
    );
  });

  it('allowed commands are never matched and never blocked by the sentinel', () => {
    fc.assert(
      fc.property(allowedNameArb, (target) => {
        expect(matchForbiddenCommand(target)).toBeNull();
        expect(() => checkSpawn(target)).not.toThrow();
      }),
      { numRuns: 200 },
    );
  });

  it('matching is invariant under directory prefixes (pure basename rule)', () => {
    fc.assert(
      fc.property(
        fc.constantFrom(...FORBIDDEN_RUNTIMES),
        dirPrefixArb,
        (base, prefix) => {
          // The bare name and the prefixed name normalize to the same basename
          // and therefore match identically.
          expect(normalizeCommand(`${prefix}${base}`)).toBe(normalizeCommand(base));
          expect(matchForbiddenCommand(`${prefix}${base}`) !== null).toBe(
            matchForbiddenCommand(base) !== null,
          );
        },
      ),
      { numRuns: 200 },
    );
  });
});
