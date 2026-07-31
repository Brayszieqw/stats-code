// tests/unit/infinisynapse.test.ts — InfiniSynapse 集成路由（Vibe Coding 参赛面）。
//
// 注入内存 store + 假 fetch，覆盖：未配置 503、探测失败不落盘、探测成功落盘、
// newTask 透传、任务轮询的 completion_result 映射、上游信封 code!=200 → 502。

import { describe, it, expect } from 'vitest';
import {
  buildRouter,
  MemSessionStore,
  mapTaskStatus,
  matchInfiniTrigger,
  createInfiniChatRunner,
  type AppState,
  type InfiniChatEvent,
  type InfiniSynapseConfig,
  type InfiniSynapseConfigStore,
} from '@stats-code/server';

function memStore(initial: InfiniSynapseConfig | null = null): InfiniSynapseConfigStore {
  let cfg = initial;
  return {
    read: () => cfg,
    write: (c) => {
      cfg = c;
    },
  };
}

function makeState(): AppState {
  return { sessionStore: new MemSessionStore() };
}

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

const CFG: InfiniSynapseConfig = { api_key: 'sk-test', base_url: 'https://app.infinisynapse.cn' };

describe('InfiniSynapse integration routes', () => {
  it('GET /api/infinisynapse/status → configured:false when no key stored', async () => {
    const app = buildRouter({ state: makeState(), infiniSynapse: { store: memStore() } });
    const res = await app.inject({ method: 'GET', url: '/api/infinisynapse/status' });
    expect(res.statusCode).toBe(200);
    expect(res.json()).toEqual({ configured: false, base_url: null });
    await app.close();
  });

  it('POST /api/infinisynapse/config probes ping and persists on success', async () => {
    const store = memStore();
    const calls: string[] = [];
    const fetchImpl = (async (url: RequestInfo | URL, init?: RequestInit) => {
      calls.push(String(url));
      expect((init?.headers as Record<string, string>).authorization).toBe('Bearer sk-new');
      return jsonResponse({ code: 200, message: 'success', data: { ok: true } });
    }) as typeof fetch;
    const app = buildRouter({ state: makeState(), infiniSynapse: { store, fetchImpl } });
    const res = await app.inject({
      method: 'POST',
      url: '/api/infinisynapse/config',
      payload: { api_key: 'sk-new' },
    });
    expect(res.statusCode).toBe(200);
    expect(calls[0]).toBe('https://app.infinisynapse.cn/api/ai/ping');
    expect(store.read()?.api_key).toBe('sk-new');
    await app.close();
  });

  it('POST /api/infinisynapse/config → 422 and no persist when the probe fails', async () => {
    const store = memStore();
    const fetchImpl = (async () => jsonResponse({ code: 1101, message: 'token expired' })) as typeof fetch;
    const app = buildRouter({ state: makeState(), infiniSynapse: { store, fetchImpl } });
    const res = await app.inject({
      method: 'POST',
      url: '/api/infinisynapse/config',
      payload: { api_key: 'sk-bad' },
    });
    expect(res.statusCode).toBe(422);
    expect(res.json().error_code).toBe('InfiniSynapseProbeFailed');
    expect(store.read()).toBeNull();
    await app.close();
  });

  it('POST /api/infinisynapse/analyze forwards a newTask message and returns task_id', async () => {
    let sentBody: Record<string, unknown> | null = null;
    const fetchImpl = (async (url: RequestInfo | URL, init?: RequestInit) => {
      expect(String(url)).toBe('https://app.infinisynapse.cn/api/ai/message');
      sentBody = JSON.parse(String(init?.body)) as Record<string, unknown>;
      return jsonResponse({ code: 200, message: 'success', data: { success: true } });
    }) as typeof fetch;
    const app = buildRouter({ state: makeState(), infiniSynapse: { store: memStore(CFG), fetchImpl } });
    const res = await app.inject({
      method: 'POST',
      url: '/api/infinisynapse/analyze',
      payload: { text: '分析销售趋势' },
    });
    expect(res.statusCode).toBe(200);
    const taskId = res.json().task_id as string;
    expect(taskId.length).toBeGreaterThan(8);
    expect(sentBody).toMatchObject({ type: 'newTask', text: '分析销售趋势', taskId });
    await app.close();
  });

  it('analyze without a stored key → 503 InfiniSynapseNotConfigured', async () => {
    const app = buildRouter({ state: makeState(), infiniSynapse: { store: memStore() } });
    const res = await app.inject({
      method: 'POST',
      url: '/api/infinisynapse/analyze',
      payload: { text: 'x' },
    });
    expect(res.statusCode).toBe(503);
    expect(res.json().error_code).toBe('InfiniSynapseNotConfigured');
    await app.close();
  });

  it('GET /api/infinisynapse/tasks/:id maps completion_result and isRunning', async () => {
    const fetchImpl = (async () =>
      jsonResponse({
        code: 200,
        message: 'success',
        data: {
          isRunning: false,
          messages: [
            { type: 'say', say: 'text', text: '正在分析…' },
            { type: 'say', say: 'completion_result', text: '组间发病率差异显著（p<0.05）' },
          ],
        },
      })) as typeof fetch;
    const app = buildRouter({ state: makeState(), infiniSynapse: { store: memStore(CFG), fetchImpl } });
    const res = await app.inject({ method: 'GET', url: '/api/infinisynapse/tasks/abcd1234-task' });
    expect(res.statusCode).toBe(200);
    expect(res.json()).toMatchObject({
      is_running: false,
      completed: true,
      failed: false,
      result_text: '组间发病率差异显著（p<0.05）',
      message_count: 2,
    });
    await app.close();
  });

  it('upstream envelope code=1105 on task poll → 502 with a reconfigure hint', async () => {
    const fetchImpl = (async () => jsonResponse({ code: 1105, message: 'invalid token' })) as typeof fetch;
    const app = buildRouter({ state: makeState(), infiniSynapse: { store: memStore(CFG), fetchImpl } });
    const res = await app.inject({ method: 'GET', url: '/api/infinisynapse/tasks/abcd1234-task' });
    expect(res.statusCode).toBe(502);
    expect(res.json().error_code).toBe('InfiniSynapseUpstream');
    expect(res.json().message).toContain('失效');
    await app.close();
  });

  it('mapTaskStatus: running task with no completion is neither completed nor failed', () => {
    const status = mapTaskStatus({ isRunning: true, messages: [{ type: 'say', say: 'text', text: '…' }] });
    expect(status).toMatchObject({ is_running: true, completed: false, failed: false });
  });
});

