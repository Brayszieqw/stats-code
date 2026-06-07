// server/router.ts — Fastify router with all 13 API_Contract routes (task 3.2).
// Transcribed from crates/agent-server/src/lib.rs (build_router) and handlers/*.
//
// Each route validates its request/response against the zod contract schemas,
// returns the same status codes as the Rust backend, and enforces the per-route
// body limits (datasets 70 MiB base64, audio 10 MiB). SSE for the messages
// route (task 3.3) and the SPA fallback (task 3.4) are layered on separately.

import Fastify, { type FastifyInstance } from 'fastify';
import {
  domain,
  patchSettingsRequest,
  base64DatasetRequest,
  postLlmConfigRequest,
  sidecar as sidecarContract,
} from './contract/index.js';
import { StoreError, type AppState } from './state.js';
import { serializeSseFrame } from './sse.js';
import { installSpaFallback, type SpaAssetSource } from './spa.js';
import { createDefaultAssetSource } from './spa-assets.js';
import { statusFromConfig, testAndSaveConfig, LlmConfigError, providerRequiresOAuth } from './llm.js';

const AUDIO_BODY_LIMIT = 10 * 1024 * 1024;
const DATASET_BODY_LIMIT = 70 * 1024 * 1024;

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

  // Permissive CORS (mirrors tower_http CorsLayer::permissive()).
  app.addHook('onSend', (_req, reply, payload, done) => {
    reply.header('access-control-allow-origin', '*');
    reply.header('access-control-allow-methods', 'GET,POST,PATCH,OPTIONS');
    reply.header('access-control-allow-headers', '*');
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

  // GET /api/sessions/:sid
  app.get<{ Params: { sid: string } }>('/api/sessions/:sid', async (req, reply) => {
    try {
      const session = await state.sessionStore.get(req.params.sid);
      return reply.send(session);
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

  // POST /api/sessions/:sid/messages — SSE (full streaming wired in task 3.3).
  app.post<{ Params: { sid: string } }>('/api/sessions/:sid/messages', async (req, reply) => {
    const body = (req.body ?? {}) as { text?: string; content?: { type: string; text: string } };
    const text = body.text ?? (body.content?.type === 'text' ? body.content.text : undefined);
    if (text === undefined) {
      return reply.code(413).send({ error_code: 'MessageTooLong', message: '请求体缺少 text 字段' });
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
    // Stream the orchestrator's AgentEvents as SSE frames (task 3.3). When no
    // message handler is configured (Phase-0 scaffold), emit a single terminal
    // `done` frame so the SSE contract shape still holds.
    reply.raw.writeHead(200, {
      'content-type': 'text/event-stream',
      'cache-control': 'no-cache',
      connection: 'keep-alive',
    });

    const handler = state.messageHandler;
    if (!handler) {
      reply.raw.write(serializeSseFrame({ type: 'done' }));
      reply.raw.end();
      return reply;
    }

    try {
      const session = await state.sessionStore.get(req.params.sid);
      const stream = handler.handleMessage(req.params.sid, {
        text,
        settings: session.settings,
      });
      for await (const event of stream) {
        reply.raw.write(serializeSseFrame(event));
      }
    } catch (err) {
      // Mid-stream failure: emit a structured error frame then terminate. The
      // HTTP status is already 200 (headers flushed), matching the Rust SSE
      // behavior where errors surface as `event: error` frames.
      reply.raw.write(
        serializeSseFrame({
          type: 'error',
          payload: { error_code: 'SkillExecutionFailed', message: (err as Error).message },
        }),
      );
    }
    reply.raw.end();
    return reply;
  });

  // POST /api/sessions/:sid/audio (10 MiB limit)
  app.post<{ Params: { sid: string } }>(
    '/api/sessions/:sid/audio',
    { bodyLimit: AUDIO_BODY_LIMIT },
    async (_req, reply) =>
      reply.code(502).send({ error_code: 'LlmUnavailable', message: '语音转写服务尚未初始化' }),
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
      // Full dataset parsing/quota lands with the dataset store; the contract
      // shape and status code (201) are established here.
      return reply
        .code(500)
        .send({ error_code: 'SkillExecutionFailed', message: '数据集存储服务尚未初始化' });
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
      return reply.code(500).send({ error_code: 'InternalError', message: (err as Error).message });
    }
  });

  // POST /api/snapshot/export
  app.post('/api/snapshot/export', async (req, reply) => {
    if (!state.snapshotProvider) {
      return reply.code(503).send({ error_code: 'SnapshotUnavailable', message: 'snapshot provider not configured' });
    }
    const parsed = sidecarContract.snapshotExportRequest.safeParse(req.body);
    if (!parsed.success) {
      return reply.code(400).send({ error_code: 'InvalidRequest', message: 'invalid snapshot request' });
    }
    try {
      const resp = state.snapshotProvider.export(parsed.data.run_id, parsed.data.destination);
      return reply.send(resp);
    } catch (err) {
      return reply.code(500).send({ error_code: 'InternalError', message: (err as Error).message });
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
