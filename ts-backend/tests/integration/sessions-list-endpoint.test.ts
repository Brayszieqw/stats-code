// tests/integration/sessions-list-endpoint.test.ts — GET /api/sessions (task 2.2).
//
// Empty → []; multiple sessions sorted by last_active_at desc; summary fields
// complete with no sensitive fields; contract zod validation passes.
//
// _Requirements: 11.1, 11.2, 11.4, 11.5, 11.6_

import { describe, it, expect } from 'vitest';
import { buildRouter, MemSessionStore, contract, type AppState } from '@stats-code/server';
import { z } from 'zod';

function makeState(overrides: Partial<AppState> = {}): AppState {
  return { sessionStore: new MemSessionStore(), ...overrides };
}

const listSchema = z.array(contract.domain.sessionSummary);

describe('GET /api/sessions (Requirements 11)', () => {
  it('returns [] when no sessions exist (R11.4)', async () => {
    const app = buildRouter({ state: makeState() });
    const res = await app.inject({ method: 'GET', url: '/api/sessions' });
    expect(res.statusCode).toBe(200);
    expect(res.json()).toEqual([]);
    await app.close();
  });

  it('returns summaries sorted by last_active_at descending (R11.2)', async () => {
    const store = new MemSessionStore();
    const app = buildRouter({ state: makeState({ sessionStore: store }) });
    const a = await store.create();
    const b = await store.create();
    const c = await store.create();
    a.last_active_at = '2026-01-01T00:00:00.000Z';
    b.last_active_at = '2026-03-01T00:00:00.000Z';
    c.last_active_at = '2026-02-01T00:00:00.000Z';
    const res = await app.inject({ method: 'GET', url: '/api/sessions' });
    expect(res.statusCode).toBe(200);
    const ids = res.json().map((s: { id: string }) => s.id);
    expect(ids).toEqual([b.id, c.id, a.id]);
    await app.close();
  });

  it('summaries are contract-valid and carry no sensitive fields (R11.1/11.5/11.6)', async () => {
    const store = new MemSessionStore();
    const app = buildRouter({ state: makeState({ sessionStore: store }) });
    await store.create();
    const res = await app.inject({ method: 'GET', url: '/api/sessions' });
    const body = res.json();
    expect(listSchema.safeParse(body).success).toBe(true);
    expect(JSON.stringify(body)).not.toContain('api_key');
    expect(JSON.stringify(body)).not.toContain('settings');
    await app.close();
  });
});
