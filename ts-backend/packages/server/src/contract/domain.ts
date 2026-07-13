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
  /** First N data rows for SPA preview (not required for legacy sessions). */
  preview_rows: z.array(z.record(z.unknown())).optional(),
});
export type DatasetSummary = z.infer<typeof datasetSummary>;

// --- skill.rs / run.rs ----------------------------------------------------

export const riskSignal = z.enum([
  // Legacy wire value retained so saved sessions remain readable. New runs do
  // not emit it: statistical non-significance is not a method risk.
  'PValueAboveAlpha',
  'VifTooHigh',
  'LowPower',
  'CoxPhAssumptionViolated',
  'ModelConvergenceFailed',
  'SparseData',
  'CollinearityDetected',
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
  /** Server-issued analysis-plan id. Optional only so the route can return a machine-readable 428. */
  plan_id: uuid.optional(),
}).strict();
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

export const protocolStatus = z.enum(['Draft', 'Approved']);
export type ProtocolStatus = z.infer<typeof protocolStatus>;

export const studyDesign = z.enum([
  'cross_sectional',
  'cohort',
  'case_control',
  'randomized_trial',
  'other',
]);
export type StudyDesign = z.infer<typeof studyDesign>;

const protocolText = z.string().max(4000);

/** The 15-field observational-research protocol card. */
export const researchProtocolFields = z.object({
  research_question: protocolText,
  study_design: studyDesign,
  population: protocolText,
  eligibility_criteria: protocolText,
  exposure: protocolText,
  comparator: protocolText,
  outcome: protocolText,
  time_zero: protocolText,
  follow_up: protocolText,
  analysis_unit: protocolText,
  estimand: protocolText,
  confounders: protocolText,
  missing_data_strategy: protocolText,
  primary_analysis: protocolText,
  sensitivity_analysis: protocolText,
});

export const APPROVAL_REQUIRED_FIELDS = [
  'research_question',
  'population',
  'outcome',
  'time_zero',
  'analysis_unit',
  'estimand',
  'primary_analysis',
] as const;

export const approvalRequiredProtocolField = z.enum(APPROVAL_REQUIRED_FIELDS);

export const researchProtocolInput = researchProtocolFields
  .extend({
    status: protocolStatus,
    /** Compare-and-swap guard for an already persisted protocol. */
    expected_version: z.number().int().positive().optional(),
  })
  .strict()
  .superRefine((value, ctx) => {
    if (value.status !== 'Approved') return;
    for (const field of APPROVAL_REQUIRED_FIELDS) {
      if (value[field].trim().length > 0) continue;
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: [field],
        message: '协议审批前必须填写',
      });
    }
  });
export type ResearchProtocolInput = z.infer<typeof researchProtocolInput>;

export const researchProtocol = researchProtocolFields.extend({
  status: protocolStatus,
  /** Monotonic content revision, generated by the server. */
  version: z.number().int().positive(),
  content_sha256: z.string().regex(/^[0-9a-f]{64}$/),
  /** Integrity hash over the complete server-owned protocol envelope. */
  state_sha256: z.string().regex(/^[0-9a-f]{64}$/),
  approval_id: uuid.nullable(),
  approved_at: dateTime.nullable(),
  updated_at: dateTime,
});
export type ResearchProtocol = z.infer<typeof researchProtocol>;

/** Natural-language input accepted by the review-only protocol compiler. */
export const protocolCompileRequest = z.object({
  brief: z.string().trim().min(20).max(8000),
}).strict();
export type ProtocolCompileRequest = z.infer<typeof protocolCompileRequest>;

/** Strict proposal shape: model output may never smuggle approval metadata. */
export const protocolCompileProposal = researchProtocolFields.strict();

export const protocolCompileResult = z.object({
  schema_version: z.literal('1.0'),
  compiler_version: z.literal('1.0.0'),
  proposal: protocolCompileProposal,
  missing_required_fields: z.array(approvalRequiredProtocolField),
  warnings: z.array(z.string().max(500)).max(8),
  brief_sha256: z.string().regex(/^[0-9a-f]{64}$/),
  /** A compiler result is always a proposal and never an approval. */
  approval_required: z.literal(true),
}).strict();
export type ProtocolCompileResult = z.infer<typeof protocolCompileResult>;

export const datasetAuditStatus = z.enum(['passed', 'warning', 'blocked']);
export type DatasetAuditStatus = z.infer<typeof datasetAuditStatus>;

export const datasetAuditSeverity = z.enum(['warning', 'blocker']);
export type DatasetAuditSeverity = z.infer<typeof datasetAuditSeverity>;

export const datasetAuditFindingCode = z.enum([
  'PRIMARY_KEY_MISSING',
  'DUPLICATE_PRIMARY_KEY',
  'DUPLICATE_OBSERVATION_KEY',
  'TIME_VALUE_INVALID',
  'TIME_ORDER_INVALID',
  'IMMORTAL_TIME_RISK',
  'PERSON_TIME_NONPOSITIVE',
  'EVENT_ENCODING_INVALID',
  'EVENT_NO_VARIATION',
  'SURVEY_DESIGN_UNSUPPORTED',
  'CLUSTERING_UNSUPPORTED',
  'PAIRED_REPEATED_UNSUPPORTED',
  'SENSITIVE_FIELD_PRESENT',
  'PRIMARY_KEY_UNBOUND',
  'POSSIBLE_CLUSTERING',
  'ANALYSIS_COLUMN_MISSING',
  'ANALYSIS_VALUE_MISSING',
  'AUDIT_ROLE_COLUMN_MISSING',
  'AUDIT_ROLE_OVERRIDE_REJECTED',
  'DATASET_NO_ROWS',
  'HEADER_INVALID',
  'ROW_WIDTH_MISMATCH',
]);
export type DatasetAuditFindingCode = z.infer<typeof datasetAuditFindingCode>;

