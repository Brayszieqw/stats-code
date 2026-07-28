// server/router.ts — Fastify router with all 13 API_Contract routes (task 3.2).
// Transcribed from crates/agent-server/src/lib.rs (build_router) and handlers/*.
//
// Each route validates its request/response against the zod contract schemas,
// returns the same status codes as the Rust backend, and enforces the per-route
// body limits (datasets 70 MiB base64, audio 10 MiB). SSE for the messages
// route (task 3.3) and the SPA fallback (task 3.4) are layered on separately.

import { randomUUID } from 'node:crypto';
import { mkdirSync, readFileSync } from 'node:fs';
import { basename, join } from 'node:path';
import Fastify, { type FastifyInstance } from 'fastify';
import { sidecar as engineSidecar, snapshot as engineSnapshot } from '@stats-code/engine';
import {
  domain,
  patchSettingsRequest,
  patchResearchProtocolRequest,
  base64DatasetRequest,
  postLlmConfigRequest,
  sidecar as sidecarContract,
} from './contract/index.js';
import { StoreError, type AgentBlock, type AgentEvent, type AppState, type Message } from './state.js';
import { serializeSseFrame } from './sse.js';
import { installSpaFallback, type SpaAssetSource } from './spa.js';
import { createDefaultAssetSource } from './spa-assets.js';
import { statusFromConfig, testAndSaveConfig, LlmConfigError, providerRequiresOAuth } from './llm.js';
import { extractPreviewRows, sanitizePreviewRows } from './conversation/dataset-store.js';
import { SpeechTranscribeError, transcribeAudio } from './conversation/speech-transcribe.js';
import { ResearchWorkflowError } from './conversation/research-workflow.js';
import { ProtocolCompilerError } from './conversation/protocol-compiler.js';
import { protocolContentSha256, protocolStateSha256 } from './conversation/research-integrity.js';

const AUDIO_BODY_LIMIT = 10 * 1024 * 1024;
const DATASET_BODY_LIMIT = 70 * 1024 * 1024;

/**
 * Map snapshot export failures to stable SPA-facing error codes.
 * "run not found" is expected after backend restart (in-memory registry), not a 500.
 * Engine SnapshotError kinds map to the statuses the SPA already decodes
 * (409 RunNotCompleted / 413 PayloadTooLarge — see web useSnapshotExport).
 */
function replySnapshotError(
  reply: { code: (status: number) => { send: (body: Record<string, unknown>) => unknown } },
  err: unknown,
) {
  const message = err instanceof Error ? err.message : String(err);
  if (err instanceof engineSnapshot.SnapshotError) {
    switch (err.kind) {
      case 'run_not_completed':
        return reply.code(409).send({
          error_code: 'RunNotCompleted',
          message,
          actual_status: err.detail?.actual ?? null,
        });
      case 'payload_too_large':
        return reply.code(413).send({
          error_code: 'PayloadTooLarge',
          message,
          measured_bytes: err.detail?.measuredBytes ?? null,
          ceiling_bytes: err.detail?.ceilingBytes ?? null,
        });
      case 'bad_destination':
        return reply.code(400).send({ error_code: 'InvalidRequest', message });
      default:
        break;
    }
  }
  if (/run not found/i.test(message)) {
    return reply.code(404).send({
      error_code: 'RunNotFound',
      message:
        '找不到该次分析的导出记录（后端重启后内存记录会清空）。请重新运行分析后再导出审计快照。',
    });
  }
  return reply.code(500).send({
    error_code: 'InternalError',
    message: message || '导出审计快照失败',
  });
}

function userTextMessage(text: string): Message {
  return {
    User: {
      id: randomUUID(),
      created_at: new Date().toISOString(),
      content: { Text: text },
    },
  };
}

function agentMessage(blocks: AgentBlock[]): Message {
  return {
    Agent: {
      id: randomUUID(),
      created_at: new Date().toISOString(),
      blocks,
    },
  };
}

function appendAgentBlockFromEvent(
  event: AgentEvent,
  blocks: AgentBlock[],
  textBuffer: { value: string },
): void {
  const flushText = () => {
    if (textBuffer.value.length === 0) return;
    blocks.push({ Text: textBuffer.value });
    textBuffer.value = '';
  };

  switch (event.type) {
    case 'text_delta':
      textBuffer.value += event.text;
      break;
    case 'choice_prompt':
      flushText();
      blocks.push({ ChoicePrompt: event.prompt } as AgentBlock);
      break;
    case 'skill_call':
      flushText();
      blocks.push({ Text: `[正在执行: ${event.skill_id}]` });
      break;
    case 'skill_result': {
      flushText();
      const result = event.result as { analysis?: { run_id?: unknown } };
      const runId = typeof result.analysis?.run_id === 'string' ? result.analysis.run_id : randomUUID();
      blocks.push({ SkillResult: { run_id: runId, result: event.result } } as AgentBlock);
      break;
    }
    case 'interpretation':
      flushText();
      blocks.push({ Interpretation: event.text });
      break;
    case 'error':
    case 'done':
      flushText();
      break;
  }
}

/** Map a StoreError to the Rust status + error_code (see session.rs). */
function storeErrorResponse(err: StoreError): { status: number; body: unknown } {
  switch (err.kind) {
    case 'not_found':
      return { status: 404, body: { error_code: 'SessionNotFound', message: '会话不存在或已被删除' } };
    case 'archived':
      return { status: 409, body: { error_code: 'SessionArchived', message: '会话已归档，仅支持只读访问' } };
    default:
      return { status: 500, body: { error_code: 'SkillExecutionFailed', message: err.message } };
  }
}

