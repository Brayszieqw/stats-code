// contract/routes.ts — per-route request/response schema registry.
//
// Each of the 13 API_Contract routes (plus the SPA fallback, handled
// separately) is described here with its zod request/response schemas and the
// success status code, transcribed from the Rust axum handlers. This registry
// is the single source consumed by:
//   - the Fastify server (task 3.2) for request/response validation,
//   - the contract/golden tests (task 3.8) for diffing against Rust fixtures.

import { z } from 'zod';
import {
  session,
  sessionSummary,
  sessionSettings,
  datasetSummary,
  skillResult,
  runRequest,
  errorPayload,
  llmProvider,
} from './domain.js';
import {
  coverageMatrix,
  sidecarSnippet,
  sidecarRenderRequest,
  snapshotExportRequest,
  snapshotExportResponse,
} from './sidecar.js';

export type HttpMethod = 'GET' | 'POST' | 'PATCH';

export interface RouteContract {
  /** Stable identifier for fixtures/tests. */
  id: string;
  method: HttpMethod;
  /** Fastify-style path with :params. */
  path: string;
  /** Request body schema (undefined for GET / no-body routes). */
  request?: z.ZodTypeAny;
  /** Success response body schema (undefined for empty bodies, e.g. 200 OK no content). */
  response?: z.ZodTypeAny;
  /** Success status code returned by the Rust handler. */
  successStatus: number;
  /** Per-route body limit in bytes, if overridden. */
  bodyLimitBytes?: number;
}

// --- route-specific request/response schemas ------------------------------

export const healthResponse = z.object({ status: z.literal('ok') });

export const patchSettingsRequest = z.object({
  decision_assistant: z.boolean(),
});

// Dataset upload (JSON base64 variant). multipart is also accepted by the
// handler but the contract schema covers the JSON shape.
export const base64DatasetRequest = z.object({
  filename: z.string(),
  data: z.string(),
});

export const postAudioResponse = z.object({
  text: z.string(),
  confidence: z.number(),
  auto_processed: z.boolean(),
});

export const llmStatusResponse = z.object({
  configured: z.boolean(),
  provider: llmProvider.nullable(),
  base_url: z.string().nullable(),
  model: z.string().nullable(),
});

export const postLlmConfigRequest = z.object({
  provider: llmProvider,
  api_key: z.string(),
  base_url: z.string().nullable().optional(),
  model: z.string().nullable().optional(),
});

// POST /api/sessions/:sid/messages — request body (SSE response handled separately).
export const postMessageRequest = z.object({
  text: z.string().optional(),
  content: z
    .object({
      type: z.string(),
      text: z.string(),
    })
    .optional(),
});

// Body limits (from crates/agent-server/src/lib.rs).
const AUDIO_BODY_LIMIT = 10 * 1024 * 1024;
const DATASET_BODY_LIMIT = 70 * 1024 * 1024;

/** The 13 API_Contract routes. The SPA fallback is registered separately (task 3.4). */
export const ROUTE_CONTRACTS: readonly RouteContract[] = [
  {
    id: 'health',
    method: 'GET',
    path: '/api/health',
    response: healthResponse,
    successStatus: 200,
  },
  {
    id: 'create_session',
    method: 'POST',
    path: '/api/sessions',
    response: session,
    successStatus: 201,
  },
  {
    id: 'get_session',
    method: 'GET',
    path: '/api/sessions/:sid',
    response: session,
    successStatus: 200,
  },
  {
    id: 'patch_settings',
    method: 'PATCH',
    path: '/api/sessions/:sid/settings',
    request: patchSettingsRequest,
    response: session,
    successStatus: 200,
  },
  {
    id: 'post_message',
    method: 'POST',
    path: '/api/sessions/:sid/messages',
    request: postMessageRequest,
    // Response is an SSE stream — no JSON response schema (task 3.3).
    successStatus: 200,
  },
  {
    id: 'post_audio',
    method: 'POST',
    path: '/api/sessions/:sid/audio',
    response: postAudioResponse,
    successStatus: 200,
    bodyLimitBytes: AUDIO_BODY_LIMIT,
  },
  {
    id: 'post_dataset',
    method: 'POST',
    path: '/api/sessions/:sid/datasets',
    request: base64DatasetRequest,
    response: datasetSummary,
    successStatus: 201,
    bodyLimitBytes: DATASET_BODY_LIMIT,
  },
  {
    id: 'get_dataset',
    method: 'GET',
    path: '/api/sessions/:sid/datasets/:did',
    response: datasetSummary,
    successStatus: 200,
  },
  {
    id: 'get_llm_status',
    method: 'GET',
    path: '/api/llm-status',
    response: llmStatusResponse,
    successStatus: 200,
  },
  {
    id: 'post_llm_config',
    method: 'POST',
    path: '/api/llm-config',
    request: postLlmConfigRequest,
    // Success returns 200 with an empty body.
    successStatus: 200,
  },
  {
    id: 'get_coverage_matrix',
    method: 'GET',
    path: '/api/coverage-matrix',
    response: coverageMatrix,
    successStatus: 200,
  },
  {
    id: 'post_sidecar',
    method: 'POST',
    path: '/api/sidecar/:algorithm_id',
    request: sidecarRenderRequest,
    response: sidecarSnippet,
    successStatus: 200,
  },
  {
    id: 'post_snapshot_export',
    method: 'POST',
    path: '/api/snapshot/export',
    request: snapshotExportRequest,
    response: snapshotExportResponse,
    successStatus: 200,
  },
  // --- additions for the dual-mode frontend (Requirements 11, 12) ----------
  {
    id: 'list_sessions',
    method: 'GET',
    path: '/api/sessions',
    response: z.array(sessionSummary),
    successStatus: 200,
  },
  {
    id: 'run_skill',
    method: 'POST',
    path: '/api/sessions/:sid/run',
    request: runRequest,
    response: skillResult,
    successStatus: 200,
  },
];

export { errorPayload, sessionSettings };
