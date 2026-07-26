/**
 * TypeScript interfaces matching the agent-core backend data models.
 *
 * These types correspond 1:1 with the Rust structures in:
 *   crates/agent-core/src/models/{session,message,dataset,skill,error}.rs
 *   crates/agent-core/src/orchestrator.rs (AgentEvent)
 */

// ---------------------------------------------------------------------------
// Session
// ---------------------------------------------------------------------------

export type SessionId = string; // UUID
export type SessionStatus = 'Active' | 'Archived';

export interface SessionSettings {
  decision_assistant: boolean;
}

export type ProtocolStatus = 'Draft' | 'Approved';
export type StudyDesign =
  | 'cross_sectional'
  | 'cohort'
  | 'case_control'
  | 'randomized_trial'
  | 'other';

export interface ResearchProtocolInput {
  status: ProtocolStatus;
  /** Compare-and-swap guard required when updating an existing protocol. */
  expected_version?: number;
  research_question: string;
  study_design: StudyDesign;
  population: string;
  eligibility_criteria: string;
  exposure: string;
  comparator: string;
  outcome: string;
  time_zero: string;
  follow_up: string;
  analysis_unit: string;
  estimand: string;
  confounders: string;
  missing_data_strategy: string;
  primary_analysis: string;
  sensitivity_analysis: string;
}

export type ResearchProtocolProposal = Omit<ResearchProtocolInput, 'status' | 'expected_version'>;

export interface ProtocolCompileRequest {
  brief: string;
}

export interface ProtocolCompileResult {
  schema_version: '1.0';
  compiler_version: '1.0.0';
  proposal: ResearchProtocolProposal;
  missing_required_fields: Array<keyof ResearchProtocolProposal>;
  warnings: string[];
  brief_sha256: string;
  /** Always true: compilation never saves or approves a protocol. */
  approval_required: true;
}

export interface ResearchProtocol extends ResearchProtocolInput {
  version: number;
  content_sha256: string;
  state_sha256: string;
  approval_id: string | null;
  approved_at: string | null;
  updated_at: string;
}

export type DatasetAuditStatus = 'passed' | 'warning' | 'blocked';
export type DatasetAuditSeverity = 'warning' | 'blocker';

export interface DatasetAuditRoles {
  primary_key?: string[];
  time_zero?: string;
  exposure_time?: string;
  follow_up_end?: string;
  event?: string;
  person_time?: string;
  weight?: string;
  psu?: string;
  cluster?: string;
  pair_id?: string;
  repeat_index?: string;
}

export interface DatasetAuditFinding {
  code: string;
  severity: DatasetAuditSeverity;
  columns: string[];
  affected_rows: number;
  sample_row_numbers: number[];
  message: string;
}

export interface DatasetAudit {
  schema_version: '1.0';
  audit_rules_version: '1.1.0';
  audit_id: string;
  dataset_id: DatasetId;
  dataset_sha256: string;
  protocol_version: number;
  skill_id: string;
  run_spec_sha256: string;
  roles: DatasetAuditRoles;
  status: DatasetAuditStatus;
  findings: DatasetAuditFinding[];
  audit_sha256: string;
  created_at: string;
}

export interface AnalysisPlanApproval {
  schema_version: '1.0';
  plan_id: string;
  approval_id: string;
  status: 'Approved';
  protocol_version: number;
  protocol_sha256: string;
  protocol_approval_id: string;
  dataset_id: DatasetId;
  dataset_sha256: string;
  skill_id: string;
  args: Record<string, unknown>;
  run_spec_sha256: string;
  audit_id: string;
  audit_sha256: string;
  audit_roles: DatasetAuditRoles;
  approved_at: string;
}

export interface DatasetAuditRequest {
  skill_id: string;
  args: Record<string, unknown>;
  expected_protocol_version: number;
  audit_roles?: DatasetAuditRoles;
}

export interface AnalysisPlanApprovalRequest extends DatasetAuditRequest {
  dataset_id: DatasetId;
  expected_audit_id: string;
  expected_audit_sha256: string;
}

export interface SessionIntegrityWarning {
  event: 'file_session_integrity_warning';
  action: 'downgraded' | 'discarded';
  record_type: 'research_protocol' | 'dataset_audit' | 'analysis_plan_approval';
  session_id: SessionId;
  reason: string;
}

export interface Session {
  id: SessionId;
  status: SessionStatus;
  created_at: string; // ISO 8601
  last_active_at: string;
  settings: SessionSettings;
  /** Absent only for sessions saved before protocol support. */
  research_protocol?: ResearchProtocol | null;
  /** Server-computed history; absent only on legacy sessions. */
  dataset_audits?: DatasetAudit[];
  /** Server-issued approvals; client timestamps are never accepted. */
  analysis_plan_approvals?: AnalysisPlanApproval[];
  /** Fail-closed decisions made while validating a persisted file-backed session. */
  integrity_warnings?: SessionIntegrityWarning[];
  messages: Message[];
  datasets: DatasetSummary[];
  skill_runs: SkillRun[];
  uploaded_bytes: number;
}

