import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import {
  createFileSessionStore,
  MemSessionStore,
  type DatasetAudit,
  type SkillRun,
} from '@stats-code/server';

const audit = {} as DatasetAudit;
const run = {} as SkillRun;

describe('session stores reject run artifacts after archival', () => {
  let dir: string;
  let filePath: string;

  beforeEach(() => {
    dir = mkdtempSync(join(tmpdir(), 'stats-sess-active-'));
    filePath = join(dir, 'sessions.json');
  });

  afterEach(() => {
    rmSync(dir, { recursive: true, force: true });
  });

  it('MemSessionStore rejects dataset audits and skill runs for an archived session', async () => {
    const store = new MemSessionStore();
    const session = await store.create();
    session.status = 'Archived';

    await expect(store.appendDatasetAudit(session.id, audit)).rejects.toMatchObject({ kind: 'archived' });
    await expect(store.appendSkillRun(session.id, run)).rejects.toMatchObject({ kind: 'archived' });
    expect(session.dataset_audits).toEqual([]);
    expect(session.skill_runs).toEqual([]);
  });

  it('FileSessionStore rejects dataset audits and skill runs for an archived session', async () => {
    const writer = createFileSessionStore({ filePath });
    const session = await writer.create();
    const persisted = JSON.parse(readFileSync(filePath, 'utf8')) as {
      sessions: Array<{ id: string; status: string }>;
    };
    persisted.sessions.find((candidate) => candidate.id === session.id)!.status = 'Archived';
    writeFileSync(filePath, JSON.stringify(persisted), 'utf8');
    const store = createFileSessionStore({ filePath });

    await expect(store.appendDatasetAudit(session.id, audit)).rejects.toMatchObject({ kind: 'archived' });
    await expect(store.appendSkillRun(session.id, run)).rejects.toMatchObject({ kind: 'archived' });
    expect((await store.get(session.id)).dataset_audits).toEqual([]);
    expect((await store.get(session.id)).skill_runs).toEqual([]);
  });
});
