// server/infinisynapse.ts — InfiniSynapse 泛数据分析集成（Vibe Coding 参赛，2026-07）。
//
// 通过 InfiniSynapse Server API 以编程方式发起分析任务、查询数据源并取回结果，
// 前端只跟本后端交互，密钥永不进浏览器：
//   POST /api/ai/message (type=newTask)      —— 发起分析任务
//   GET  /api/ai_task/tasks?taskId=          —— 轮询运行状态/消息（官方 SSE 的轮询替代）
//   GET  /api/ai_task/getTaskWorkspace/:id   —— 任务产物文件列表
//   GET  /api/ai_task/downloadZip?taskId=    —— 结果打包下载
//   GET  /api/ai_database/list               —— 数据源清单
//   GET  /api/ai/ping                        —— 保存密钥前的连通性探测
//
// 密钥持久化与 llm-config 同目录同语义（%APPDATA%\stats-code\infinisynapse.json，
// 原子写，状态接口不回显密钥）。这些路由是参赛集成面，不属于 API_Contract 平价集，
// 因此不注册进 ROUTE_CONTRACTS（其长度断言保持 21 不变）。

import { existsSync, mkdirSync, readFileSync, renameSync, writeFileSync } from 'node:fs';
import { randomUUID } from 'node:crypto';
import { homedir } from 'node:os';
import { dirname, join } from 'node:path';
import type { FastifyInstance } from 'fastify';
import { z } from 'zod';

export const DEFAULT_INFINISYNAPSE_BASE_URL = 'https://app.infinisynapse.cn';

// ---------------------------------------------------------------------------
// Config store（文件持久化，密钥仅存本机）
// ---------------------------------------------------------------------------

export interface InfiniSynapseConfig {
  api_key: string;
  base_url: string;
}

export interface InfiniSynapseConfigStore {
  read(): InfiniSynapseConfig | null;
  write(config: InfiniSynapseConfig): void;
}

/** 与 llm-config.json 同目录：%APPDATA%\stats-code\ 或 ~/.config/stats-code/。 */
export function defaultInfiniSynapseConfigPath(): string {
  if (process.platform === 'win32') {
    const appData = process.env.APPDATA ?? join(homedir(), 'AppData', 'Roaming');
    return join(appData, 'stats-code', 'infinisynapse.json');
  }
  const xdg = process.env.XDG_CONFIG_HOME;
  const base = xdg && xdg.length > 0 ? xdg : join(homedir(), '.config');
  return join(base, 'stats-code', 'infinisynapse.json');
}

export function createFileInfiniSynapseStore(opts: { filePath?: string } = {}): InfiniSynapseConfigStore {
  const filePath = opts.filePath ?? defaultInfiniSynapseConfigPath();
  return {
    read(): InfiniSynapseConfig | null {
      if (!existsSync(filePath)) return null;
      try {
        const parsed: unknown = JSON.parse(readFileSync(filePath, 'utf8'));
        if (typeof parsed !== 'object' || parsed === null) return null;
        const v = parsed as Record<string, unknown>;
        if (typeof v.api_key !== 'string' || v.api_key.length === 0) return null;
        const baseUrl = typeof v.base_url === 'string' && v.base_url.length > 0
          ? v.base_url
          : DEFAULT_INFINISYNAPSE_BASE_URL;
        return { api_key: v.api_key, base_url: baseUrl };
      } catch {
        return null;
      }
    },
    write(config: InfiniSynapseConfig): void {
      mkdirSync(dirname(filePath), { recursive: true });
      const tmp = `${filePath}.tmp-${randomUUID()}`;
      writeFileSync(tmp, JSON.stringify({ version: 1, ...config }, null, 2), { mode: 0o600 });
      renameSync(tmp, filePath);
    },
  };
}

// ---------------------------------------------------------------------------
// Upstream client（Bearer 鉴权 + 统一信封解包）
// ---------------------------------------------------------------------------

class UpstreamError extends Error {}

function normalizeBaseUrl(url: string): string {
  return url.replace(/\/+$/, '');
}

