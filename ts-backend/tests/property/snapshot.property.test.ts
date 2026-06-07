// tests/property/snapshot.property.test.ts — Properties 17 & 18 (tasks 13.13, 13.14).
//
// Property 17 (Snapshot integrity / determinism): for ALL entry sets, the
// deterministic ZIP writer produces byte-identical output regardless of input
// order, and the manifest digests match the content bytes (Requirements 6.1, 6.2).
//
// Property 18 (No partial artifact on failure): for ALL runs that fail a gate
// (oversized payload, invalid entry name, non-completed run, bad narrative),
// exportSnapshot throws and leaves neither the final file nor a .tmp behind
// (Requirements 6.3, 6.4, 6.5).
//
// Validates: Requirements 6.1, 6.2, 6.3, 6.4, 6.5

import { describe, it, expect, afterEach } from 'vitest';
import fc from 'fast-check';
import { mkdtempSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createHash } from 'node:crypto';
import { snapshot } from '@stats-code/engine';

const { buildZipBytes, exportSnapshot, sha256Hex, ARTIFACT_PAYLOAD_CEILING_BYTES, SnapshotError } =
  snapshot;

type RunSnapshot = Parameters<typeof exportSnapshot>[0];

const tmpDirs: string[] = [];
afterEach(() => {
  for (const d of tmpDirs.splice(0)) rmSync(d, { recursive: true, force: true });
});
function freshTmp(): string {
  const d = mkdtempSync(join(tmpdir(), 'sc-snap-prop-'));
  tmpDirs.push(d);
  return d;
}

const enc = (s: string) => new TextEncoder().encode(s);
const sha32 = (b: Uint8Array) => createHash('sha256').update(b).digest();
const DATASET = enc('id,v\n1,2\n');

// Arbitrary unique-named zip entries.
const entryArb = fc.record({
  name: fc.stringMatching(/^[a-z][a-z0-9_]{0,10}(\/[a-z][a-z0-9_]{0,10}){0,2}\.txt$/),
  content: fc.string({ maxLength: 64 }),
});

const uniqueEntriesArb = fc
  .uniqueArray(entryArb, { minLength: 1, maxLength: 6, selector: (e) => e.name })
  .map((es) => es.map((e) => ({ name: e.name, bytes: enc(e.content) })));

function baseRun(overrides: Partial<RunSnapshot> = {}): RunSnapshot {
  return {
    runId: 'run-prop',
    status: 'completed',
    datasetSha256: sha32(DATASET),
    datasetCsvBytes: DATASET,
    workflow: { schemaVersion: 1, inputDataset: { path: 'data.csv', sha256: '0'.repeat(64) }, steps: [] },
    artifacts: [],
    llmCalls: [],
    referenceSoftware: [],
    osFamily: 'Windows',
    osVersion: '10',
    releaseVersion: '0.5.0',
    commitSha: '0'.repeat(40),
    createdAtUtc: '2024-01-01T00:00:00Z',
    runtimeDependencies: {},
    apiKeys: [],
    narrativeSteps: [],
    ...overrides,
  };
}

describe('Property 17: snapshot determinism (Requirements 6.1, 6.2)', () => {
  it('byte output is invariant under input entry ordering', () => {
    fc.assert(
      fc.property(uniqueEntriesArb, (entries) => {
        const shuffled = [...entries].reverse();
        const a = buildZipBytes(entries);
        const b = buildZipBytes(shuffled);
        expect(Buffer.from(a).equals(Buffer.from(b))).toBe(true);
      }),
      { numRuns: 200 },
    );
  });

  it('repeated builds of the same entry set are byte-identical', () => {
    fc.assert(
      fc.property(uniqueEntriesArb, (entries) => {
        expect(sha256Hex(buildZipBytes(entries))).toBe(sha256Hex(buildZipBytes(entries)));
      }),
      { numRuns: 200 },
    );
  });

  it('a completed export is byte-deterministic for identical runs', () => {
    fc.assert(
      fc.property(fc.constant(null), () => {
        const dir = freshTmp();
        const a = join(dir, 'a.zip');
        const b = join(dir, 'b.zip');
        const r1 = exportSnapshot(baseRun(), a);
        const r2 = exportSnapshot(baseRun(), b);
        expect(r1.sha256).toBe(r2.sha256);
      }),
      { numRuns: 5 },
    );
  });
});

describe('Property 18: no partial artifact on failure (Requirements 6.3, 6.4, 6.5)', () => {
  it('an oversized payload throws and leaves no file or .tmp', () => {
    fc.assert(
      fc.property(fc.integer({ min: 1, max: 1024 }), (over) => {
        const dir = freshTmp();
        const dest = join(dir, 'big.zip');
        const big = new Uint8Array(ARTIFACT_PAYLOAD_CEILING_BYTES + over);
        const run = baseRun({ artifacts: [{ path: 'artifacts/big.bin', bytes: big }] });
        expect(() => exportSnapshot(run, dest)).toThrow(SnapshotError);
        expect(existsSync(dest)).toBe(false);
        expect(existsSync(`${dest}.tmp`)).toBe(false);
      }),
      { numRuns: 10 },
    );
  });

  it('an invalid artifact path throws and leaves no file or .tmp', () => {
    fc.assert(
      fc.property(fc.constantFrom('../escape.txt', '/abs.txt', 'a\\b.txt', '..\\x.txt'), (badPath) => {
        const dir = freshTmp();
        const dest = join(dir, 'bad.zip');
        const run = baseRun({ artifacts: [{ path: badPath, bytes: enc('x') }] });
        expect(() => exportSnapshot(run, dest)).toThrow();
        expect(existsSync(dest)).toBe(false);
        expect(existsSync(`${dest}.tmp`)).toBe(false);
      }),
      { numRuns: 20 },
    );
  });

  it('a non-completed run is refused with no file written', () => {
    fc.assert(
      fc.property(fc.constantFrom('running', 'failed') as fc.Arbitrary<'running' | 'failed'>, (status) => {
        const dir = freshTmp();
        const dest = join(dir, 'nc.zip');
        expect(() => exportSnapshot(baseRun({ status }), dest)).toThrow(SnapshotError);
        expect(existsSync(dest)).toBe(false);
        expect(existsSync(`${dest}.tmp`)).toBe(false);
      }),
      { numRuns: 10 },
    );
  });
});
