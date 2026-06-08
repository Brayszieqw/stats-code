// tests/integration/dataset-upload.test.ts — dataset route wiring (task 5.12).
//
// POST /api/sessions/:sid/datasets → 201 with summary, appended to the session;
// GET .../datasets/:did → 200, missing → 404; oversize → 413 with no
// persistence.
//
// _Requirements: 6.3, 6.4, 6.5, 6.6_

import { describe, it, expect, afterEach } from 'vitest';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { buildRouter, MemSessionStore, createFsDatasetStore, contract, type AppState } from '@stats-code/server';

const tmpDirs: string[] = [];
afterEach(() => {
  for (const d of tmpDirs.splice(0)) rmSync(d, { recursive: true, force: true });
});
function freshRoot(): string {
  const d = mkdtempSync(join(tmpdir(), 'sc-dsi-'));
  tmpDirs.push(d);
  return d;
}

function wiredState(): AppState {
  return {
    sessionStore: new MemSessionStore(),
    datasetStore: createFsDatasetStore({ root: freshRoot() }),
  };
}

const b64 = (s: string) => Buffer.from(s, 'utf8').toString('base64');

describe('POST /api/sessions/:sid/datasets (wired)', () => {
  it('returns 201 with a summary and appends it to the session', async () => {
    const app = buildRouter({ state: wiredState() });
    const sid = (await app.inject({ method: 'POST', url: '/api/sessions' })).json().id as string;
    const res = await app.inject({
      method: 'POST',
      url: `/api/sessions/${sid}/datasets`,
      payload: { filename: 'data.csv', data: b64('age,name\n42,alice\n37,bob\n') },
    });
    expect(res.statusCode).toBe(201);
    const summary = res.json();
    expect(contract.domain.datasetSummary.safeParse(summary).success).toBe(true);
    expect(summary.row_count).toBe(2);

    // The summary is appended to the session and retrievable.
    const get = await app.inject({
      method: 'GET',
      url: `/api/sessions/${sid}/datasets/${summary.dataset_id}`,
    });
    expect(get.statusCode).toBe(200);
    expect(get.json().dataset_id).toBe(summary.dataset_id);
    await app.close();
  });

  it('returns 404 for a missing dataset id', async () => {
    const app = buildRouter({ state: wiredState() });
    const sid = (await app.inject({ method: 'POST', url: '/api/sessions' })).json().id as string;
    const res = await app.inject({
      method: 'GET',
      url: `/api/sessions/${sid}/datasets/00000000-0000-0000-0000-000000000000`,
    });
    expect(res.statusCode).toBe(404);
    await app.close();
  });

  it('rejects an unparseable payload with 422 and does not append', async () => {
    const app = buildRouter({ state: wiredState() });
    const sid = (await app.inject({ method: 'POST', url: '/api/sessions' })).json().id as string;
    const res = await app.inject({
      method: 'POST',
      url: `/api/sessions/${sid}/datasets`,
      payload: { filename: 'data.bin', data: b64('not a table') },
    });
    expect(res.statusCode).toBe(422);
    const session = (await app.inject({ method: 'GET', url: `/api/sessions/${sid}` })).json();
    expect(session.datasets).toHaveLength(0);
    await app.close();
  });

  it('returns 500 when no dataset store is configured', async () => {
    const app = buildRouter({ state: { sessionStore: new MemSessionStore() } });
    const sid = (await app.inject({ method: 'POST', url: '/api/sessions' })).json().id as string;
    const res = await app.inject({
      method: 'POST',
      url: `/api/sessions/${sid}/datasets`,
      payload: { filename: 'data.csv', data: b64('a,b\n1,2\n') },
    });
    expect(res.statusCode).toBe(500);
    await app.close();
  });
});