export const datasetAuditRoles = z.object({
  primary_key: z.array(z.string().min(1)).min(1).optional(),
  time_zero: z.string().min(1).optional(),
  exposure_time: z.string().min(1).optional(),
  follow_up_end: z.string().min(1).optional(),
  event: z.string().min(1).optional(),
  person_time: z.string().min(1).optional(),
  weight: z.string().min(1).optional(),
  psu: z.string().min(1).optional(),
  cluster: z.string().min(1).optional(),
  pair_id: z.string().min(1).optional(),
  repeat_index: z.string().min(1).optional(),
}).strict();
export type DatasetAuditRoles = z.infer<typeof datasetAuditRoles>;

export const datasetAuditFinding = z.object({
  code: datasetAuditFindingCode,
  severity: datasetAuditSeverity,
  columns: z.array(z.string()),
  affected_rows: z.number().int().nonnegative(),
  /** 1-based data row numbers, capped by the service; never includes cell values. */
  sample_row_numbers: z.array(z.number().int().positive()).max(5),
  message: z.string(),
});
export type DatasetAuditFinding = z.infer<typeof datasetAuditFinding>;

export const datasetAudit = z.object({
  schema_version: z.literal('1.0'),
  audit_rules_version: z.literal('1.1.0'),
  audit_id: uuid,
  dataset_id: uuid,
  dataset_sha256: z.string().regex(/^[0-9a-f]{64}$/),
  protocol_version: z.number().int().positive(),
  skill_id: z.string(),
  run_spec_sha256: z.string().regex(/^[0-9a-f]{64}$/),
  roles: datasetAuditRoles,
  status: datasetAuditStatus,
  findings: z.array(datasetAuditFinding),
  audit_sha256: z.string().regex(/^[0-9a-f]{64}$/),
  created_at: dateTime,
});
export type DatasetAudit = z.infer<typeof datasetAudit>;

export const analysisPlanApproval = z.object({
  schema_version: z.literal('1.0'),
  plan_id: uuid,
  approval_id: uuid,
  status: z.literal('Approved'),
  protocol_version: z.number().int().positive(),
  protocol_sha256: z.string().regex(/^[0-9a-f]{64}$/),
  protocol_approval_id: uuid,
  dataset_id: uuid,
  dataset_sha256: z.string().regex(/^[0-9a-f]{64}$/),
  skill_id: z.string(),
  args: z.record(z.string(), z.unknown()),
  run_spec_sha256: z.string().regex(/^[0-9a-f]{64}$/),
  audit_id: uuid,
  audit_sha256: z.string().regex(/^[0-9a-f]{64}$/),
  audit_roles: datasetAuditRoles,
  approved_at: dateTime,
});
export type AnalysisPlanApproval = z.infer<typeof analysisPlanApproval>;

export const datasetAuditRequest = z.object({
  skill_id: z.string(),
  args: z.record(z.string(), z.unknown()).default({}),
  expected_protocol_version: z.number().int().positive(),
  audit_roles: datasetAuditRoles.optional().default({}),
}).strict();
export type DatasetAuditRequest = z.infer<typeof datasetAuditRequest>;

export const analysisPlanApprovalRequest = z.object({
  skill_id: z.string(),
  dataset_id: uuid,
  args: z.record(z.string(), z.unknown()).default({}),
  expected_protocol_version: z.number().int().positive(),
  expected_audit_id: uuid,
  expected_audit_sha256: z.string().regex(/^[0-9a-f]{64}$/),
  audit_roles: datasetAuditRoles.optional().default({}),
}).strict();
export type AnalysisPlanApprovalRequest = z.infer<typeof analysisPlanApprovalRequest>;

// SessionId is a newtype `struct SessionId(pub Uuid)` → serializes as a bare Uuid.
export const sessionId = uuid;

export const session = z.object({
  id: sessionId,
  status: sessionStatus,
  created_at: dateTime,
  last_active_at: dateTime,
  settings: sessionSettings,
  // Optional on the wire so sessions created before the protocol feature remain readable.
  research_protocol: researchProtocol.nullable().optional(),
  /** Server-computed audit history; optional only for legacy sessions. */
  dataset_audits: z.array(datasetAudit).optional(),
  /** Server-issued approvals; old client timestamps are never migrated into this list. */
  analysis_plan_approvals: z.array(analysisPlanApproval).optional(),
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
  'ResearchProtocolRequired',
  'ResearchApprovalRequired',
  'ResearchApprovalStale',
  'ResearchAuditBlocked',
  'ResearchVersionConflict',
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
  ResearchProtocolRequired: 428,
  ResearchApprovalRequired: 428,
  ResearchApprovalStale: 409,
  ResearchAuditBlocked: 409,
  ResearchVersionConflict: 409,
};

// --- llm_config.rs --------------------------------------------------------

export const llmProvider = z.enum(['deepseek', 'openai']);
export type LlmProvider = z.infer<typeof llmProvider>;
