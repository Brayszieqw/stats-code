// server/state.ts — application state: injectable stores + providers.
// Mirrors crates/agent-server/src/state.rs (AppState). Providers are optional;
// routes that need an absent provider return 503, matching the Rust handlers.

import type { domain, sidecar } from './contract/index.js';
import type { z } from 'zod';

export type Session = z.infer<typeof domain.session>;
export type Message = z.infer<typeof domain.message>;
export type AgentBlock = z.infer<typeof domain.agentBlock>;
export type DatasetSummary = z.infer<typeof domain.datasetSummary>;
export type ColumnSummary = z.infer<typeof domain.columnSummary>;
export type SessionSettings = z.infer<typeof domain.sessionSettings>;
export type ResearchProtocol = z.infer<typeof domain.researchProtocol>;
export type ProtocolCompileRequest = z.infer<typeof domain.protocolCompileRequest>;
export type ProtocolCompileResult = z.infer<typeof domain.protocolCompileResult>;
export type DatasetAudit = z.infer<typeof domain.datasetAudit>;
export type DatasetAuditFinding = z.infer<typeof domain.datasetAuditFinding>;
export type DatasetAuditRoles = z.infer<typeof domain.datasetAuditRoles>;
export type AnalysisPlanApproval = z.infer<typeof domain.analysisPlanApproval>;
export type SkillRun = z.infer<typeof domain.skillRun>;

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
  /** Atomic compare-and-swap; false means the expected version is stale. */
  updateResearchProtocol(id: string, protocol: ResearchProtocol, expectedVersion?: number): Promise<boolean>;
  /** Append only while the session is active. */
  appendDatasetAudit(id: string, audit: DatasetAudit): Promise<void>;
  /** Atomically append only while the session is active and the bound protocol is still current. */
  appendAnalysisPlanApproval(id: string, approval: AnalysisPlanApproval): Promise<boolean>;
  /** Append only while the session is active. */
  appendSkillRun(id: string, run: SkillRun): Promise<void>;
  appendMessages(id: string, messages: Message[]): Promise<void>;
  appendDataset(id: string, dataset: DatasetSummary): Promise<void>;
  deleteSession(id: string): Promise<void>;
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

/** Review-only natural-language → protocol proposal service. */
export interface ProtocolCompiler {
  compile(input: ProtocolCompileRequest, context?: { sessionId: string }): Promise<ProtocolCompileResult>;
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
  export(
    runId: string,
    destination: string,
  ): z.infer<typeof sidecar.snapshotExportResponse> | Promise<z.infer<typeof sidecar.snapshotExportResponse>>;
}

export interface SnapshotRunRegistration {
  runId: string;
  sessionId: string;
  algorithmId: string;
  params: Record<string, unknown>;
  result: unknown;
  datasetSummary: DatasetSummary;
  researchProtocol: ResearchProtocol | null;
  analysisPlanApproval: AnalysisPlanApproval;
  datasetAudit: DatasetAudit;
  startedAtUtc: string;
  endedAtUtc: string;
}

/** Records completed deterministic runs so the snapshot route can materialize them later. */
export interface SnapshotRunRecorder {
  register(run: SnapshotRunRegistration): void;
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

export interface ResearchWorkflowAuditInput {
  sessionId: string;
  datasetId: string;
  skillId: string;
  args: Record<string, unknown>;
  expectedProtocolVersion: number;
  auditRoles?: DatasetAuditRoles;
}

export interface ResearchWorkflowApproveInput extends ResearchWorkflowAuditInput {
  expectedAuditId: string;
  expectedAuditSha256: string;
}

export interface ResearchWorkflowExecuteInput {
  sessionId: string;
  datasetId: string;
  skillId: string;
  args: Record<string, unknown>;
  /** Required for HTTP; conversation may opt into an exact previously approved match. */
  planId?: string;
  allowMatchingPlan?: boolean;
}

/** Single session-aware gate used by every server execution entry point. */
export interface ResearchWorkflowService {
  now(): Date;
  auditDataset(input: ResearchWorkflowAuditInput): Promise<DatasetAudit>;
  approveAnalysisPlan(input: ResearchWorkflowApproveInput): Promise<AnalysisPlanApproval>;
  execute(input: ResearchWorkflowExecuteInput): Promise<unknown>;
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
  /** Mandatory gate for formal analysis execution. */
  researchWorkflow?: ResearchWorkflowService;
  llmConfigStore?: LlmConfigStore;
  llmProbe?: LlmProbe;
  protocolCompiler?: ProtocolCompiler;
  /** Whether the backend can drive an OAuth flow (Requirement 13.4/13.5). */
  oauthCapability?: { available: boolean };
  coverageMatrixProvider?: CoverageMatrixProvider;
  sidecarProvider?: SidecarProvider;
  snapshotProvider?: SnapshotProvider;
  snapshotRunRecorder?: SnapshotRunRecorder;
}
