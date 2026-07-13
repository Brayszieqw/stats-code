/**
 * Typed API client for all /api/* endpoints.
 *
 * Each function corresponds to an agent-server HTTP route.
 * Relative paths are used so Vite's dev proxy handles routing to :8080.
 */

import type {
  Session,
  SessionSummary,
  SessionSettings,
  DatasetSummary,
  RunRequest,
  SkillResult,
  PostAudioResponse,
  HealthResponse,
  ErrorPayload,
  LlmProvider,
  LlmStatusResponse,
  ResearchProtocolInput,
  ProtocolCompileRequest,
  ProtocolCompileResult,
  DatasetAudit,
  DatasetAuditRequest,
  AnalysisPlanApproval,
  AnalysisPlanApprovalRequest,
} from './types';

// ---------------------------------------------------------------------------
// Browser file helpers
// ---------------------------------------------------------------------------

function arrayBufferToBase64(buffer: ArrayBuffer): string {
  const bytes = new Uint8Array(buffer);
  const chunkSize = 0x8000;
  let binary = '';

  for (let offset = 0; offset < bytes.length; offset += chunkSize) {
    binary += String.fromCharCode(...bytes.subarray(offset, offset + chunkSize));
  }

  return btoa(binary);
}

export async function fileToBase64(file: File): Promise<string> {
  if (typeof file.arrayBuffer === 'function') {
    return arrayBufferToBase64(await file.arrayBuffer());
  }

  const buffer = await new Promise<ArrayBuffer>((resolve, reject) => {
    const reader = new FileReader();
    reader.addEventListener('error', () => {
      reject(reader.error ?? new Error('failed to read file'));
    });
    reader.addEventListener('load', () => {
      if (reader.result instanceof ArrayBuffer) {
        resolve(reader.result);
        return;
      }
      reject(new Error('file reader returned a non-array-buffer result'));
    });
    reader.readAsArrayBuffer(file);
  });
  return arrayBufferToBase64(buffer);
}

// ---------------------------------------------------------------------------
// Error handling
// ---------------------------------------------------------------------------

/**
 * Custom error class wrapping structured API error responses.
 */
export class ApiError extends Error {
  public readonly payload: ErrorPayload;
  public readonly status: number;

  constructor(status: number, payload: ErrorPayload) {
    super(payload.message);
    this.name = 'ApiError';
    this.status = status;
    this.payload = payload;
  }
}

/**
 * Parse a fetch Response and throw ApiError on non-2xx status.
 */
async function handleResponse<T>(res: Response): Promise<T> {
  if (!res.ok) {
    let payload: ErrorPayload;
    try {
      payload = await res.json();
    } catch {
      payload = {
        error_code: 'SkillExecutionFailed',
        message: `HTTP ${res.status}: ${res.statusText}`,
      };
    }
    throw new ApiError(res.status, payload);
  }
  return res.json() as Promise<T>;
}

// ---------------------------------------------------------------------------
// Session endpoints
// ---------------------------------------------------------------------------

/** POST /api/sessions — 创建新会话 */
export async function createSession(): Promise<Session> {
  const res = await fetch('/api/sessions', { method: 'POST' });
  return handleResponse<Session>(res);
}

/** GET /api/sessions — 列出会话摘要（历史会话，倒序）。Requirement 11. */
export async function listSessions(): Promise<SessionSummary[]> {
  const res = await fetch('/api/sessions');
  return handleResponse<SessionSummary[]>(res);
}

/** GET /api/sessions/:sid — 获取会话状态与历史 */
export async function getSession(sid: string): Promise<Session> {
  const res = await fetch(`/api/sessions/${sid}`);
  return handleResponse<Session>(res);
}

export async function deleteSession(sid: string): Promise<void> {
  const res = await fetch(`/api/sessions/${sid}`, { method: 'DELETE' });
  if (!res.ok) {
    let payload: ErrorPayload;
    try {
      payload = await res.json();
    } catch {
      payload = {
        error_code: 'SkillExecutionFailed',
        message: `HTTP ${res.status}: ${res.statusText}`,
      };
    }
    throw new ApiError(res.status, payload);
  }
}

/** PATCH /api/sessions/:sid/settings — 更新会话设置 */
export async function patchSettings(
  sid: string,
  settings: Partial<SessionSettings>,
): Promise<Session> {
  const res = await fetch(`/api/sessions/${sid}/settings`, {
    method: 'PATCH',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(settings),
  });
  return handleResponse<Session>(res);
}

/** PATCH /api/sessions/:sid/protocol — 保存研究协议草稿或审批版本。 */
export async function patchResearchProtocol(
  sid: string,
  protocol: ResearchProtocolInput,
): Promise<Session> {
  const res = await fetch(`/api/sessions/${sid}/protocol`, {
    method: 'PATCH',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(protocol),
  });
  return handleResponse<Session>(res);
}

/** POST /api/sessions/:sid/protocol/compile — generate a review-only draft proposal. */
export async function compileResearchProtocol(
  sid: string,
  request: ProtocolCompileRequest,
): Promise<ProtocolCompileResult> {
  const res = await fetch(`/api/sessions/${sid}/protocol/compile`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(request),
  });
  return handleResponse<ProtocolCompileResult>(res);
}

// ---------------------------------------------------------------------------
// Message endpoint (SSE)
// ---------------------------------------------------------------------------