/** InfiniSynapse 统一信封 { code, message, data }；code=1101/1105 为密钥失效。 */
function unwrapEnvelope(json: unknown): unknown {
  if (typeof json === 'object' && json !== null && 'code' in json) {
    const env = json as { code?: unknown; message?: unknown; data?: unknown };
    if (typeof env.code === 'number' && env.code !== 200) {
      const hint = env.code === 1101 || env.code === 1105 ? '（API Key 已过期或失效，请重新配置）' : '';
      throw new UpstreamError(`InfiniSynapse 返回业务错误 code=${env.code}${hint}：${String(env.message ?? '')}`);
    }
    if ('data' in env) return env.data;
  }
  return json;
}

interface UpstreamCallOptions {
  method?: 'GET' | 'POST';
  body?: unknown;
  timeoutMs?: number;
}

async function upstreamFetch(
  config: InfiniSynapseConfig,
  fetchImpl: typeof fetch,
  path: string,
  opts: UpstreamCallOptions = {},
): Promise<Response> {
  const url = `${normalizeBaseUrl(config.base_url)}${path}`;
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), opts.timeoutMs ?? 15_000);
  try {
    return await fetchImpl(url, {
      method: opts.method ?? 'GET',
      headers: {
        authorization: `Bearer ${config.api_key}`,
        accept: 'application/json',
        ...(opts.body !== undefined ? { 'content-type': 'application/json' } : {}),
      },
      ...(opts.body !== undefined ? { body: JSON.stringify(opts.body) } : {}),
      signal: controller.signal,
    });
  } catch (err) {
    const reason = err instanceof Error ? err.message : String(err);
    throw new UpstreamError(`无法连接 InfiniSynapse（${url}）：${reason}`);
  } finally {
    clearTimeout(timer);
  }
}

async function upstreamJson(
  config: InfiniSynapseConfig,
  fetchImpl: typeof fetch,
  path: string,
  opts: UpstreamCallOptions = {},
): Promise<unknown> {
  const res = await upstreamFetch(config, fetchImpl, path, opts);
  const text = await res.text().catch(() => '');
  if (!res.ok) {
    throw new UpstreamError(`InfiniSynapse 返回 HTTP ${res.status}：${text.trim().slice(0, 200)}`);
  }
  let json: unknown = null;
  try {
    json = text.length > 0 ? JSON.parse(text) : null;
  } catch {
    throw new UpstreamError(`InfiniSynapse 返回了非 JSON 响应：${text.trim().slice(0, 120)}`);
  }
  return unwrapEnvelope(json);
}

// ---------------------------------------------------------------------------
// 任务状态映射（轮询 /api/ai_task/tasks 的消息流，识别 completion_result）
// ---------------------------------------------------------------------------

export interface InfiniTaskStatus {
  is_running: boolean;
  completed: boolean;
  failed: boolean;
  result_text: string | null;
  latest_text: string | null;
  message_count: number;
}

interface RawMessage {
  type?: unknown;
  say?: unknown;
  ask?: unknown;
  text?: unknown;
}

export function mapTaskStatus(data: unknown): InfiniTaskStatus {
  const d = (typeof data === 'object' && data !== null ? data : {}) as {
    isRunning?: unknown;
    messages?: unknown;
  };
  const messages: RawMessage[] = Array.isArray(d.messages) ? (d.messages as RawMessage[]) : [];
  let resultText: string | null = null;
  let latestText: string | null = null;
  let failed = false;
  for (const m of messages) {
    const text = typeof m.text === 'string' ? m.text : '';
    if (m.say === 'completion_result' || m.ask === 'completion_result') {
      resultText = text || resultText || '';
    }
    if (m.say === 'error') failed = true;
    if (m.type === 'say' && text.length > 0) latestText = text.slice(0, 2000);
  }
  const isRunning = d.isRunning === true;
  return {
    is_running: isRunning,
    completed: resultText !== null,
    failed: failed && resultText === null && !isRunning,
    result_text: resultText,
    latest_text: latestText,
    message_count: messages.length,
  };
}

