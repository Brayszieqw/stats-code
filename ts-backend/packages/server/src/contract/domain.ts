// contract/domain.ts — zod transcription of the agent-core serde DTOs.
//
// These mirror the Rust domain models 1:1 (crates/agent-core/src/models/*).
// Rust enums serialize as externally-tagged JSON by default (serde):
//   - unit variants → the variant name as a string (e.g. "Active")
//   - data variants → { "VariantName": <payload> }
// Where the Rust type uses #[serde(rename_all = "lowercase")] or #[serde(rename)]
// we match the exact wire token. The inferred TS types are the compile-time
// contract; the schemas are the runtime contract and the contract-test harness.

import { z } from 'zod';

// --- primitives -----------------------------------------------------------

/** Rust Uuid → string (canonical hyphenated form). */
export const uuid = z.string().uuid();

/** RFC3339 / ISO-8601 timestamp (chrono DateTime<Utc> via serde). */
export const dateTime = z.string().datetime({ offset: true });

// --- dataset.rs -----------------------------------------------------------

export const encoding = z.enum(['Utf8', 'Gbk', 'Utf16']);
export type Encoding = z.infer<typeof encoding>;

export const columnType = z.enum(['Numeric', 'Categorical', 'Date', 'String']);
export type ColumnType = z.infer<typeof columnType>;

export const columnSummary = z.object({
  name: z.string(),
  inferred_type: columnType,
  missing_count: z.number().int().nonnegative(),
});
export type ColumnSummary = z.infer<typeof columnSummary>;

export const datasetSummary = z.object({
  dataset_id: uuid,
  file_name: z.string(),
  size_bytes: z.number().int().nonnegative(),
  encoding,
  row_count: z.number().int().nonnegative(),
  columns: z.array(columnSummary),
  uploaded_at: dateTime,
  // serde(default): always serialized as string|null; absent allowed for legacy.
  sha256: z.string().nullable().optional(),
});
export type DatasetSummary = z.infer<typeof datasetSummary>;

// --- skill.rs / run.rs ----------------------------------------------------

export const riskSignal = z.enum([
  'PValueAboveAlpha',
  'VifTooHigh',
  'LowPower',
  'CoxPhAssumptionViolated',
]);
export type RiskSignal = z.infer<typeof riskSignal>;

// AnalysisResultMeta is an opaque structured payload at the wire boundary;
// the SPA treats it as data. Kept permissive until run.rs is transcribed in
// the snapshot phase.
export const analysisResultMeta = z.record(z.unknown());

export const skillResult = z.object({
  schema_version: z.string(),
  payload: z.unknown(),
  risk_signals: z.array(riskSignal),
  analysis: analysisResultMeta.nullable().optional(),
});
export type SkillResult = z.infer<typeof skillResult>;

export const skillError = z.object({
  message: z.string(),
  stderr_excerpt: z.string().nullable(),
});

/**
 * RunRequest — code-run endpoint body (Requirement 12). Targets a skill (or its
 * algorithm) + dataset with an args bag. Response reuses the `skillResult` shape.
 */
export const runRequest = z.object({
  skill_id: z.string(),
  dataset_id: z.string(),
  args: z.record(z.string(), z.unknown()).default({}),
});
export type RunRequest = z.infer<typeof runRequest>;

// SkillOutcome: externally-tagged enum.
export const skillOutcome = z.union([
  z.literal('Pending'),
  z.object({ Ok: skillResult }),
  z.object({ Failed: skillError }),
]);

export const skillRun = z.object({
  run_id: uuid,
  skill_id: z.string(),
  args: z.unknown(),
  started_at: dateTime,
  finished_at: dateTime.nullable(),
  outcome: skillOutcome,
});
export type SkillRun = z.infer<typeof skillRun>;

// --- message.rs -----------------------------------------------------------

export const choiceOption = z.object({
  option_id: z.string(),
  text: z.string(),
  explanation: z.string().nullable(),
});

export const choicePrompt = z.object({
  prompt_id: uuid,
  question: z.string(),
  options: z.array(choiceOption),
  multi_select: z.boolean(),
  allow_custom_text: z.boolean(),
  recommendation: z.string().nullable(),
});
export type ChoicePrompt = z.infer<typeof choicePrompt>;