/**
 * Lightweight session summary returned by `GET /api/sessions` (Requirement 11).
 * Mirrors the backend `sessionSummary` zod contract; never carries sensitive
 * fields. `title` is the first user text message (truncated) or "新对话".
 */
export interface SessionSummary {
  id: SessionId;
  status: SessionStatus;
  created_at: string;
  last_active_at: string;
  message_count: number;
  title: string;
  dataset_count: number;
}

// ---------------------------------------------------------------------------
// Message
// ---------------------------------------------------------------------------

export type MessageId = string; // UUID
export type PromptId = string; // UUID
export type OptionId = string;

export type Message =
  | { User: UserMessage }
  | { Agent: AgentMessage };

export interface UserMessage {
  id: MessageId;
  created_at: string;
  content: UserContent;
}

export type UserContent =
  | { Text: string }
  | { AudioTranscript: { text: string; confidence: number } }
  | { ChoiceAnswer: ChoiceAnswer };

export interface AgentMessage {
  id: MessageId;
  created_at: string;
  blocks: AgentBlock[];
}

export type AgentBlock =
  | { Text: string }
  | { ChoicePrompt: ChoicePrompt }
  | { SkillResult: { run_id: string; result: SkillResult } }
  | { Interpretation: string };

// ---------------------------------------------------------------------------
// Choice Prompt
// ---------------------------------------------------------------------------

export interface ChoicePrompt {
  prompt_id: PromptId;
  question: string;
  options: ChoiceOption[];
  multi_select: boolean;
  allow_custom_text: boolean;
  recommendation: OptionId | null;
}

export interface ChoiceOption {
  option_id: OptionId;
  text: string;
  explanation: string | null;
}

export interface ChoiceAnswer {
  prompt_id: PromptId;
  options: OptionId[];
  custom_text: string | null;
}

// ---------------------------------------------------------------------------
// Dataset
// ---------------------------------------------------------------------------

export type DatasetId = string; // UUID
export type Encoding = 'Utf8' | 'Gbk' | 'Utf16';
export type ColumnType = 'Numeric' | 'Categorical' | 'Date' | 'String';

export interface ColumnSummary {
  name: string;
  inferred_type: ColumnType;
  missing_count: number;
}

export interface DatasetSummary {
  dataset_id: DatasetId;
  file_name: string;
  size_bytes: number;
  encoding: Encoding;
  row_count: number;
  columns: ColumnSummary[];
  uploaded_at: string;
  sha256?: string | null;
  /** First N real rows for SPA preview (optional on legacy sessions). */
  preview_rows?: Record<string, string | number>[] | null;
}

// ---------------------------------------------------------------------------
// Skill
// ---------------------------------------------------------------------------

export type RunStatus = 'running' | 'completed' | 'failed';

export interface ResultContractEstimate {
  term: string;
  estimate: number;
  ci_95: { lower: number; upper: number } | null;
  p_value: number | null;
  effect_unit:
    | 'Beta'
    | 'OR'
    | 'HR'
    | 'Mean difference'
    | 'Median survival'
    | 'Correlation coefficient'
    | 'Eta squared';
  adjustment: 'adjusted' | 'unadjusted' | 'descriptive';
}

export interface StandardResultContract {
  schema_version: '1.0';
  method: { algorithm_id: string; method_version: string };
  estimates: ResultContractEstimate[];
  counts: {
    input_n: number;
    complete_case_n: number;
    missing_n: number;
    event_n: number | null;
    person_time: number | null;
  };
  analysis_availability: {
    unadjusted: 'available' | 'not_computed' | 'not_applicable';
    adjusted: 'available' | 'not_computed' | 'not_applicable';
  };
  effect_unit: ResultContractEstimate['effect_unit'] | null;
  convergence: { status: 'converged' | 'failed' | 'not_applicable' | 'unknown' };
  assumption_diagnostics: Array<{
    code: string;
    status: 'passed' | 'failed' | 'warning' | 'not_evaluated';
    message: string;
  }>;
  exclusions: Array<{ reason: string; n: number | null }>;
  interpretation: {
    statistical: string | null;
    practical_significance: string | null;
    unsupported_conclusions: string[];
  };
  provenance: {
    engine_name: '@stats-code/engine';
    engine_version: string;
    validation_coverage: Record<string, string>;
  };
}

export interface AnalysisResultMeta {
  algorithm_id: string;
  dataset_id: DatasetId;
  dataset_sha256: string | null;
  columns: ColumnSummary[];
  params: unknown;
  run_id: string;
  run_status: RunStatus;
  plan_id?: string;
  research_workflow?: {
    protocol_version: number;
    protocol_approval_id: string;
    plan_id: string;
    plan_approval_id: string;
    plan_approved_at: string;
    audit_id: string;
    audit_sha256: string;
  };
  result_contract?: StandardResultContract;
}