/** getTaskWorkspace 的 files 项可能是字符串或对象；统一压成字符串名。 */
export function mapWorkspace(data: unknown): { cwd: string; files: string[] } {
  const d = (typeof data === 'object' && data !== null ? data : {}) as { cwd?: unknown; files?: unknown };
  const files = Array.isArray(d.files)
    ? d.files
        .map((f) => {
          if (typeof f === 'string') return f;
          if (typeof f === 'object' && f !== null) {
            const o = f as Record<string, unknown>;
            if (typeof o.name === 'string') return o.name;
            if (typeof o.path === 'string') return o.path;
          }
          return '';
        })
        .filter((name) => name.length > 0)
    : [];
  return { cwd: typeof d.cwd === 'string' ? d.cwd : '', files };
}

// ---------------------------------------------------------------------------
// 路由注册
// ---------------------------------------------------------------------------

const postConfigBody = z.object({
  api_key: z.string().min(1),
  base_url: z.string().url().optional(),
});

const postAnalyzeBody = z.object({
  text: z.string().min(1).max(20_000),
});

/** taskId 由本模块用 randomUUID 生成；轮询/下载入参按同字符集校验。 */
const TASK_ID_RE = /^[0-9a-zA-Z-]{8,64}$/;

export interface RegisterInfiniSynapseOptions {
  store?: InfiniSynapseConfigStore;
  fetchImpl?: typeof fetch;
}