// UserContent: externally-tagged enum.
export const userContent = z.union([
  z.object({ Text: z.string() }),
  z.object({ AudioTranscript: z.object({ text: z.string(), confidence: z.number() }) }),
  z.object({
    ChoiceAnswer: z.object({
      prompt_id: uuid,
      options: z.array(z.string()),
      custom_text: z.string().nullable(),
    }),
  }),
]);

export const userMessage = z.object({
  id: uuid,
  created_at: dateTime,
  content: userContent,
});

// AgentBlock: externally-tagged enum.
export const agentBlock = z.union([
  z.object({ Text: z.string() }),
  z.object({ ChoicePrompt: choicePrompt }),
  z.object({ SkillResult: z.object({ run_id: uuid, result: skillResult }) }),
  z.object({ Interpretation: z.string() }),
]);

export const agentMessage = z.object({
  id: uuid,
  created_at: dateTime,
  blocks: z.array(agentBlock),
});

// Message: externally-tagged enum { User: ... } | { Agent: ... }.
export const message = z.union([
  z.object({ User: userMessage }),
  z.object({ Agent: agentMessage }),
]);
export type Message = z.infer<typeof message>;

// --- session.rs -----------------------------------------------------------

export const sessionStatus = z.enum(['Active', 'Archived']);
export type SessionStatus = z.infer<typeof sessionStatus>;

export const sessionSettings = z.object({
  decision_assistant: z.boolean(),
});
export type SessionSettings = z.infer<typeof sessionSettings>;

// SessionId is a newtype `struct SessionId(pub Uuid)` → serializes as a bare Uuid.
export const sessionId = uuid;

export const session = z.object({
  id: sessionId,
  status: sessionStatus,
  created_at: dateTime,
  last_active_at: dateTime,
  settings: sessionSettings,
  messages: z.array(message),
  datasets: z.array(datasetSummary),
  skill_runs: z.array(skillRun),
  uploaded_bytes: z.number().int().nonnegative(),
});
export type Session = z.infer<typeof session>;

/**
 * SessionSummary — lightweight history-list projection (Requirement 11).
 * Derived from a Session; never carries sensitive fields. `title` is the first
 * user text message (truncated), otherwise "新对话".
 */
export const sessionSummary = z.object({
  id: sessionId,
  status: sessionStatus,
  created_at: dateTime,
  last_active_at: dateTime,
  message_count: z.number().int().nonnegative(),
  title: z.string(),
  dataset_count: z.number().int().nonnegative(),
});
export type SessionSummary = z.infer<typeof sessionSummary>;

// --- error.rs -------------------------------------------------------------

export const errorCode = z.enum([
  'MessageTooLong',
  'AudioTooLarge',
  'DatasetTooLarge',
  'DatasetEmpty',
  'InvalidChoice',
  'SkillInvalidArgs',
  'SkillTimeout',
  'SkillOom',
  'SkillExecutionFailed',
  'LlmUnavailable',
  'SessionNotFound',
  'SessionArchived',
  'SessionQuotaExceeded',
]);
export type ErrorCode = z.infer<typeof errorCode>;

export const errorPayload = z.object({
  error_code: errorCode,
  message: z.string(),
  // serde(skip_serializing_if = Option::is_none): absent when null.
  details: z.unknown().optional(),
});
export type ErrorPayload = z.infer<typeof errorPayload>;

/**
 * ErrorCode → HTTP status. Single source of truth, transcribed from
 * agent-core::models::error::http_status_for.
 */
export const HTTP_STATUS_FOR: Record<ErrorCode, number> = {
  MessageTooLong: 413,
  AudioTooLarge: 413,
  DatasetTooLarge: 413,
  SessionQuotaExceeded: 413,
  DatasetEmpty: 422,
  InvalidChoice: 422,
  SkillInvalidArgs: 422,
  SkillTimeout: 504,
  SkillOom: 507,
  SkillExecutionFailed: 500,
  LlmUnavailable: 502,
  SessionNotFound: 404,
  SessionArchived: 409,
};

// --- llm_config.rs --------------------------------------------------------

export const llmProvider = z.enum(['deepseek', 'openai']);
export type LlmProvider = z.infer<typeof llmProvider>;
