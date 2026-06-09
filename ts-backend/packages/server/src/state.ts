// server/state.ts — application state: injectable stores + providers.
// Mirrors crates/agent-server/src/state.rs (AppState). Providers are optional;
// routes that need an absent provider return 503, matching the Rust handlers.

import type { domain, sidecar } from './contract/index.js';
import type { z } from 'zod';

export type Session = z.infer<typeof domain.session>;
export type DatasetSummary = z.infer<typeof domain.datasetSummary>;
export type ColumnSummary = z.infer<typeof domain.columnSummary>;
export type SessionSettings = z.infer<typeof domain.sessionSettings>;

/**
 * Lightweight session summary for the history list (Requirement 11). Shaped to
 * match the `sessionSummary` zod contract (contract/domain.ts). Never includes
 * sensitive fields (no api_key — the session entity does not carry one).
 */
export interface SessionSummary {
  id: string;
  status: z.infer<typeof domain.sessionStatus>;
  created_at: string;
  last_active_at: string;
  message_count: number;
  title: string;
  dataset_count: number;
}

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
  /** List session summaries, sorted by last_active_at descending (Requirement 11.2). */
  list(): Promise<SessionSummary[]>;
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
// Code-run endpoint dependencies (Requirement 12). Structural interfaces so the
// server router can stay decoupled from the concrete conversation-layer
// SkillRunner/SkillRegistry implementations (avoids a circular import).
// ---------------------------------------------------------------------------

/** Context handed to a skill run: the raw dataset bytes + its summary. */
export interface RunSkillContext {
  datasetBytes: Uint8Array;
  datasetSummary: DatasetSummary;
}

/** Minimal descriptor surface the runner consumes. */
export interface RunSkillDescriptor {
  skillId: string;
  inputSchema: Record<string, unknown>;
}

/** Structural view of the SkillRegistry (lookup by skill id). */
export interface SkillRegistryLike {
  get(skillId: string): RunSkillDescriptor | undefined;
}

/** Structural view of the in-process SkillRunner (Requirement 12.2: never spawns). */
export interface SkillRunnerLike {
  run(
    descriptor: RunSkillDescriptor,
    args: Record<string, unknown>,
    ctx: RunSkillContext,
  ): Promise<unknown>;
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
  /** In-process skill runner for the code-run endpoint (Requirement 12). */
  skillRunner?: SkillRunnerLike;
  /** Skill registry for resolving a run target descriptor (Requirement 12). */
  skillRegistry?: SkillRegistryLike;
  llmConfigStore?: LlmConfigStore;
  llmProbe?: LlmProbe;
  /** Whether the backend can drive an OAuth flow (Requirement 13.4/13.5). */
  oauthCapability?: { available: boolean };
  coverageMatrixProvider?: CoverageMatrixProvider;
  sidecarProvider?: SidecarProvider;
  snapshotProvider?: SnapshotProvider;
}