function workflowErrorResponse(err: ResearchWorkflowError): { status: number; body: unknown } {
  return {
    status: err.status,
    body: {
      error_code: err.code,
      message: err.message,
      ...(err.details === undefined ? {} : { details: err.details }),
    },
  };
}

/** Structural SkillRunError detail (avoids importing the conversation layer). */
interface SkillRunErrorDetail {
  kind: 'timeout' | 'invalid_args' | 'execution_failed';
  message?: string;
  missing?: string[];
  diagnosticExcerpt?: string;
  wallSecs?: number;
}

function skillRunDetail(err: unknown): SkillRunErrorDetail | null {
  const detail = (err as { detail?: unknown } | null)?.detail;
  if (
    detail &&
    typeof detail === 'object' &&
    typeof (detail as { kind?: unknown }).kind === 'string'
  ) {
    return detail as SkillRunErrorDetail;
  }
  return null;
}

/** Map a SkillRunError → HTTP status (R12.4 invalid_args→422, R12.5 timeout→504). */
function runErrorStatus(err: unknown): number {
  const detail = skillRunDetail(err);
  switch (detail?.kind) {
    case 'invalid_args':
      return 422;
    case 'timeout':
      return 504;
    default:
      return 500;
  }
}

/** Map a SkillRunError → contract ErrorPayload body. */
function runErrorBody(err: unknown): { error_code: string; message: string } {
  const detail = skillRunDetail(err);
  switch (detail?.kind) {
    case 'invalid_args':
      return { error_code: 'SkillInvalidArgs', message: detail.message ?? '缺少必需参数' };
    case 'timeout':
      return { error_code: 'SkillTimeout', message: `运行超时（${detail.wallSecs ?? 120}s）` };
    case 'execution_failed':
      return { error_code: 'SkillExecutionFailed', message: detail.diagnosticExcerpt ?? '运行失败' };
    default:
      return { error_code: 'SkillExecutionFailed', message: (err as Error)?.message ?? '运行失败' };
  }
}

export interface BuildRouterOptions {
  state: AppState;
  /** Install the SPA fallback (task 3.4). Prod enables this; tests may opt in. */
  installSpaFallback?: boolean;
  /** Override the embedded asset source (defaults to SEA/disk auto-detect). */
  spaAssetSource?: SpaAssetSource;
}

/**
 * Build the Fastify instance with all contract routes. The caller binds it to
 * 127.0.0.1 (the launcher) — this function does not listen.
 */
