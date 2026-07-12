import { describe, it, expect } from 'vitest';
import { buildRouter, MemSessionStore, type AppState } from '@stats-code/server';
import { coverage } from '@stats-code/engine';

function makeState(overrides: Partial<AppState> = {}): AppState {
  return { sessionStore: new MemSessionStore(), ...overrides };
}

describe('HTTP contract routes', () => {
  it('GET /api/health → 200 {status:"ok"}', async () => {
    const app = buildRouter({ state: makeState() });
    const res = await app.inject({ method: 'GET', url: '/api/health' });
    expect(res.statusCode).toBe(200);
    expect(res.json()).toEqual({ status: 'ok' });
    await app.close();
  });

  it('POST /api/sessions → 201 with a session DTO', async () => {
    const app = buildRouter({ state: makeState() });
    const res = await app.inject({ method: 'POST', url: '/api/sessions' });
    expect(res.statusCode).toBe(201);
    const body = res.json();
    expect(body.status).toBe('Active');
    expect(body.settings.decision_assistant).toBe(true);
    await app.close();
  });

  it('GET /api/sessions/:sid → 200 then 404 for unknown', async () => {
    const app = buildRouter({ state: makeState() });
    const created = (await app.inject({ method: 'POST', url: '/api/sessions' })).json();
    const ok = await app.inject({ method: 'GET', url: `/api/sessions/${created.id}` });
    expect(ok.statusCode).toBe(200);
    const missing = await app.inject({
      method: 'GET',
      url: '/api/sessions/00000000-0000-4000-8000-000000000000',
    });
    expect(missing.statusCode).toBe(404);
    expect(missing.json().error_code).toBe('SessionNotFound');
    await app.close();
  });

  it('PATCH settings updates decision_assistant', async () => {
    const app = buildRouter({ state: makeState() });
    const created = (await app.inject({ method: 'POST', url: '/api/sessions' })).json();
    const res = await app.inject({
      method: 'PATCH',
      url: `/api/sessions/${created.id}/settings`,
      payload: { decision_assistant: false },
    });
    expect(res.statusCode).toBe(200);
    expect(res.json().settings.decision_assistant).toBe(false);
    await app.close();
  });

  it('POST messages streams an SSE response', async () => {
    const app = buildRouter({ state: makeState() });
    const created = (await app.inject({ method: 'POST', url: '/api/sessions' })).json();
    const res = await app.inject({
      method: 'POST',
      url: `/api/sessions/${created.id}/messages`,
      payload: { text: 'hello' },
    });
    expect(res.statusCode).toBe(200);
    expect(res.headers['content-type']).toContain('text/event-stream');
    expect(res.body).toContain('event: done');
    await app.close();
  });

  it('POST messages persists user text and recoverable agent blocks', async () => {
    const handler = {
      // eslint-disable-next-line @typescript-eslint/require-await
      async *handleMessage() {
        yield { type: 'text_delta', text: 'hello back' } as const;
        yield { type: 'interpretation', text: 'saved interpretation' } as const;
        yield { type: 'done' } as const;
      },
    };
    const app = buildRouter({ state: makeState({ messageHandler: handler }) });
    const created = (await app.inject({ method: 'POST', url: '/api/sessions' })).json();
    const res = await app.inject({
      method: 'POST',
      url: `/api/sessions/${created.id}/messages`,
      payload: { text: 'history title' },
    });
    expect(res.statusCode).toBe(200);

    const session = (await app.inject({ method: 'GET', url: `/api/sessions/${created.id}` })).json();
    expect(session.messages).toHaveLength(2);
    expect(session.messages[0].User.content.Text).toBe('history title');
    expect(session.messages[1].Agent.blocks).toEqual([
      { Text: 'hello back' },
      { Interpretation: 'saved interpretation' },
    ]);

    const list = (await app.inject({ method: 'GET', url: '/api/sessions' })).json();
    expect(list.find((s: { id: string }) => s.id === created.id).title).toBe('history title');
    await app.close();
  });

  it('POST messages relays orchestrator AgentEvents as SSE frames (task 3.3)', async () => {
    const handler = {
      // eslint-disable-next-line @typescript-eslint/require-await
      async *handleMessage() {
        yield { type: 'text_delta', text: '你好' } as const;
        yield { type: 'skill_call', skill_id: 'ttest', args: { y: 'age' } } as const;
        yield { type: 'interpretation', text: 'done thinking' } as const;
        yield { type: 'done' } as const;
      },
    };
    const app = buildRouter({ state: makeState({ messageHandler: handler }) });
    const created = (await app.inject({ method: 'POST', url: '/api/sessions' })).json();
    const res = await app.inject({
      method: 'POST',
      url: `/api/sessions/${created.id}/messages`,
      payload: { text: 'hi' },
    });
    expect(res.statusCode).toBe(200);
    expect(res.headers['content-type']).toContain('text/event-stream');
    const body = res.body;
    expect(body).toContain('event: text_delta\ndata: {"text":"你好"}\n\n');
    expect(body).toContain('event: skill_call\ndata: {"skill_id":"ttest","args":{"y":"age"}}\n\n');
    expect(body).toContain('event: interpretation\ndata: {"text":"done thinking"}\n\n');
    expect(body.trimEnd().endsWith('event: done\ndata: {}')).toBe(true);
    await app.close();
  });

  it('POST messages emits an error frame when the handler throws mid-stream', async () => {
    const handler = {
      // eslint-disable-next-line @typescript-eslint/require-await
      async *handleMessage() {
        yield { type: 'text_delta', text: 'partial' } as const;
        throw new Error('boom');
      },
    };
    const app = buildRouter({ state: makeState({ messageHandler: handler }) });
    const created = (await app.inject({ method: 'POST', url: '/api/sessions' })).json();
    const res = await app.inject({
      method: 'POST',
      url: `/api/sessions/${created.id}/messages`,
      payload: { text: 'hi' },
    });
    expect(res.statusCode).toBe(200);
    expect(res.body).toContain('event: text_delta');
    expect(res.body).toContain('event: error');
    expect(res.body).toContain('SkillExecutionFailed');
    await app.close();
  });

  it('POST messages with no text → 413', async () => {
    const app = buildRouter({ state: makeState() });
    const created = (await app.inject({ method: 'POST', url: '/api/sessions' })).json();
    const res = await app.inject({
      method: 'POST',
      url: `/api/sessions/${created.id}/messages`,
      payload: {},
    });
    expect(res.statusCode).toBe(413);
    await app.close();
  });

  it('GET /api/llm-status never exposes the api key', async () => {
    const app = buildRouter({
      state: makeState({
        llmConfigStore: {
          read: () => ({ provider: 'deepseek', api_key: 'sk-secret', base_url: null, model: null }),
          write: () => {},
        },
      }),
    });
    const res = await app.inject({ method: 'GET', url: '/api/llm-status' });
    expect(res.statusCode).toBe(200);
    const body = res.json();
    expect(body.configured).toBe(true);
    expect(body.provider).toBe('deepseek');
    expect(JSON.stringify(body)).not.toContain('sk-secret');
    await app.close();
  });

  it('DELETE /api/sessions/:sid removes a session and returns 404 afterward', async () => {
    const app = buildRouter({ state: makeState() });
    const created = (await app.inject({ method: 'POST', url: '/api/sessions' })).json();
    const deleted = await app.inject({ method: 'DELETE', url: `/api/sessions/${created.id}` });
    expect(deleted.statusCode).toBe(204);

    const missing = await app.inject({ method: 'GET', url: `/api/sessions/${created.id}` });
    expect(missing.statusCode).toBe(404);
    const list = (await app.inject({ method: 'GET', url: '/api/sessions' })).json();
    expect(list.some((s: { id: string }) => s.id === created.id)).toBe(false);
    await app.close();
  });

  it('GET /api/llm-status unconfigured → configured:false', async () => {
    const app = buildRouter({ state: makeState() });
    const res = await app.inject({ method: 'GET', url: '/api/llm-status' });
    expect(res.json()).toEqual({ configured: false, provider: null, base_url: null, model: null });
    await app.close();
  });

  it('POST /api/llm-config saves on probe success, 422 on probe failure', async () => {
    let saved: unknown = null;
    const app = buildRouter({
      state: makeState({
        llmConfigStore: { read: () => null, write: (c) => { saved = c; } },
        llmProbe: { probe: async (p) => { if (p === 'openai') throw new Error('bad key'); } },
      }),
    });
    const ok = await app.inject({
      method: 'POST',
      url: '/api/llm-config',
      payload: { provider: 'deepseek', api_key: 'sk-x' },
    });
    expect(ok.statusCode).toBe(200);
    expect(saved).toMatchObject({ provider: 'deepseek' });

    const bad = await app.inject({
      method: 'POST',
      url: '/api/llm-config',
      payload: { provider: 'openai', api_key: 'sk-y' },
    });
    expect(bad.statusCode).toBe(422);
    expect(bad.json().error_code).toBe('LLM_PROBE_FAILED');
    await app.close();
  });

  it('GET /api/coverage-matrix → 503 without provider, 200 with', async () => {
    const noProvider = buildRouter({ state: makeState() });
    const res503 = await noProvider.inject({ method: 'GET', url: '/api/coverage-matrix' });
    expect(res503.statusCode).toBe(503);
    await noProvider.close();

    const withProvider = buildRouter({
      state: makeState({ coverageMatrixProvider: { get: () => coverage.getLoadedMatrix() } }),
    });
    const res200 = await withProvider.inject({ method: 'GET', url: '/api/coverage-matrix' });
    expect(res200.statusCode).toBe(200);
    expect(res200.json().algorithms).toHaveLength(17);
    await withProvider.close();
  });

  it('POST /api/sidecar/:id → 503 without provider', async () => {
    const app = buildRouter({ state: makeState() });
    const res = await app.inject({
      method: 'POST',
      url: '/api/sidecar/tableone',
      payload: { software: 'R', dataset_sha256: 'a'.repeat(64), columns: [], params: {} },
    });
    expect(res.statusCode).toBe(503);
    await app.close();
  });

  it('POST /api/snapshot/export → 503 without provider', async () => {
    const app = buildRouter({ state: makeState() });
    const res = await app.inject({
      method: 'POST',
      url: '/api/snapshot/export',
      payload: { run_id: 'r', destination: 'out.zip' },
    });
    expect(res.statusCode).toBe(503);
    await app.close();
  });

  it('enforces the audio body limit (10 MiB)', async () => {
    const app = buildRouter({ state: makeState() });
    const created = (await app.inject({ method: 'POST', url: '/api/sessions' })).json();
    const big = Buffer.alloc(11 * 1024 * 1024);
    const res = await app.inject({
      method: 'POST',
      url: `/api/sessions/${created.id}/audio`,
      payload: big,
      headers: { 'content-type': 'application/octet-stream' },
    });
    expect(res.statusCode).toBe(413);
    await app.close();
  });

  describe('SPA embedding + catch-all fallback (task 3.4)', () => {
    const enc = (s: string) => new TextEncoder().encode(s);
    const assetSource = {
      get(routePath: string) {
        if (routePath === '/assets/app.js') {
          return { bytes: enc('console.log(1)'), contentType: 'text/javascript; charset=utf-8' };
        }
        if (routePath === '/demo_cohort.csv') {
          return { bytes: enc('age,group\n42,A\n'), contentType: 'text/csv; charset=utf-8' };
        }
        return undefined;
      },
      indexHtml() {
        return {
          bytes: enc('<!doctype html><div id=root></div>'),
          contentType: 'text/html; charset=utf-8',
        };
      },
    };

    function spaApp() {
      return buildRouter({
        state: makeState(),
        installSpaFallback: true,
        spaAssetSource: assetSource,
      });
    }

    it('serves a known embedded asset by exact path', async () => {
      const app = spaApp();
      const res = await app.inject({ method: 'GET', url: '/assets/app.js' });
      expect(res.statusCode).toBe(200);
      expect(res.headers['content-type']).toContain('text/javascript');
      expect(res.body).toBe('console.log(1)');
      await app.close();
    });

    it('serves an embedded root-level public asset instead of the SPA shell', async () => {
      const app = spaApp();
      const res = await app.inject({ method: 'GET', url: '/demo_cohort.csv' });
      expect(res.statusCode).toBe(200);
      expect(res.headers['content-type']).toContain('text/csv');
      expect(res.body).toBe('age,group\n42,A\n');
      await app.close();
    });

    it('falls back to index.html for a deep link route', async () => {
      const app = spaApp();
      const res = await app.inject({ method: 'GET', url: '/sessions/abc/deep/link' });
      expect(res.statusCode).toBe(200);
      expect(res.headers['content-type']).toContain('text/html');
      expect(res.body).toContain('id=root');
      await app.close();
    });

    it('falls back to index.html for an unknown asset-looking path', async () => {
      const app = spaApp();
      const res = await app.inject({ method: 'GET', url: '/assets/missing.js' });
      expect(res.statusCode).toBe(200);
      expect(res.headers['content-type']).toContain('text/html');
      await app.close();
    });

    it('unmatched /api routes still return a JSON 404, not the SPA shell', async () => {
      const app = spaApp();
      const res = await app.inject({ method: 'GET', url: '/api/does-not-exist' });
      expect(res.statusCode).toBe(404);
      expect(res.json().error_code).toBe('NotFound');
      await app.close();
    });

    it('contract routes are unaffected by the fallback', async () => {
      const app = spaApp();
      const res = await app.inject({ method: 'GET', url: '/api/health' });
      expect(res.statusCode).toBe(200);
      expect(res.json()).toEqual({ status: 'ok' });
      await app.close();
    });
  });
});
