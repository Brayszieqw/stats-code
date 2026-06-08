// server/state.ts — application state: injectable stores + providers.
// Mirrors crates/agent-server/src/state.rs (AppState). Providers are optional;
// routes that need an absent provider return 503, matching the Rust handlers.

import type { domain, sidecar } from './contract/index.js';
import type { z } from 'zod';

export type Session = z.infer<typeof domain.session>;
export type DatasetSummary = z.infer<typeof domain.datasetSummary>;
export type ColumnSummary = z.infer<typeof domain.columnSummary>;
export type SessionSettings = z.infer<typeof domain.sessionSettings>;

export class StoreError extends Error {
  constructor(
    public readonly kind: 'not_found' | 'archived' | 'internal',
    message: string,
  ) {
    super(message);
    this.name = 'StoreError';
  }
}

/** Session persistence abstraction (in-memory default provided). */
export interface SessionStore {
  create(): Promise<Session>;
  get(id: string): Promise<Session>;
  updateSettings(id: string, settings: SessionSettings): Promise<void>;
  appendDataset(id: string, dataset: DatasetSummary): Promise<void>;
}

export interface LlmConfig {
  provider: 'deepseek' | 'openai';
  api_key: string;
  base_url?: string | null;
  model?: string | null;
}

export interface LlmConfigStore {
  read(): LlmConfig | null;
  write(config: LlmConfig): void;
}

export interface LlmProbe {
  probe(provider: 'deepseek' | 'openai', apiKey: string, baseUrl?: string, model?: string): Promise<void>;
}

export interface CoverageMatrixProvider {
  get(): z.infer<typeof sidecar.coverageMatrix>;
}

export interface SidecarProvider {
  generate(
    algorithmId: string,
    request: z.infer<typeof sidecar.sidecarRenderRequest>,
  ): z.infer<typeof sidecar.sidecarSnippet>;
}

export interface SnapshotProvider {
  export(runId: string, destination: string): z.infer<typeof sidecar.snapshotExportResponse>;
}

/** Dataset persistence + parsing abstraction (conversation layer). */
export interface DatasetStore {
  saveAndParse(sid: string, fileName: string, bytes: Uint8Array): Promise<DatasetSummary>;
  readRawById(datasetId: string): Promise<Uint8Array>;
}

// ---------------------------------------------------------------------------
// Orchestrator message handler + AgentEvent stream (task 3.3).
// Mirrors agent_core::orchestrator::AgentEvent and the Rust SSE emitter in
// crates/agent-server/src/handlers/message.rs. Each variant maps to a distinct
// SSE `event:` name with a JSON `data:` payload.
// ---------------------------------------------------------------------------

export type AgentEvent =
  | { type: 'text_delta'; text: string }
  | { type: 'choice_prompt'; prompt: unknown }
  | { type: 'skill_call'; skill_id: string; args: unknown }
  | { type: 'skill_result'; result: unknown }
  | { type: 'interpretation'; text: string }
  | { type: 'error'; payload: unknown }
  | { type: 'done' };

/** Input handed to the orchestrator for one user message. */
export interface UserMessageInput {
  text: string;
  settings: SessionSettings;
}

/**
 * Orchestrator abstraction: consume a user message and produce an async stream
 * of AgentEvents to relay over SSE. Optional — when absent, the messages route
 * emits a single terminal `done` frame (Phase-0 scaffold behavior).
 */
export interface MessageHandler {
  handleMessage(sessionId: string, input: UserMessageInput): AsyncIterable<AgentEvent>;
}

export interface AppState {
  sessionStore: SessionStore;
  messageHandler?: MessageHandler;
  datasetStore?: DatasetStore;
  llmConfigStore?: LlmConfigStore;
  llmProbe?: LlmProbe;
  /** Whether the backend can drive an OAuth flow (Requirement 13.4/13.5). */
  oauthCapability?: { available: boolean };
  coverageMatrixProvider?: CoverageMatrixProvider;
  sidecarProvider?: SidecarProvider;
  snapshotProvider?: SnapshotProvider;
}