export type SkillRunId = string; // UUID

export interface SkillRun {
  run_id: SkillRunId;
  skill_id: string;
  args: unknown;
  started_at: string;
  finished_at: string | null;
  outcome: SkillOutcome;
}

export type SkillOutcome =
  | 'Pending'
  | { Ok: SkillResult }
  | { Failed: SkillError };

export interface SkillResult {
  schema_version: string;
  payload: unknown;
  risk_signals: RiskSignal[];
  analysis?: AnalysisResultMeta;
}

export type RiskSignal =
  // Legacy wire value retained for historical sessions; hidden by the UI.
  | 'PValueAboveAlpha'
  | 'VifTooHigh'
  | 'LowPower'
  | 'CoxPhAssumptionViolated'
  | 'ModelConvergenceFailed'
  | 'SparseData'
  | 'CollinearityDetected';

export interface SkillError {
  message: string;
  stderr_excerpt: string | null;
}

/**
 * RunRequest — body for `POST /api/sessions/:sid/run` (Requirement 12).
 * Targets a skill (or its algorithm) + dataset with an args bag; the response
 * reuses the `SkillResult` shape.
 */
export interface RunRequest {
  skill_id: string;
  dataset_id: DatasetId;
  args: Record<string, unknown>;
  /** Server-issued plan id. Required by the formal-analysis gate. */
  plan_id?: string;
}

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

export type ErrorCode =
  | 'MessageTooLong'
  | 'AudioTooLarge'
  | 'DatasetTooLarge'
  | 'DatasetEmpty'
  | 'InvalidChoice'
  | 'SkillInvalidArgs'
  | 'SkillTimeout'
  | 'SkillOom'
  | 'SkillExecutionFailed'
  | 'LlmUnavailable'
  | 'SessionNotFound'
  | 'SessionArchived'
  | 'SessionQuotaExceeded'
  | 'ResearchProtocolRequired'
  | 'ResearchApprovalRequired'
  | 'ResearchApprovalStale'
  | 'ResearchAuditBlocked'
  | 'ResearchVersionConflict';

export interface ErrorPayload {
  error_code: ErrorCode;
  message: string;
  details?: unknown;
}

// ---------------------------------------------------------------------------
// Agent Event (SSE stream)
// ---------------------------------------------------------------------------
//
// Wire format (POST /api/sessions/:sid/messages, text/event-stream):
//   event: text_delta|choice_prompt|skill_call|skill_result|interpretation|error|done
//   data:  JSON (see serializeSseFrame in ts-backend packages/server/src/sse.ts)
//
// Contract stability note (2026-07):
//   Backend conversation upgrades (method notes, column sanitization, heuristic
//   args) keep the same event names and JSON envelopes. Frontend must not parse
//   interpretation as numeric result reading — numbers live in skill_result.

export type AgentEvent =
  | { TextDelta: string }
  | { ChoicePrompt: ChoicePrompt }
  | { SkillCall: { skill_id: string; args: unknown } }
  | { SkillResult: SkillResult }
  /** Methodology tip string (deterministic method note ± optional LLM tip). */
  | { Interpretation: string }
  | { Error: ErrorPayload }
  | 'Done';

// ---------------------------------------------------------------------------
// API Response types
// ---------------------------------------------------------------------------

export interface PostAudioResponse {
  text: string;
  confidence: number;
  /**
   * Whether the transcription was high-confidence enough to be auto-sent
   * (≥ 0.6). When false, the frontend should ask user to confirm or edit.
   */
  auto_processed: boolean;
}

export interface HealthResponse {
  status: string;
}

// ---------------------------------------------------------------------------
// LLM Config (single-command-launcher)
// ---------------------------------------------------------------------------

/**
 * LLM provider identifier — wire format matches the Rust `Provider` enum
 * in `crates/stats-code/src/launcher/config_store.rs`
 * (`#[serde(rename_all = "lowercase")]`).
 */
export type LlmProvider = 'deepseek' | 'openai';

/**
 * Response shape for `GET /api/llm-status`.
 *
 * Per Requirement 10:
 *   - Unconfigured  → `{ configured: false, provider: null }`
 *   - Configured    → `{ configured: true, provider: <one of LlmProvider> }`
 *   - api_key is never present (Requirement 10.4).
 */
export interface LlmStatusResponse {
  configured: boolean;
  provider: LlmProvider | null;
  base_url?: string | null;
  model?: string | null;
}

/**
 * Frontend-only runtime error info recorded when a chat request comes back
 * with `error.code === "LLM_UPSTREAM_ERROR"` (Requirement 12.1).
 *
 * `last_message_id` lets the Connection_Banner "重试" button re-send the
 * specific user message that failed.
 */
export interface LlmRuntimeError {
  provider: LlmProvider | null;
  summary: string;
  last_message_id: MessageId | null;
}