export function registerInfiniSynapseRoutes(
  app: FastifyInstance,
  opts: RegisterInfiniSynapseOptions = {},
): void {
  const store = opts.store ?? createFileInfiniSynapseStore();
  const fetchImpl = opts.fetchImpl ?? globalThis.fetch;

  const requireConfig = (): InfiniSynapseConfig => {
    const cfg = store.read();
    if (!cfg) {
      throw new NotConfiguredError();
    }
    return cfg;
  };

  // GET /api/infinisynapse/status — 是否已配置；不回显密钥。
  app.get('/api/infinisynapse/status', async () => {
    const cfg = store.read();
    return { configured: cfg !== null, base_url: cfg?.base_url ?? null };
  });

  // POST /api/infinisynapse/config — 先 /api/ai/ping 探测连通，成功才落盘。
  app.post('/api/infinisynapse/config', async (req, reply) => {
    const parsed = postConfigBody.safeParse(req.body);
    if (!parsed.success) {
      return reply.code(400).send({ error_code: 'InvalidRequest', message: 'api_key 不能为空，base_url 需为合法 URL' });
    }
    const candidate: InfiniSynapseConfig = {
      api_key: parsed.data.api_key.trim(),
      base_url: normalizeBaseUrl(parsed.data.base_url ?? DEFAULT_INFINISYNAPSE_BASE_URL),
    };
    try {
      await upstreamJson(candidate, fetchImpl, '/api/ai/ping', { timeoutMs: 10_000 });
    } catch (err) {
      return reply.code(422).send({
        error_code: 'InfiniSynapseProbeFailed',
        message: err instanceof Error ? err.message : 'InfiniSynapse 连通性探测失败',
      });
    }
    store.write(candidate);
    return reply.code(200).send({ configured: true, base_url: candidate.base_url });
  });

  // POST /api/infinisynapse/analyze — 发起分析任务（Server API newTask）。
  app.post('/api/infinisynapse/analyze', async (req, reply) => {
    const parsed = postAnalyzeBody.safeParse(req.body);
    if (!parsed.success) {
      return reply.code(400).send({ error_code: 'InvalidRequest', message: '分析指令 text 不能为空' });
    }
    try {
      const cfg = requireConfig();
      const taskId = randomUUID();
      const resp = (await upstreamJson(cfg, fetchImpl, '/api/ai/message', {
        method: 'POST',
        timeoutMs: 30_000,
        body: { type: 'newTask', taskId, text: parsed.data.text },
      })) as { success?: unknown; error?: unknown } | null;
      if (typeof resp === 'object' && resp !== null && 'success' in resp && resp.success !== true) {
        const detail = typeof resp.error === 'string' ? resp.error : JSON.stringify(resp.error ?? '');
        return reply.code(502).send({ error_code: 'InfiniSynapseUpstream', message: `任务创建被拒绝：${detail}` });
      }
      return reply.code(200).send({ task_id: taskId });
    } catch (err) {
      return replyIntegrationError(reply, err);
    }
  });

  // GET /api/infinisynapse/tasks/:taskId — 轮询任务状态与结果文本。
  app.get<{ Params: { taskId: string } }>('/api/infinisynapse/tasks/:taskId', async (req, reply) => {
    const taskId = req.params.taskId ?? '';
    if (!TASK_ID_RE.test(taskId)) {
      return reply.code(400).send({ error_code: 'InvalidRequest', message: '任务 ID 无效' });
    }
    try {
      const cfg = requireConfig();
      const data = await upstreamJson(cfg, fetchImpl, `/api/ai_task/tasks?taskId=${encodeURIComponent(taskId)}`);
      return reply.send(mapTaskStatus(data));
    } catch (err) {
      return replyIntegrationError(reply, err);
    }
  });

  // GET /api/infinisynapse/tasks/:taskId/files — 任务工作区产物列表。
  app.get<{ Params: { taskId: string } }>('/api/infinisynapse/tasks/:taskId/files', async (req, reply) => {
    const taskId = req.params.taskId ?? '';
    if (!TASK_ID_RE.test(taskId)) {
      return reply.code(400).send({ error_code: 'InvalidRequest', message: '任务 ID 无效' });
    }
    try {
      const cfg = requireConfig();
      const data = await upstreamJson(cfg, fetchImpl, `/api/ai_task/getTaskWorkspace/${encodeURIComponent(taskId)}`);
      return reply.send(mapWorkspace(data));
    } catch (err) {
      return replyIntegrationError(reply, err);
    }
  });

  // GET /api/infinisynapse/tasks/:taskId/download — 结果 zip 透传下载。
  app.get<{ Params: { taskId: string } }>('/api/infinisynapse/tasks/:taskId/download', async (req, reply) => {
    const taskId = req.params.taskId ?? '';
    if (!TASK_ID_RE.test(taskId)) {
      return reply.code(400).send({ error_code: 'InvalidRequest', message: '任务 ID 无效' });
    }
    try {
      const cfg = requireConfig();
      const res = await upstreamFetch(
        cfg,
        fetchImpl,
        `/api/ai_task/downloadZip?taskId=${encodeURIComponent(taskId)}`,
        { timeoutMs: 120_000 },
      );
      if (!res.ok) {
        const detail = (await res.text().catch(() => '')).trim().slice(0, 200);
        return reply.code(502).send({ error_code: 'InfiniSynapseUpstream', message: `下载失败 HTTP ${res.status}：${detail}` });
      }
      const bytes = Buffer.from(await res.arrayBuffer());
      return reply
        .header('Content-Type', res.headers.get('content-type') ?? 'application/zip')
        .header('Content-Length', String(bytes.byteLength))
        .header('Content-Disposition', `attachment; filename="infinisynapse-${taskId}.zip"`)
        .header('Cache-Control', 'no-store')
        .send(bytes);
    } catch (err) {
      return replyIntegrationError(reply, err);
    }
  });

  // GET /api/infinisynapse/tasks/:taskId/file?path= — 单个产物文件透传下载。
  // （上游 downloadZip 对部分任务返回空 zip，按官方 downloadTaskFile 端点逐文件取。）
  app.get<{ Params: { taskId: string }; Querystring: { path?: string } }>(
    '/api/infinisynapse/tasks/:taskId/file',
    async (req, reply) => {
      const taskId = req.params.taskId ?? '';
      const filePath = typeof req.query.path === 'string' ? req.query.path : '';
      if (!TASK_ID_RE.test(taskId) || filePath.length === 0 || filePath.includes('..')) {
        return reply.code(400).send({ error_code: 'InvalidRequest', message: '任务 ID 或文件路径无效' });
      }
      try {
        const cfg = requireConfig();
        const res = await upstreamFetch(
          cfg,
          fetchImpl,
          `/api/tools/storage/downloadTaskFile/${encodeURIComponent(taskId)}?path=${encodeURIComponent(filePath)}`,
          { timeoutMs: 120_000 },
        );
        if (!res.ok) {
          const detail = (await res.text().catch(() => '')).trim().slice(0, 200);
          return reply.code(502).send({ error_code: 'InfiniSynapseUpstream', message: `下载失败 HTTP ${res.status}：${detail}` });
        }
        const bytes = Buffer.from(await res.arrayBuffer());
        const baseName = filePath.split('/').pop() ?? 'result';
        return reply
          .header('Content-Type', res.headers.get('content-type') ?? 'application/octet-stream')
          .header('Content-Length', String(bytes.byteLength))
          .header('Content-Disposition', `attachment; filename="${baseName.replace(/[^\w.-]+/g, '_')}"`)
          .header('Cache-Control', 'no-store')
          .send(bytes);
      } catch (err) {
        return replyIntegrationError(reply, err);
      }
    },
  );

  // GET /api/infinisynapse/datasources — 数据源清单（管理数据源要求的读取面）。
  app.get('/api/infinisynapse/datasources', async (_req, reply) => {
    try {
      const cfg = requireConfig();
      const data = await upstreamJson(cfg, fetchImpl, '/api/ai_database/list?page=1&pageSize=100');
      const d = (typeof data === 'object' && data !== null ? data : {}) as { items?: unknown };
      const rawItems = Array.isArray(d.items) ? d.items : Array.isArray(data) ? (data as unknown[]) : [];
      const items = rawItems
        .filter((it): it is Record<string, unknown> => typeof it === 'object' && it !== null)
        .map((it) => ({
          id: it.id ?? null,
          name: typeof it.name === 'string' ? it.name : '',
          type: typeof it.type === 'string' ? it.type : '',
          enabled: it.enabled === 1 || it.enabled === true,
          description: typeof it.description === 'string' ? it.description : null,
        }));
      return reply.send({ items });
    } catch (err) {
      return replyIntegrationError(reply, err);
    }
  });
}