/**
 * POST /api/sessions/:sid/messages — 发送用户消息并接收 SSE 事件流。
 *
 * 浏览器原生 `EventSource` 仅支持 GET，所以这里使用 fetch + ReadableStream
 * 读取 POST 响应的 SSE 流。返回原始 `Response` 由 `useSseChat` hook 解析。
 * `signal` 用于打断式追问：abort 时取消底层请求与流（R8.3）。
 */
export async function postMessageFetch(
  sid: string,
  text: string,
  signal?: AbortSignal,
): Promise<Response> {
  const res = await fetch(`/api/sessions/${sid}/messages`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ text }),
    signal,
  });
  if (!res.ok) {
    let payload: ErrorPayload;
    try {
      payload = await res.json();
    } catch {
      payload = {
        error_code: 'SkillExecutionFailed',
        message: `HTTP ${res.status}: ${res.statusText}`,
      };
    }
    throw new ApiError(res.status, payload);
  }
  return res;
}

// ---------------------------------------------------------------------------
// Audio endpoint
// ---------------------------------------------------------------------------

/**
 * POST /api/sessions/:sid/audio — 上传音频文件。
 *
 * Wire format (matches `crates/agent-server/src/handlers/audio.rs`):
 *   - Content-Type: application/octet-stream
 *   - Body: raw audio bytes
 *   - Header X-Audio-Duration-Secs: integer seconds
 */
export async function postAudio(
  sid: string,
  audio: Blob,
  durationSecs: number,
): Promise<PostAudioResponse> {
  const res = await fetch(`/api/sessions/${sid}/audio`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/octet-stream',
      'X-Audio-Duration-Secs': String(Math.max(0, Math.round(durationSecs))),
    },
    body: audio,
  });
  return handleResponse<PostAudioResponse>(res);
}

// ---------------------------------------------------------------------------
// Dataset endpoints
// ---------------------------------------------------------------------------

/** POST /api/sessions/:sid/datasets — 上传数据文件 */
export async function postDataset(
  sid: string,
  file: File,
): Promise<DatasetSummary> {
  const data = await fileToBase64(file);

  const res = await fetch(`/api/sessions/${sid}/datasets`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ filename: file.name, data }),
  });
  return handleResponse<DatasetSummary>(res);
}

/** GET /api/sessions/:sid/datasets/:did — 获取数据集摘要 */
export async function getDataset(
  sid: string,
  did: string,
): Promise<DatasetSummary> {
  const res = await fetch(`/api/sessions/${sid}/datasets/${did}`);
  return handleResponse<DatasetSummary>(res);
}

/** POST dataset audit — server recomputes integrity and design blockers. */
export async function auditDataset(
  sid: string,
  did: string,
  body: DatasetAuditRequest,
): Promise<DatasetAudit> {
  const res = await fetch(`/api/sessions/${sid}/datasets/${did}/audit`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
  return handleResponse<DatasetAudit>(res);
}

/** POST analysis-plan approval — returns the only plan id accepted by /run. */
export async function approveAnalysisPlan(
  sid: string,
  body: AnalysisPlanApprovalRequest,
): Promise<AnalysisPlanApproval> {
  const res = await fetch(`/api/sessions/${sid}/analysis-plans/approve`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
  return handleResponse<AnalysisPlanApproval>(res);
}

/**
 * POST /api/sessions/:sid/run — 运行指定 skill/algorithm（进程内执行）。
 * Requirement 12. 成功返回 SkillResult；失败抛出携带错误码的 ApiError。
 * `signal` 供 RunControls 的「停止」真正取消底层请求（R12.7）。
 */
export async function runSkill(sid: string, body: RunRequest, signal?: AbortSignal): Promise<SkillResult> {
  const res = await fetch(`/api/sessions/${sid}/run`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
    signal,
  });
  return handleResponse<SkillResult>(res);
}

// ---------------------------------------------------------------------------
// Health
// ---------------------------------------------------------------------------

/** GET /api/health — 健康检查 */
export async function healthCheck(): Promise<HealthResponse> {
  const res = await fetch('/api/health');
  return handleResponse<HealthResponse>(res);
}

// ---------------------------------------------------------------------------
// LLM config
// ---------------------------------------------------------------------------

/**
 * GET /api/llm-status — 查询 LLM 配置状态。
 *
 * 用于 Onboarding_Card 的遮罩判定（Requirement 10、11.1）。
 * 响应不含 `api_key`（Requirement 10.4）。
 */
export async function getLlmStatus(): Promise<LlmStatusResponse> {
  const res = await fetch('/api/llm-status');
  return handleResponse<LlmStatusResponse>(res);
}

/**
 * POST /api/llm-config — 测试并保存 LLM 配置。
 *
 * 后端先用提供的 provider + api_key 发起一次连通性 probe：
 *   - probe 成功 → 落盘并返回 200
 *   - probe 失败 → 不落盘并返回 422 + `LLM_PROBE_FAILED`
 *
 * Validates: Requirements 11.3, 11.4, 11.5
 */
export async function postLlmConfig(
  provider: LlmProvider,
  apiKey: string,
  baseUrl?: string,
  model?: string,
): Promise<void> {
  const res = await fetch('/api/llm-config', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ provider, api_key: apiKey, base_url: baseUrl, model }),
  });
  if (!res.ok) {
    let payload: ErrorPayload;
    try {
      payload = await res.json();
    } catch {
      payload = {
        error_code: 'SkillExecutionFailed',
        message: `HTTP ${res.status}: ${res.statusText}`,
      };
    }
    throw new ApiError(res.status, payload);
  }
}
