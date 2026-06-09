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

export interface Session {
  id: SessionId;
  status: SessionStatus;
  created_at: string; // ISO 8601
  last_active_at: string;
  settings: SessionSettings;
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
}

// ---------------------------------------------------------------------------
// Skill
// ---------------------------------------------------------------------------

export type RunStatus = 'running' | 'completed' | 'failed';

export interface AnalysisResultMeta {
  algorithm_id: string;
  dataset_id: DatasetId;
  dataset_sha256: string | null;
  columns: ColumnSummary[];
  params: unknown;
  run_id: string;
  run_status: RunStatus;
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
  | 'PValueAboveAlpha'
  | 'VifTooHigh'
  | 'LowPower'
  | 'CoxPhAssumptionViolated';

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
  | 'SessionQuotaExceeded';

export interface ErrorPayload {
  error_code: ErrorCode;
  message: string;
  details?: unknown;
}

// ---------------------------------------------------------------------------
// Agent Event (SSE stream)
// ---------------------------------------------------------------------------

export type AgentEvent =
  | { TextDelta: string }
  | { ChoicePrompt: ChoicePrompt }
  | { SkillCall: { skill_id: string; args: unknown } }
  | { SkillResult: SkillResult }
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