// ---------------------------------------------------------------------------
// 对话集成：让主聊天框像用本地 LLM 一样直接发起云端分析。
// 触发词开头（@云端 / @infini / 云端分析）→ 显式路由；orchestrator 在本地
// LLM 未配置时也可整句兜底到这里。事件形状与 AgentEvent 的 text_delta/error
// 子集一致，SSE 链路零改动。
// ---------------------------------------------------------------------------

const TRIGGER_RE = /^\s*(?:@\s*(?:infinisynapse|infini|云端)|云端分析|云分析)[:：,，\s]*/i;

/** 命中触发词则返回剥掉前缀后的正文；未命中或正文为空返回 null。 */
export function matchInfiniTrigger(text: string): string | null {
  const m = TRIGGER_RE.exec(text);
  if (!m) return null;
  const rest = text.slice(m[0].length).trim();
  return rest.length > 0 ? rest : null;
}

export type InfiniChatEvent =
  | { type: 'text_delta'; text: string }
  | { type: 'error'; payload: { error_code: string; message: string } };

export interface InfiniChatRunner {
  configured(): boolean;
  /** 发起云端任务并轮询到完成，把进度与结论作为聊天事件流吐出。 */
  run(text: string): AsyncIterable<InfiniChatEvent>;
}

export interface CreateInfiniChatRunnerOptions {
  store?: InfiniSynapseConfigStore;
  fetchImpl?: typeof fetch;
  /** 轮询间隔；默认 3s。 */
  pollIntervalMs?: number;
  /** 最长等待；默认 10 分钟。 */
  maxWaitMs?: number;
  /** 注入 sleep 以便测试免等待。 */
  sleepImpl?: (ms: number) => Promise<void>;
}