export function buildRouter(opts: BuildRouterOptions): FastifyInstance {
  const { state } = opts;
  const app = Fastify({
    bodyLimit: DATASET_BODY_LIMIT, // global ceiling; per-route limits below
    logger: false,
  });

  // Fastify's own content-type/body errors bypass every route handler, so they
  // used to reach the SPA as `{statusCode, code: 'FST_ERR_...', error, message}`
  // with an English message. The SPA decodes `{error_code, message}` and shows
  // `message` verbatim, so an oversized upload surfaced as a raw English string
  // in an otherwise Chinese UI. Normalise the transport-layer failures we can
  // actually hit; anything else keeps Fastify's default handling.
  app.setErrorHandler((err, _req, reply) => {
    if (err.code === 'FST_ERR_CTP_BODY_TOO_LARGE') {
      const limitMb = Math.floor(DATASET_BODY_LIMIT / 1024 / 1024);
      return reply.code(413).send({
        error_code: 'PayloadTooLarge',
        message: `请求体超出上限 ${limitMb} MB。数据文件会以 base64 编码传输（体积约为原文件的 1.34 倍），`
          + '请先抽取所需变量与观测后再上传。',
      });
    }
    if (err.code === 'FST_ERR_CTP_INVALID_MEDIA_TYPE') {
      return reply.code(415).send({
        error_code: 'UnsupportedMediaType',
        message: '不支持的请求内容类型，数据上传请使用 application/json。',
      });
    }
    // 空 body 有专属错误码；语法错误的 JSON 由 Fastify 的解析器直接抛 SyntaxError
    // （没有 FST_ 前缀），所以两者都要判，否则后者会掉进兜底分支拿到无信息的文案。
    if (
      err.code === 'FST_ERR_CTP_EMPTY_JSON_BODY'
      || err.code === 'FST_ERR_CTP_INVALID_JSON_BODY'
      || err instanceof SyntaxError
    ) {
      return reply.code(400).send({
        error_code: 'SkillInvalidArgs',
        message: '请求体不是合法的 JSON。',
      });
    }
    const status = typeof err.statusCode === 'number' && err.statusCode >= 400 ? err.statusCode : 500;
    return reply.code(status).send({
      error_code: status >= 500 ? 'SkillExecutionFailed' : 'SkillInvalidArgs',
      message: status >= 500 ? '服务端处理请求时发生未预期的错误。' : '请求无法处理。',
    });
  });

  // Same-origin CORS (S1). This API is an unauthenticated localhost service:
  // a wildcard ACAO would let any web page in the user's browser drive it
  // cross-origin (including the DNS-rebinding surface). Only loopback origins
  // (the SPA itself or a local dev server) are reflected; every other origin
  // gets no CORS headers, and its state-changing requests are refused outright.
  //
  // onSend alone is not enough for browser preflight: OPTIONS has no matching
  // route, so it used to fall through to the /api not-found handler (404) while
  // still advertising Allow-Methods: OPTIONS. Answer OPTIONS for /api/* with
  // 204 before routing.
  const CORS_ALLOW_METHODS = 'GET,POST,PATCH,DELETE,OPTIONS';
  const LOOPBACK_ORIGIN = /^https?:\/\/(127\.0\.0\.1|localhost|\[::1\])(:\d+)?$/i;
  // Online-demo mode: STATS_CODE_PUBLIC_HOSTS lists extra hostnames (comma-
  // separated, e.g. tunnel domains) allowed through the Host/Origin gates.
  // Loopback-only remains the default; this is opt-in for demo deployments.
  const publicHosts = new Set(
    (process.env.STATS_CODE_PUBLIC_HOSTS ?? '')
      .split(',')
      .map((h) => h.trim().toLowerCase())
      .filter((h) => h.length > 0),
  );
  const hostAllowed = (hostName: string): boolean => {
    if (LOOPBACK_ORIGIN.test(`http://${hostName}`)) return true;
    const bare = hostName.replace(/:\d+$/, '').toLowerCase();
    return publicHosts.has(bare);
  };
  const trustedOrigin = (req: { headers: { origin?: string | string[] | undefined } }): string | null => {
    const origin = req.headers.origin;
    if (typeof origin !== 'string' || origin.length === 0) return null;
    if (LOOPBACK_ORIGIN.test(origin)) return origin;
    try {
      const url = new URL(origin);
      if (publicHosts.has(url.hostname.toLowerCase())) return origin;
    } catch {
      // fall through
    }
    return null;
  };
  const corsAllowHeaders = (req: { headers: { [key: string]: string | string[] | undefined } }): string => {
    const requested = req.headers['access-control-request-headers'];
    if (typeof requested === 'string' && requested.length > 0) return requested;
    if (Array.isArray(requested) && requested.length > 0) return requested.join(',');
    return '*';
  };

  app.addHook('onRequest', async (req, reply) => {
    const path = req.url.split('?')[0] ?? req.url;
    if (!path.startsWith('/api/')) return;

    // DNS-rebinding defense (S1 残余闭环): a rebound hostname reaches this
    // server with Host: attacker.tld and NO Origin header (the browser deems
    // it same-origin), so the Origin checks below never fire. The service only
    // ever binds loopback, so any non-loopback Host is hostile or misrouted.
    const hostHeader = req.headers.host;
    const hostName = typeof hostHeader === 'string' ? hostHeader.trim() : '';
    if (!hostAllowed(hostName)) {
      return reply.code(403).send({
        error_code: 'ForbiddenHost',
        message: '非本机 Host 的请求被拒绝：本地 API 仅接受 127.0.0.1/localhost 访问。',
      });
    }

    const origin =
      typeof req.headers.origin === 'string' && req.headers.origin.length > 0
        ? req.headers.origin
        : undefined;
    const trusted = trustedOrigin(req);

    if (req.method === 'OPTIONS') {
      if (origin !== undefined && trusted === null) {
        // Cross-origin preflight: refuse without any CORS headers.
        return reply.code(403).send();
      }
      if (trusted !== null) {
        reply.header('access-control-allow-origin', trusted);
        reply.header('vary', 'origin');
        reply.header('access-control-allow-methods', CORS_ALLOW_METHODS);
        reply.header('access-control-allow-headers', corsAllowHeaders(req));
      }
      return reply.code(204).send();
    }

    // A state-changing cross-origin request must not execute at all — CORS
    // only hides the response from the page; it does not stop the side effect.
    if (
      (req.method === 'POST' || req.method === 'PATCH' || req.method === 'DELETE') &&
      origin !== undefined &&
      trusted === null
    ) {
      return reply.code(403).send({
        error_code: 'ForbiddenOrigin',
        message: '跨源请求被拒绝：本地 API 仅接受同源（127.0.0.1/localhost）访问。',
      });
    }
  });

  app.addHook('onSend', (req, reply, payload, done) => {
    const trusted = trustedOrigin(req);
    if (trusted !== null) {
      reply.header('access-control-allow-origin', trusted);
      reply.header('vary', 'origin');
    }
    done(null, payload);
  });

  // Raw binary parser for audio uploads (application/octet-stream). Respects
  // the per-route bodyLimit, so oversized payloads surface as 413 not 415.
  app.addContentTypeParser(
    'application/octet-stream',
    { parseAs: 'buffer' },
    (_req, body, done) => {
      done(null, body);
    },
  );

  // GET /api/health
  app.get('/api/health', async () => ({ status: 'ok' }));

  // POST /api/sessions → 201
  app.post('/api/sessions', async (_req, reply) => {
    const session = await state.sessionStore.create();
    return reply.code(201).send(session);
  });

  // GET /api/sessions → 200 with session summaries (empty → []). Requirement 11.
  app.get('/api/sessions', async (_req, reply) => {
    const summaries = await state.sessionStore.list();
    return reply.send(summaries);
  });

  // GET /api/sessions/:sid
  app.get<{ Params: { sid: string } }>('/api/sessions/:sid', async (req, reply) => {
    try {
      const session = await state.sessionStore.get(req.params.sid);
      // Re-sanitize legacy previews before returning them; enrich only when absent.
      if (Array.isArray(session.datasets)) {
        for (const ds of session.datasets) {
          if (ds.preview_rows && ds.preview_rows.length > 0) {
            ds.preview_rows = sanitizePreviewRows(ds.preview_rows);
            continue;
          }
          if (!state.datasetStore) continue;
          try {
            const raw = await state.datasetStore.readRawById(ds.dataset_id);
            ds.preview_rows = extractPreviewRows(raw, ds.file_name);
          } catch {
            /* leave without preview */
          }
        }
      }
      return reply.send(session);
    } catch (err) {
      if (err instanceof StoreError) {
        const { status, body } = storeErrorResponse(err);
        return reply.code(status).send(body);
      }
      throw err;
    }
  });

  // DELETE /api/sessions/:sid
  app.delete<{ Params: { sid: string } }>('/api/sessions/:sid', async (req, reply) => {
    try {
      await state.sessionStore.deleteSession(req.params.sid);
      return reply.code(204).send();
    } catch (err) {
      if (err instanceof StoreError) {
        const { status, body } = storeErrorResponse(err);
        return reply.code(status).send(body);
      }
      throw err;
    }
  });

  // PATCH /api/sessions/:sid/settings
  app.patch<{ Params: { sid: string } }>('/api/sessions/:sid/settings', async (req, reply) => {
    const parsed = patchSettingsRequest.safeParse(req.body);
    if (!parsed.success) {
      return reply.code(422).send({ error_code: 'SkillInvalidArgs', message: 'invalid settings body' });
    }
    try {
      const session = await state.sessionStore.get(req.params.sid);
      if (session.status === 'Archived') {
        return reply.code(409).send({ error_code: 'SessionArchived', message: '会话已归档，仅支持只读访问' });
      }
      await state.sessionStore.updateSettings(req.params.sid, {
        decision_assistant: parsed.data.decision_assistant,
      });
      const updated = await state.sessionStore.get(req.params.sid);
      return reply.send(updated);
    } catch (err) {
      if (err instanceof StoreError) {
        const { status, body } = storeErrorResponse(err);
        return reply.code(status).send(body);
      }
      throw err;
    }
  });

  // POST /api/sessions/:sid/protocol/compile — review-only LLM proposal; never persists.
  app.post<{ Params: { sid: string } }>('/api/sessions/:sid/protocol/compile', async (req, reply) => {
    const parsed = domain.protocolCompileRequest.safeParse(req.body);
    if (!parsed.success) {
      return reply.code(422).send({
        error_code: 'SkillInvalidArgs',
        message: '研究摘要需为 20–8000 个字符，且不能包含服务端审批字段。',
      });
    }
    try {
      const session = await state.sessionStore.get(req.params.sid);
      if (session.status === 'Archived') {
        return reply.code(409).send({ error_code: 'SessionArchived', message: '会话已归档，仅支持只读访问' });
      }
      if (!state.protocolCompiler) {
        return reply.code(502).send({ error_code: 'LlmUnavailable', message: 'LLM 未配置；可继续使用手工协议表单。' });
      }
      return reply.send(await state.protocolCompiler.compile(parsed.data, { sessionId: req.params.sid }));
    } catch (err) {
      if (err instanceof StoreError) {
        const { status, body } = storeErrorResponse(err);
        return reply.code(status).send(body);
      }
      if (err instanceof ProtocolCompilerError) {
        return reply.code(err.code === 'SkillInvalidArgs' ? 422 : 502).send({
          error_code: err.code,
          message: err.message,
        });
      }
      throw err;
    }
  });

  // PATCH /api/sessions/:sid/protocol — save a draft or approve a complete protocol.
  app.patch<{ Params: { sid: string } }>('/api/sessions/:sid/protocol', async (req, reply) => {
    const parsed = patchResearchProtocolRequest.safeParse(req.body);
    if (!parsed.success) {
      return reply.code(422).send({
        error_code: 'SkillInvalidArgs',
        message: '研究协议字段不完整，无法保存或审批',
      });
    }
    try {
      const session = await state.sessionStore.get(req.params.sid);
      if (session.status === 'Archived') {
        return reply.code(409).send({ error_code: 'SessionArchived', message: '会话已归档，仅支持只读访问' });
      }
      const current = session.research_protocol ?? null;
      if (current && parsed.data.expected_version === undefined) {
        return reply.code(409).send({
          error_code: 'ResearchVersionConflict',
          message: `更新已有协议必须提供 expected_version（当前为 v${current.version}）。`,
          details: { current_version: current.version },
        });
      }
      if (current && parsed.data.expected_version !== current.version) {
        return reply.code(409).send({
          error_code: 'ResearchVersionConflict',
          message: `协议版本冲突：当前为 v${current.version}，请求基于 v${parsed.data.expected_version}。`,
          details: { current_version: current.version, expected_version: parsed.data.expected_version },
        });
      }
      const { status, expected_version: _expectedVersion, ...fields } = parsed.data;
      const contentSha256 = protocolContentSha256(fields);
      const sameContent = current?.content_sha256 === contentSha256;
      const sameState = sameContent && current?.status === status;
      // `version` is also the compare-and-swap revision. Any semantic state
      // transition must advance it, even when the protocol text is unchanged,
      // so a stale request cannot revoke and then resurrect an approval.
      const version = current ? (sameState ? current.version : current.version + 1) : 1;
      const timestamp = (state.researchWorkflow?.now() ?? new Date()).toISOString();
      const preserveApproval = status === 'Approved'
        && sameContent
        && current?.status === 'Approved'
        && current.approval_id !== null;
      const protocolState = {
        ...fields,
        status,
        version,
        content_sha256: contentSha256,
        approval_id: status === 'Approved'
          ? (preserveApproval ? current.approval_id : randomUUID())
          : null,
        approved_at: status === 'Approved'
          ? (preserveApproval ? current.approved_at : timestamp)
          : null,
        updated_at: timestamp,
      };
      const updated = await state.sessionStore.updateResearchProtocol(req.params.sid, {
        ...protocolState,
        state_sha256: protocolStateSha256(protocolState),
      }, parsed.data.expected_version);
      if (!updated) {
        const latest = await state.sessionStore.get(req.params.sid);
        return reply.code(409).send({
          error_code: 'ResearchVersionConflict',
          message: `协议版本冲突：当前为 v${latest.research_protocol?.version ?? 0}。`,
          details: {
            current_version: latest.research_protocol?.version ?? null,
            expected_version: parsed.data.expected_version ?? null,
          },
        });
      }
      return reply.send(await state.sessionStore.get(req.params.sid));
    } catch (err) {
      if (err instanceof StoreError) {
        const { status, body } = storeErrorResponse(err);
        return reply.code(status).send(body);
      }
      throw err;
    }
  });

  // POST /api/sessions/:sid/messages — SSE (full streaming wired in task 3.3).
  app.post<{ Params: { sid: string } }>('/api/sessions/:sid/messages', async (req, reply) => {
    const body = (req.body ?? {}) as { text?: string; content?: { type: string; text: string } };
    const text = body.text ?? (body.content?.type === 'text' ? body.content.text : undefined);
    if (text === undefined) {
      return reply.code(422).send({ error_code: 'SkillInvalidArgs', message: '请求体缺少 text 字段' });
    }
    if ([...text].length > 8000) {
      return reply.code(413).send({ error_code: 'MessageTooLong', message: '消息过长' });
    }
    try {
      const session = await state.sessionStore.get(req.params.sid);
      if (session.status === 'Archived') {
        return reply.code(409).send({ error_code: 'SessionArchived', message: '会话已归档，仅支持只读访问' });
      }
    } catch (err) {
      if (err instanceof StoreError) {
        const { status, body: errBody } = storeErrorResponse(err);
        return reply.code(status).send(errBody);
      }
      throw err;
    }

    try {
      await state.sessionStore.appendMessages(req.params.sid, [userTextMessage(text)]);
    } catch (err) {
      if (err instanceof StoreError) {
        const { status, body: errBody } = storeErrorResponse(err);
        return reply.code(status).send(errBody);
      }
      throw err;
    }

    // Stream the orchestrator's AgentEvents as SSE frames (task 3.3). When no
    // message handler is configured (Phase-0 scaffold), emit a single terminal
    // `done` frame so the SSE contract shape still holds.
    reply.raw.writeHead(200, {
      'content-type': 'text/event-stream; charset=utf-8',
      'cache-control': 'no-cache',
      connection: 'keep-alive',
    });

    const handler = state.messageHandler;
    if (!handler) {
      reply.raw.write(serializeSseFrame({ type: 'done' }));
      reply.raw.end();
      return reply;
    }

    const agentBlocks: AgentBlock[] = [];
    const textBuffer = { value: '' };

    try {
      const session = await state.sessionStore.get(req.params.sid);
      const stream = handler.handleMessage(req.params.sid, {
        text,
        settings: session.settings,
      });
      for await (const event of stream) {
        appendAgentBlockFromEvent(event, agentBlocks, textBuffer);
        reply.raw.write(serializeSseFrame(event));
      }
    } catch (err) {
      // Mid-stream failure: emit a structured error frame then terminate. The
      // HTTP status is already 200 (headers flushed), matching the Rust SSE
      // behavior where errors surface as `event: error` frames.
      const errorEvent: AgentEvent = {
        type: 'error',
        payload: { error_code: 'SkillExecutionFailed', message: (err as Error).message },
      };
      appendAgentBlockFromEvent(errorEvent, agentBlocks, textBuffer);
      reply.raw.write(serializeSseFrame(errorEvent));
    }
    appendAgentBlockFromEvent({ type: 'done' }, agentBlocks, textBuffer);
    if (agentBlocks.length > 0) {
      try {
        await state.sessionStore.appendMessages(req.params.sid, [agentMessage(agentBlocks)]);
      } catch {
        // The SSE response has already been emitted; a concurrent deletion should not corrupt the stream.
      }
    }
    reply.raw.end();
    return reply;
  });

  // POST /api/sessions/:sid/audio (10 MiB limit) — Whisper-compatible STT.
  app.post<{ Params: { sid: string } }>(
    '/api/sessions/:sid/audio',
    { bodyLimit: AUDIO_BODY_LIMIT },
    async (req, reply) => {
      try {
        await state.sessionStore.get(req.params.sid);
      } catch (err) {
        if (err instanceof StoreError) {
          const { status, body } = storeErrorResponse(err);
          return reply.code(status).send(body);
        }
        throw err;
      }

      const cfg = state.llmConfigStore?.read() ?? null;
      if (!cfg?.api_key) {
        return reply.code(502).send({
          error_code: 'LlmUnavailable',
          message:
            '语音转写需要已配置的 API Key。也可使用浏览器内置语音识别（Chrome/Edge 麦克风按钮旁会优先走本地识别）。',
        });
      }

      // Fastify may give a Buffer, Uint8Array, or (rarely) a string for raw bodies.
      const raw = req.body;
      let bytes: Uint8Array;
      if (raw instanceof Uint8Array) {
        bytes = raw;
      } else if (Buffer.isBuffer(raw)) {
        bytes = new Uint8Array(raw);
      } else if (typeof raw === 'string') {
        bytes = new TextEncoder().encode(raw);
      } else if (raw && typeof raw === 'object' && 'type' in (raw as object) && (raw as { type: string }).type === 'Buffer') {
        bytes = new Uint8Array((raw as { data: number[] }).data);
      } else {
        return reply.code(422).send({ error_code: 'SkillInvalidArgs', message: '请求体缺少音频数据' });
      }

      if (bytes.byteLength === 0) {
        return reply.code(422).send({ error_code: 'SkillInvalidArgs', message: '音频为空' });
      }

      const contentType = typeof req.headers['content-type'] === 'string' ? req.headers['content-type'] : undefined;
      try {
        const result = await transcribeAudio({
          bytes,
          contentType,
          filename: 'recording.webm',
          language: 'zh',
          config: cfg,
        });
        return reply.send(result);
      } catch (err) {
        if (err instanceof SpeechTranscribeError) {
          const status = err.code === 'SkillInvalidArgs' ? 422 : err.code === 'InternalError' ? 500 : 502;
          return reply.code(status).send({ error_code: err.code, message: err.message });
        }
        return reply.code(502).send({
          error_code: 'LlmUnavailable',
          message: (err as Error).message || '语音转写失败',
        });
      }
    },
  );

  // POST /api/sessions/:sid/datasets (70 MiB base64 limit) → 201
  app.post<{ Params: { sid: string } }>(
    '/api/sessions/:sid/datasets',
    { bodyLimit: DATASET_BODY_LIMIT },
    async (req, reply) => {
      const parsed = base64DatasetRequest.safeParse(req.body);
      if (!parsed.success) {
        return reply.code(422).send({ error_code: 'SkillInvalidArgs', message: 'invalid dataset body' });
      }
      try {
        await state.sessionStore.get(req.params.sid);
      } catch (err) {
        if (err instanceof StoreError) {
          const { status, body } = storeErrorResponse(err);
          return reply.code(status).send(body);
        }
        throw err;
      }
      // Dataset parsing/persistence requires a configured DatasetStore.
      if (!state.datasetStore) {
        return reply
          .code(500)
          .send({ error_code: 'SkillExecutionFailed', message: '数据集存储服务尚未初始化' });
      }
      // Decode the base64 payload (Requirement 6.1). Buffer.from silently maps
      // garbage input to bytes instead of throwing, so validate strictly before
      // decoding (D1): whitespace wrapping is tolerated, everything else must
      // be canonical base64 with trailing padding only.
      const normalizedB64 = parsed.data.data.replace(/\s+/g, '');
      if (!/^[A-Za-z0-9+/]*={0,2}$/.test(normalizedB64) || normalizedB64.length % 4 !== 0) {
        return reply.code(422).send({ error_code: 'SkillInvalidArgs', message: 'invalid base64 dataset' });
      }
      const bytes = new Uint8Array(Buffer.from(normalizedB64, 'base64'));
      if (bytes.byteLength === 0) {
        return reply.code(422).send({ error_code: 'DatasetEmpty', message: '数据集为空' });
      }
      try {
        const summary = await state.datasetStore.saveAndParse(
          req.params.sid,
          parsed.data.filename,
          bytes,
        );
        await state.sessionStore.appendDataset(req.params.sid, summary);
        return reply.code(201).send(summary);
      } catch (err) {
        if (err instanceof StoreError) {
          const { status, body } = storeErrorResponse(err);
          return reply.code(status).send(body);
        }
        // Parse failure (Requirement 6.7): reject without appending a summary.
        return reply
          .code(422)
          .send({ error_code: 'SkillInvalidArgs', message: (err as Error).message });
      }
    },
  );

  // GET /api/sessions/:sid/datasets/:did
  app.get<{ Params: { sid: string; did: string } }>(
    '/api/sessions/:sid/datasets/:did',
    async (req, reply) => {
      try {
        const session = await state.sessionStore.get(req.params.sid);
        const dataset = session.datasets.find((d) => d.dataset_id === req.params.did);
        if (!dataset) {
          return reply.code(404).send({ error_code: 'SessionNotFound', message: '数据集不存在' });
        }
        return reply.send(dataset);
      } catch (err) {
        if (err instanceof StoreError) {
          const { status, body } = storeErrorResponse(err);
          return reply.code(status).send(body);
        }
        throw err;
      }
    },
  );

  // POST /api/sessions/:sid/datasets/:did/audit — full server-side preflight.
  app.post<{ Params: { sid: string; did: string } }>(
    '/api/sessions/:sid/datasets/:did/audit',
    async (req, reply) => {
      const parsed = domain.datasetAuditRequest.safeParse(req.body);
      if (!parsed.success) {
        return reply.code(422).send({ error_code: 'SkillInvalidArgs', message: 'invalid dataset audit request' });
      }
      if (!state.researchWorkflow) {
        return reply.code(500).send({ error_code: 'SkillExecutionFailed', message: '研究工作流服务尚未初始化' });
      }
      try {
        const audit = await state.researchWorkflow.auditDataset({
          sessionId: req.params.sid,
          datasetId: req.params.did,
          skillId: parsed.data.skill_id,
          args: parsed.data.args,
          expectedProtocolVersion: parsed.data.expected_protocol_version,
          auditRoles: parsed.data.audit_roles,
        });
        return reply.send(audit);
      } catch (err) {
        if (err instanceof ResearchWorkflowError) {
          const mapped = workflowErrorResponse(err);
          return reply.code(mapped.status).send(mapped.body);
        }
        if (err instanceof StoreError) {
          const mapped = storeErrorResponse(err);
          return reply.code(mapped.status).send(mapped.body);
        }
        throw err;
      }
    },
  );

  // POST /api/sessions/:sid/analysis-plans/approve — server timestamp + bound hashes.
  app.post<{ Params: { sid: string } }>(
    '/api/sessions/:sid/analysis-plans/approve',
    async (req, reply) => {
      const parsed = domain.analysisPlanApprovalRequest.safeParse(req.body);
      if (!parsed.success) {
        return reply.code(422).send({ error_code: 'SkillInvalidArgs', message: 'invalid analysis plan approval request' });
      }
      if (!state.researchWorkflow) {
        return reply.code(500).send({ error_code: 'SkillExecutionFailed', message: '研究工作流服务尚未初始化' });
      }
      try {
        const approval = await state.researchWorkflow.approveAnalysisPlan({
          sessionId: req.params.sid,
          datasetId: parsed.data.dataset_id,
          skillId: parsed.data.skill_id,
          args: parsed.data.args,
          expectedProtocolVersion: parsed.data.expected_protocol_version,
          expectedAuditId: parsed.data.expected_audit_id,
          expectedAuditSha256: parsed.data.expected_audit_sha256,
          auditRoles: parsed.data.audit_roles,
        });
        return reply.code(201).send(approval);
      } catch (err) {
        if (err instanceof ResearchWorkflowError) {
          const mapped = workflowErrorResponse(err);
          return reply.code(mapped.status).send(mapped.body);
        }
        if (err instanceof StoreError) {
          const mapped = storeErrorResponse(err);
          return reply.code(mapped.status).send(mapped.body);
        }
        throw err;
      }
    },
  );

  // POST /api/sessions/:sid/run — in-process skill execution (Requirement 12).
  app.post<{ Params: { sid: string } }>('/api/sessions/:sid/run', async (req, reply) => {
    // All research-gate routes resolve schema errors before session state so
    // malformed requests have one stable 422 contract across entry points.
    const parsed = domain.runRequest.safeParse(req.body);
    if (!parsed.success) {
      return reply.code(422).send({ error_code: 'SkillInvalidArgs', message: 'invalid run request body' });
    }

    let session;
    try {
      session = await state.sessionStore.get(req.params.sid);
    } catch (err) {
      if (err instanceof StoreError) {
        const { status, body } = storeErrorResponse(err);
        return reply.code(status).send(body);
      }
      throw err;
    }
    if (session.status === 'Archived') {
      return reply.code(409).send({ error_code: 'SessionArchived', message: '会话已归档，仅支持只读访问' });
    }

    // Every formal analysis enters through the same session-aware gate.
    if (!state.researchWorkflow) {
      return reply
        .code(500)
        .send({ error_code: 'SkillExecutionFailed', message: '研究工作流服务尚未初始化' });
    }

    try {
      const result = await state.researchWorkflow.execute({
        sessionId: req.params.sid,
        datasetId: parsed.data.dataset_id,
        skillId: parsed.data.skill_id,
        args: parsed.data.args,
        planId: parsed.data.plan_id,
      });
      const runResult = result as {
        analysis?: {
          run_id?: unknown;
        };
      };
      const runId = typeof runResult.analysis?.run_id === 'string' ? runResult.analysis.run_id : randomUUID();
      await state.sessionStore.appendMessages(req.params.sid, [
        agentMessage([{ SkillResult: { run_id: runId, result } } as AgentBlock]),
      ]);
      return reply.send(result);
    } catch (err) {
      if (err instanceof ResearchWorkflowError) {
        const mapped = workflowErrorResponse(err);
        return reply.code(mapped.status).send(mapped.body);
      }
      if (err instanceof StoreError) {
        const mapped = storeErrorResponse(err);
        return reply.code(mapped.status).send(mapped.body);
      }
      return reply.code(runErrorStatus(err)).send(runErrorBody(err));
    }
  });

  // GET /api/llm-status — never exposes the api key.
  app.get('/api/llm-status', async () => {
    const config = state.llmConfigStore?.read() ?? null;
    return statusFromConfig(config);
  });

  // POST /api/llm-config — reject OAuth-required-but-unavailable, then test, then save.
  app.post('/api/llm-config', async (req, reply) => {
    const parsed = postLlmConfigRequest.safeParse(req.body);
    if (!parsed.success) {
      return reply.code(422).send({ error_code: 'SkillInvalidArgs', message: 'invalid llm config' });
    }
    const store = state.llmConfigStore;
    const probe = state.llmProbe;
    if (!store || !probe) {
      return reply.code(500).send({ error_code: 'InternalError', message: 'LLM config store/probe not configured' });
    }
    // Reject an OAuth-required provider up-front when the flow is unavailable
    // (Requirement 13.5). The current provider set is API-key based, so this
    // guard is dormant until an OAuth-only provider is added.
    if (providerRequiresOAuth(parsed.data.provider) && !state.oauthCapability?.available) {
      return reply
        .code(422)
        .send({ error_code: 'OAUTH_UNAVAILABLE', message: `provider '${parsed.data.provider}' requires OAuth` });
    }
    try {
      await testAndSaveConfig(
        probe,
        store,
        {
          provider: parsed.data.provider,
          apiKey: parsed.data.api_key,
          baseUrl: parsed.data.base_url ?? undefined,
          model: parsed.data.model ?? undefined,
        },
        state.oauthCapability ?? { available: false },
      );
    } catch (err) {
      if (err instanceof LlmConfigError) {
        return reply.code(422).send({ error_code: err.code, message: err.message });
      }
      throw err;
    }
    return reply.code(200).send();
  });

  // GET /api/coverage-matrix — 503 if no provider.
  app.get('/api/coverage-matrix', async (_req, reply) => {
    if (!state.coverageMatrixProvider) {
      return reply.code(503).send({ error_code: 'CoverageMatrixUnavailable', message: 'coverage matrix provider not configured' });
    }
    return reply.send(state.coverageMatrixProvider.get());
  });

  // POST /api/sidecar/:algorithm_id
  app.post<{ Params: { algorithm_id: string } }>('/api/sidecar/:algorithm_id', async (req, reply) => {
    if (!state.sidecarProvider) {
      return reply.code(503).send({ error_code: 'SidecarUnavailable', message: 'sidecar provider not configured' });
    }
    const parsed = sidecarContract.sidecarRenderRequest.safeParse(req.body);
    if (!parsed.success) {
      return reply.code(400).send({ error_code: 'InvalidRequest', message: 'invalid sidecar request' });
    }
    try {
      const dto = state.sidecarProvider.generate(req.params.algorithm_id, parsed.data);
      return reply.send(dto);
    } catch (err) {
      if (err instanceof engineSidecar.GenerateError && err.kind === 'unsafe_identifier') {
        return reply.code(400).send({
          error_code: 'InvalidRequest',
          message: 'sidecar request contains a non-portable identifier',
        });
      }
      // Unknown algorithm id is a client addressing error, not a server fault (D4).
      if (err instanceof engineSidecar.GenerateError && err.kind === 'unknown_algorithm') {
        return reply.code(404).send({
          error_code: 'NotFound',
          message: `unknown sidecar algorithm: ${req.params.algorithm_id}`,
        });
      }
      // Template placeholders referencing columns/params the request did not
      // supply (e.g. the contract-default empty `columns`) are a client error,
      // not a 500 (D3). The renderer itself stays strict.
      if (err instanceof engineSidecar.RenderError) {
        return reply.code(400).send({
          error_code: 'InvalidRequest',
          message: `sidecar request is missing required columns/params: ${err.message}`,
        });
      }
      return reply.code(500).send({ error_code: 'InternalError', message: (err as Error).message });
    }
  });

  // POST /api/snapshot/export — JSON materialize (default) or zip body (download:true).
  app.post('/api/snapshot/export', async (req, reply) => {
    if (!state.snapshotProvider) {
      return reply.code(503).send({ error_code: 'SnapshotUnavailable', message: 'snapshot provider not configured' });
    }
    const parsed = sidecarContract.snapshotExportRequest.safeParse(req.body);
    if (!parsed.success) {
      return reply.code(400).send({ error_code: 'InvalidRequest', message: 'invalid snapshot request' });
    }
    try {
      const resp = await state.snapshotProvider.export(parsed.data.run_id, parsed.data.destination);
      // Prefer GET /api/snapshot/files/:runId for SPA downloads (Content-Length + proxy-friendly).
      // POST download:true kept as fallback with explicit Content-Length.
      if (parsed.data.download) {
        const bytes = readFileSync(resp.snapshot_path);
        const filename = basename(resp.snapshot_path) || `snapshot-${parsed.data.run_id}.zip`;
        return reply
          .header('Content-Type', 'application/zip')
          .header('Content-Length', String(bytes.byteLength))
          .header('Content-Disposition', `attachment; filename="${filename}"`)
          .header('X-Snapshot-Path', resp.snapshot_path)
          .header('X-Snapshot-Sha256', resp.sha256)
          .send(bytes);
      }
      return reply.send(resp);
    } catch (err) {
      return replySnapshotError(reply, err);
    }
  });

  /**
   * GET /api/snapshot/files/:runId — SPA-safe download.
   * Materializes a zip under `<cwd>/exports/` then streams with Content-Length
   * so Vite proxy / Chrome do not truncate the body (avoids ERR_CONNECTION_CLOSED
   * on half-written downloads).
   */
  app.get<{ Params: { runId: string } }>('/api/snapshot/files/:runId', async (req, reply) => {
    if (!state.snapshotProvider) {
      return reply.code(503).send({ error_code: 'SnapshotUnavailable', message: 'snapshot provider not configured' });
    }
    const runId = req.params.runId?.trim();
    if (!runId || !/^[0-9a-fA-F-]{8,64}$/.test(runId)) {
      return reply.code(400).send({ error_code: 'InvalidRequest', message: '运行 ID 无效' });
    }
    try {
      const exportsDir = join(process.cwd(), 'exports');
      mkdirSync(exportsDir, { recursive: true });
      const destination = join(exportsDir, `snapshot-${runId}.zip`);
      const resp = await state.snapshotProvider.export(runId, destination);
      const bytes = readFileSync(resp.snapshot_path);
      const filename = basename(resp.snapshot_path) || `snapshot-${runId}.zip`;
      return reply
        .header('Content-Type', 'application/zip')
        .header('Content-Length', String(bytes.byteLength))
        .header('Content-Disposition', `attachment; filename="${filename}"`)
        .header('Cache-Control', 'no-store')
        .header('X-Snapshot-Path', resp.snapshot_path)
        .header('X-Snapshot-Sha256', resp.sha256)
        .send(bytes);
    } catch (err) {
      return replySnapshotError(reply, err);
    }
  });

  // SPA embedding + catch-all fallback (task 3.4). Opt-in so contract tests
  // can assert raw 404s; prod and the launcher enable it.
  if (opts.installSpaFallback) {
    const source = opts.spaAssetSource ?? createDefaultAssetSource();
    installSpaFallback(app, source);
  }

  return app;
}

export { domain };
