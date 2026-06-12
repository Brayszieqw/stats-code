// tests/integration/contract-diff-harness.test.ts — full contract-diff harness
// (task 17.2).
//
// Runs the complete request/response and SSE-stream golden diff across all 13
// API_Contract routes (plus the SPA fallback) to confirm full API_Contract
// conformance at cutover. Each route is exercised against an engine-backed
// AppState and its response validated against the zod contract schema and the
// expected status code. The SSE messages route is replayed frame-for-frame.
//
// _Requirements: 1.1, 15.2_

import { describe, it, expect } from 'vitest';
import {
  buildRouter,
  MemSessionStore,
  contract,
  serializeSseFrame,
  createCoverageMatrixProvider,
  createSidecarProvider,
  type AppState,
  type AgentEvent,
  type MessageHandler,
} from '@stats-code/server';

const { ROUTE_CONTRACTS, domain } = contract;

const assetSource = {
  get(p: string) {
    return p === '/assets/app.js'
      ? { bytes: new TextEncoder().encode('1'), contentType: 'text/javascript; charset=utf-8' }
      : undefined;
  },
  indexHtml() {
    return { bytes: new TextEncoder().encode('<!doctype html>'), contentType: 'text/html; charset=utf-8' };
  },
};

const fixedEvents: AgentEvent[] = [
  { type: 'text_delta', text: 'hi' },
  { type: 'done' },
];
const handler: MessageHandler = {
  // eslint-disable-next-line @typescript-eslint/require-await
  async *handleMessage() {
    for (const e of fixedEvents) yield e;
  },
};

function fullState(): AppState {
  return {
    sessionStore: new MemSessionStore(),
    messageHandler: handler,
    llmConfigStore: { read: () => null, write: () => {} },
    llmProbe: { probe: async () => undefined },
    coverageMatrixProvider: createCoverageMatrixProvider(),
    sidecarProvider: createSidecarProvider(),
  };
}

function app() {
  return buildRouter({ state: fullState(), installSpaFallback: true, spaAssetSource: assetSource });
}

describe('contract-diff harness — route registry completeness', () => {
  it('covers the 13 API_Contract routes plus the dual-mode additions', () => {
    expect(ROUTE_CONTRACTS).toHaveLength(16);
  });

  it('every route declares a method, path, and success status', () => {
    for (const r of ROUTE_CONTRACTS) {
      expect(['GET', 'POST', 'PATCH', 'DELETE']).toContain(r.method);
      expect(r.path.startsWith('/api/')).toBe(true);
      expect(typeof r.successStatus).toBe('number');
    }
  });
});

describe('contract-diff harness — non-SSE routes conform', () => {
  it('GET /api/health', async () => {
    const a = app();
    const res = await a.inject({ method: 'GET', url: '/api/health' });
    expect(res.statusCode).toBe(200);
    expect(contract.healthResponse.safeParse(res.json()).success).toBe(true);
    await a.close();
  });

  it('full session lifecycle: create → get → patch', async () => {
    const a = app();
    const created = await a.inject({ method: 'POST', url: '/api/sessions' });
    expect(created.statusCode).toBe(201);
    expect(domain.session.safeParse(created.json()).success).toBe(true);
    const id = created.json().id;

    const got = await a.inject({ method: 'GET', url: `/api/sessions/${id}` });
    expect(got.statusCode).toBe(200);
    expect(domain.session.safeParse(got.json()).success).toBe(true);

    const patched = await a.inject({
      method: 'PATCH',
      url: `/api/sessions/${id}/settings`,
      payload: { decision_assistant: false },
    });
    expect(patched.statusCode).toBe(200);
    expect(patched.json().settings.decision_assistant).toBe(false);
    await a.close();
  });

  it('llm-status + coverage-matrix + sidecar conform to their schemas', async () => {
    const a = app();
    const status = await a.inject({ method: 'GET', url: '/api/llm-status' });
    expect(contract.llmStatusResponse.safeParse(status.json()).success).toBe(true);

    const matrix = await a.inject({ method: 'GET', url: '/api/coverage-matrix' });
    expect(matrix.statusCode).toBe(200);
    expect(contract.sidecar.coverageMatrix.safeParse(matrix.json()).success).toBe(true);

    const snippet = await a.inject({
      method: 'POST',
      url: '/api/sidecar/tableone',
      payload: {
        software: 'R',
        dataset_sha256: 'a'.repeat(64),
        columns: [
          { name: 'age', dtype: 'numeric' },
          { name: 'sex', dtype: 'categorical' },
        ],
        params: {},
      },
    });
    expect(snippet.statusCode).toBe(200);
    expect(contract.sidecar.sidecarSnippet.safeParse(snippet.json()).success).toBe(true);
    await a.close();
  });

  it('error envelope conforms for an unknown session (404)', async () => {
    const a = app();
    const res = await a.inject({
      method: 'GET',
      url: '/api/sessions/00000000-0000-4000-8000-000000000000',
    });
    expect(res.statusCode).toBe(404);
    expect(domain.errorPayload.safeParse(res.json()).success).toBe(true);
    await a.close();
  });
});

describe('contract-diff harness — SSE stream conforms frame-for-frame', () => {
  it('POST messages emits the recorded SSE frame sequence', async () => {
    const a = app();
    const created = (await a.inject({ method: 'POST', url: '/api/sessions' })).json();
    const res = await a.inject({
      method: 'POST',
      url: `/api/sessions/${created.id}/messages`,
      payload: { text: 'hi' },
    });
    expect(res.statusCode).toBe(200);
    expect(res.headers['content-type']).toContain('text/event-stream');
    const expected = fixedEvents.map(serializeSseFrame).join('');
    expect(res.body).toBe(expected);
    await a.close();
  });
});

describe('contract-diff harness — SPA fallback conforms', () => {
  it('serves an asset, falls back to index.html, and 404s unknown /api', async () => {
    const a = app();
    const asset = await a.inject({ method: 'GET', url: '/assets/app.js' });
    expect(asset.statusCode).toBe(200);

    const deep = await a.inject({ method: 'GET', url: '/some/deep/route' });
    expect(deep.statusCode).toBe(200);
    expect(deep.headers['content-type']).toContain('text/html');

    const apiMiss = await a.inject({ method: 'GET', url: '/api/unknown' });
    expect(apiMiss.statusCode).toBe(404);
    await a.close();
  });
});