export function createInfiniChatRunner(opts: CreateInfiniChatRunnerOptions = {}): InfiniChatRunner {
  const store = opts.store ?? createFileInfiniSynapseStore();
  const fetchImpl = opts.fetchImpl ?? globalThis.fetch;
  const pollIntervalMs = opts.pollIntervalMs ?? 3_000;
  const maxWaitMs = opts.maxWaitMs ?? 10 * 60_000;
  const sleep = opts.sleepImpl ?? ((ms: number) => new Promise<void>((r) => setTimeout(r, ms)));

  return {
    configured: () => store.read() !== null,
    async *run(text: string): AsyncIterable<InfiniChatEvent> {
      const cfg = store.read();
      if (!cfg) {
        yield {
          type: 'error',
          payload: {
            error_code: 'InfiniSynapseNotConfigured',
            message: '尚未配置 InfiniSynapse API Key，请在右下角云图标面板中保存密钥。',
          },
        };
        return;
      }
      const taskId = randomUUID();
      try {
        const resp = (await upstreamJson(cfg, fetchImpl, '/api/ai/message', {
          method: 'POST',
          timeoutMs: 30_000,
          body: { type: 'newTask', taskId, text },
        })) as { success?: unknown; error?: unknown } | null;
        if (typeof resp === 'object' && resp !== null && 'success' in resp && resp.success !== true) {
          const detail = typeof resp.error === 'string' ? resp.error : JSON.stringify(resp.error ?? '');
          yield {
            type: 'error',
            payload: { error_code: 'InfiniSynapseUpstream', message: `云端任务创建被拒绝：${detail}` },
          };
          return;
        }
      } catch (err) {
        yield {
          type: 'error',
          payload: {
            error_code: 'InfiniSynapseUpstream',
            message: err instanceof Error ? err.message : String(err),
          },
        };
        return;
      }

      yield { type: 'text_delta', text: `☁️ 已提交 InfiniSynapse 云端分析（任务 ${taskId.slice(0, 8)}…），运行中…\n` };

      let waited = 0;
      let lastNotified = 0;
      while (waited < maxWaitMs) {
        await sleep(pollIntervalMs);
        waited += pollIntervalMs;
        let status: InfiniTaskStatus;
        try {
          const data = await upstreamJson(cfg, fetchImpl, `/api/ai_task/tasks?taskId=${encodeURIComponent(taskId)}`);
          status = mapTaskStatus(data);
        } catch (err) {
          yield {
            type: 'error',
            payload: {
              error_code: 'InfiniSynapseUpstream',
              message: err instanceof Error ? err.message : String(err),
            },
          };
          return;
        }
        if (status.completed) {
          yield { type: 'text_delta', text: `\n✅ 云端分析完成：\n\n${status.result_text ?? '（无文本结论）'}` };
          return;
        }
        if (status.failed) {
          yield {
            type: 'error',
            payload: { error_code: 'InfiniSynapseUpstream', message: '云端任务执行失败，请在 InfiniSynapse 控制台查看详情。' },
          };
          return;
        }
        // 每 ~30s 报一次心跳，避免长任务看起来卡死；不逐条刷进度以免污染最终消息。
        if (waited - lastNotified >= 30_000) {
          lastNotified = waited;
          yield { type: 'text_delta', text: `（仍在运行，已收到 ${status.message_count} 条云端消息…）\n` };
        }
      }
      yield {
        type: 'error',
        payload: {
          error_code: 'InfiniSynapseUpstream',
          message: `云端任务超过 ${Math.round(maxWaitMs / 60_000)} 分钟未完成，可稍后在 InfiniSynapse 控制台查看任务 ${taskId}。`,
        },
      };
    },
  };
}

class NotConfiguredError extends Error {
  constructor() {
    super('尚未配置 InfiniSynapse API Key，请先在面板中保存密钥。');
  }
}

function replyIntegrationError(
  reply: { code: (status: number) => { send: (body: Record<string, unknown>) => unknown } },
  err: unknown,
) {
  if (err instanceof NotConfiguredError) {
    return reply.code(503).send({ error_code: 'InfiniSynapseNotConfigured', message: err.message });
  }
  if (err instanceof UpstreamError) {
    return reply.code(502).send({ error_code: 'InfiniSynapseUpstream', message: err.message });
  }
  const message = err instanceof Error ? err.message : String(err);
  return reply.code(500).send({ error_code: 'InternalError', message });
}