describe('InfiniSynapse chat trigger + runner (对话框云端分析)', () => {
  it('matchInfiniTrigger strips @云端/@infini/云端分析 prefixes, null otherwise', () => {
    expect(matchInfiniTrigger('@云端 分析销售趋势')).toBe('分析销售趋势');
    expect(matchInfiniTrigger('@infini: analyze sales')).toBe('analyze sales');
    expect(matchInfiniTrigger('云端分析：各组发病率差异')).toBe('各组发病率差异');
    expect(matchInfiniTrigger('普通的本地统计问题')).toBeNull();
    expect(matchInfiniTrigger('@云端   ')).toBeNull();
  });

  it('runner streams submit → poll → completion_result as chat text', async () => {
    let polls = 0;
    const fetchImpl = (async (url: RequestInfo | URL) => {
      const u = String(url);
      if (u.endsWith('/api/ai/message')) {
        return jsonResponse({ code: 200, message: 'success', data: { success: true } });
      }
      polls += 1;
      return jsonResponse({
        code: 200,
        message: 'success',
        data:
          polls < 2
            ? { isRunning: true, messages: [{ type: 'say', say: 'text', text: 'working' }] }
            : {
                isRunning: false,
                messages: [{ type: 'say', say: 'completion_result', text: '均值 54.75，SD 25.36' }],
              },
      });
    }) as typeof fetch;
    const runner = createInfiniChatRunner({
      store: memStore(CFG),
      fetchImpl,
      pollIntervalMs: 1,
      sleepImpl: async () => {},
    });
    const events: InfiniChatEvent[] = [];
    for await (const e of runner.run('分析随机数')) events.push(e);
    const text = events
      .filter((e): e is Extract<InfiniChatEvent, { type: 'text_delta' }> => e.type === 'text_delta')
      .map((e) => e.text)
      .join('');
    expect(text).toContain('已提交 InfiniSynapse 云端分析');
    expect(text).toContain('均值 54.75，SD 25.36');
    expect(events.some((e) => e.type === 'error')).toBe(false);
  });

  it('runner yields a friendly error when unconfigured', async () => {
    const runner = createInfiniChatRunner({ store: memStore(), sleepImpl: async () => {} });
    const events: InfiniChatEvent[] = [];
    for await (const e of runner.run('x')) events.push(e);
    expect(events).toHaveLength(1);
    expect(events[0]).toMatchObject({ type: 'error' });
  });
});
