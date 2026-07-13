import { randomUUID } from 'node:crypto';
import type {
  AnalysisPlanApproval,
  DatasetAudit,
  DatasetStore,
  ResearchProtocol,
  ResearchWorkflowApproveInput,
  ResearchWorkflowAuditInput,
  ResearchWorkflowExecuteInput,
  ResearchWorkflowService,
  Session,
  SessionStore,
  SkillRegistryLike,
  SkillRunnerLike,
  SnapshotRunRecorder,
} from '../state.js';
import { auditDataset as computeDatasetAudit } from './dataset-audit.js';
import {
  findReservedWorkflowKey,
  normalizedRunArgs,
  runSpecSha256,
  sha256Bytes,
} from './research-integrity.js';

const ALGORITHM_TO_SKILL_ID: Readonly<Record<string, string>> = {
  linear: 'model_linear',
  logistic: 'model_logistic',
  cox: 'model_cox',
  kaplan_meier: 'survival_km',
};

export type ResearchWorkflowErrorCode =
  | 'SkillInvalidArgs'
  | 'SessionArchived'
  | 'ResearchProtocolRequired'
  | 'ResearchApprovalRequired'
  | 'ResearchApprovalStale'
  | 'ResearchAuditBlocked'
  | 'ResearchVersionConflict';

export class ResearchWorkflowError extends Error {
  constructor(
    public readonly code: ResearchWorkflowErrorCode,
    message: string,
    public readonly status: number,
    public readonly details?: unknown,
  ) {
    super(message);
    this.name = 'ResearchWorkflowError';
  }
}

export interface CreateResearchWorkflowServiceOptions {
  sessionStore: SessionStore;
  datasetStore: DatasetStore;
  registry: SkillRegistryLike;
  runner: SkillRunnerLike;
  snapshotRunRecorder?: SnapshotRunRecorder;
  now?: () => Date;
}

function resolveDescriptor(registry: SkillRegistryLike, requestedId: string) {
  return registry.get(requestedId) ?? registry.get(ALGORITHM_TO_SKILL_ID[requestedId] ?? '');
}

function validatePlanArgs(
  descriptor: { inputSchema: Record<string, unknown> },
  args: Record<string, unknown>,
  datasetId: string,
): void {
  const candidate: Record<string, unknown> = { ...args, dataset_id: datasetId };
  const required = Array.isArray(descriptor.inputSchema.required)
    ? descriptor.inputSchema.required.filter((value): value is string => typeof value === 'string')
    : [];
  const missing = required.filter((key) => candidate[key] === undefined || candidate[key] === null);
  if (missing.length > 0) {
    throw new ResearchWorkflowError(
      'SkillInvalidArgs',
      `方案缺少必填参数：${missing.join('、')}`,
      422,
    );
  }
  const properties = descriptor.inputSchema.properties;
  if (!properties || typeof properties !== 'object' || Array.isArray(properties)) return;
  for (const [key, value] of Object.entries(candidate)) {
    const rule = (properties as Record<string, unknown>)[key];
    if (!rule || typeof rule !== 'object' || Array.isArray(rule)) continue;
    const type = (rule as Record<string, unknown>).type;
    const valid = type === 'string' ? typeof value === 'string' && value.length > 0
      : type === 'array' ? Array.isArray(value)
        : type === 'number' ? typeof value === 'number' && Number.isFinite(value)
          : type === 'integer' ? typeof value === 'number' && Number.isInteger(value)
            : type === 'boolean' ? typeof value === 'boolean'
              : true;
    if (!valid) {
      throw new ResearchWorkflowError('SkillInvalidArgs', `方案参数 ${key} 类型无效。`, 422);
    }
  }
}

function requireActive(session: Session): void {
  if (session.status === 'Archived') {
    throw new ResearchWorkflowError('SessionArchived', '会话已归档，仅支持只读访问', 409);
  }
}

function requireApprovedProtocol(session: Session, expectedVersion?: number): ResearchProtocol {
  const protocol = session.research_protocol ?? null;
  if (!protocol || protocol.status !== 'Approved' || !protocol.approval_id) {
    throw new ResearchWorkflowError(
      'ResearchProtocolRequired',
      '必须先由服务端保存并审批当前研究协议。',
      428,
    );
  }
  if (expectedVersion !== undefined && protocol.version !== expectedVersion) {
    throw new ResearchWorkflowError(
      'ResearchVersionConflict',
      `协议版本冲突：当前为 v${protocol.version}，请求基于 v${expectedVersion}。`,
      409,
      { current_version: protocol.version, expected_version: expectedVersion },
    );
  }
  return protocol;
}

function assertSafeArgs(args: Record<string, unknown>): void {
  const reserved = findReservedWorkflowKey(args);
  if (reserved) {
    throw new ResearchWorkflowError(
      'SkillInvalidArgs',
      `参数 ${reserved} 由服务端管理，客户端不得提交。`,
      422,
    );
  }
}

function findDataset(session: Session, datasetId: string) {
  const summary = session.datasets.find((dataset) => dataset.dataset_id === datasetId);
  if (!summary) {
    throw new ResearchWorkflowError(
      'SkillInvalidArgs',
      `数据集不属于当前会话：${datasetId}`,
      422,
    );
  }
  return summary;
}

function blockerDetails(audit: DatasetAudit): unknown {
  return {
    audit_id: audit.audit_id,
    audit_sha256: audit.audit_sha256,
    status: audit.status,
    findings: audit.findings,
  };
}

export function createResearchWorkflowService(
  options: CreateResearchWorkflowServiceOptions,
): ResearchWorkflowService {
  const now = options.now ?? (() => new Date());

  async function loadAuditContext(input: ResearchWorkflowAuditInput) {
    assertSafeArgs(input.args);
    const session = await options.sessionStore.get(input.sessionId);
    requireActive(session);
    const protocol = requireApprovedProtocol(session, input.expectedProtocolVersion);
    const descriptor = resolveDescriptor(options.registry, input.skillId);
    if (!descriptor) {
      throw new ResearchWorkflowError('SkillInvalidArgs', `未知统计方法：${input.skillId}`, 422);
    }
    const summary = findDataset(session, input.datasetId);
    validatePlanArgs(descriptor, input.args, summary.dataset_id);
    const bytes = await options.datasetStore.readRawById(summary.dataset_id);
    const actualSha = sha256Bytes(bytes);
    if (!summary.sha256 || summary.sha256.toLowerCase() !== actualSha) {
      throw new ResearchWorkflowError(
        'ResearchApprovalStale',
        '原始数据指纹与会话摘要不一致；请重新上传并审批方案。',
        409,
        { dataset_id: summary.dataset_id, expected_sha256: summary.sha256 ?? null, actual_sha256: actualSha },
      );
    }
    const canonicalSkillId = descriptor.skillId;
    const args = normalizedRunArgs(input.args);
    const audit = computeDatasetAudit({
      datasetId: summary.dataset_id,
      datasetSha256: actualSha,
      bytes,
      fileName: summary.file_name,
      protocolVersion: protocol.version,
      skillId: canonicalSkillId,
      args,
      roles: input.auditRoles,
      now,
    });
    return { session, protocol, descriptor, summary, bytes, actualSha, args, audit };
  }

  return {
    now,

    async auditDataset(input) {
      const context = await loadAuditContext(input);
      await options.sessionStore.appendDatasetAudit(input.sessionId, context.audit);
      return context.audit;
    },

    async approveAnalysisPlan(input: ResearchWorkflowApproveInput) {
      const context = await loadAuditContext(input);
      const reviewedAudit = (context.session.dataset_audits ?? []).find((audit) => (
        audit.audit_id === input.expectedAuditId
        && audit.audit_sha256 === input.expectedAuditSha256
      ));
      if (
        !reviewedAudit
        || reviewedAudit.dataset_id !== context.summary.dataset_id
        || reviewedAudit.protocol_version !== context.protocol.version
        || reviewedAudit.skill_id !== context.descriptor.skillId
        || reviewedAudit.run_spec_sha256 !== context.audit.run_spec_sha256
        || reviewedAudit.audit_sha256 !== context.audit.audit_sha256
      ) {
        throw new ResearchWorkflowError(
          'ResearchApprovalStale',
          '待审批方案与已审阅的服务端数据审计不一致，请重新审计。',
          409,
        );
      }
      if (context.audit.status === 'blocked') {
        throw new ResearchWorkflowError(
          'ResearchAuditBlocked',
          '数据审计存在阻断项，不能批准分析方案。',
          409,
          blockerDetails(context.audit),
        );
      }
      const timestamp = now().toISOString();
      const approval: AnalysisPlanApproval = {
        schema_version: '1.0',
        plan_id: randomUUID(),
        approval_id: randomUUID(),
        status: 'Approved',
        protocol_version: context.protocol.version,
        protocol_sha256: context.protocol.content_sha256,
        protocol_approval_id: context.protocol.approval_id!,
        dataset_id: context.summary.dataset_id,
        dataset_sha256: context.actualSha,
        skill_id: context.descriptor.skillId,
        args: context.args,
        run_spec_sha256: runSpecSha256(
          context.descriptor.skillId,
          context.summary.dataset_id,
          context.args,
        ),
        audit_id: reviewedAudit.audit_id,
        audit_sha256: reviewedAudit.audit_sha256,
        audit_roles: reviewedAudit.roles,
        approved_at: timestamp,
      };
      const persisted = await options.sessionStore.appendAnalysisPlanApproval(input.sessionId, approval);
      if (!persisted) {
        throw new ResearchWorkflowError(
          'ResearchApprovalStale',
          '方案审批期间协议或会话状态已变化，审批未生效。',
          409,
        );
      }
      return approval;
    },

    async execute(input: ResearchWorkflowExecuteInput) {
      assertSafeArgs(input.args);
      const session = await options.sessionStore.get(input.sessionId);
      requireActive(session);
      const protocol = requireApprovedProtocol(session);
      const descriptor = resolveDescriptor(options.registry, input.skillId);
      if (!descriptor) throw new ResearchWorkflowError('SkillInvalidArgs', `未知统计方法：${input.skillId}`, 422);
      const summary = findDataset(session, input.datasetId);
      const args = normalizedRunArgs(input.args);
      const specHash = runSpecSha256(descriptor.skillId, summary.dataset_id, args);
      const approvals = session.analysis_plan_approvals ?? [];
      const approval = input.planId
        ? approvals.find((candidate) => candidate.plan_id === input.planId)
        : input.allowMatchingPlan
          ? [...approvals].reverse().find((candidate) => candidate.run_spec_sha256 === specHash)
          : undefined;
      if (!approval) {
        throw new ResearchWorkflowError(
          'ResearchApprovalRequired',
          '本次统计方案尚未由服务端批准。请先完成数据审计并批准方案。',
          428,
        );
      }
      if (
        approval.run_spec_sha256 !== specHash
        || approval.skill_id !== descriptor.skillId
        || approval.dataset_id !== summary.dataset_id
        || approval.protocol_version !== protocol.version
        || approval.protocol_sha256 !== protocol.content_sha256
        || approval.protocol_approval_id !== protocol.approval_id
      ) {
        throw new ResearchWorkflowError(
          'ResearchApprovalStale',
          '已批准方案与当前协议、数据或参数不一致，请重新审计并审批。',
          409,
        );
      }

      const bytes = await options.datasetStore.readRawById(summary.dataset_id);
      const actualSha = sha256Bytes(bytes);
      if (actualSha !== approval.dataset_sha256 || actualSha !== summary.sha256?.toLowerCase()) {
        throw new ResearchWorkflowError(
          'ResearchApprovalStale',
          '方案批准后原始数据已发生变化，请重新上传并审批。',
          409,
          { dataset_id: summary.dataset_id, approved_sha256: approval.dataset_sha256, actual_sha256: actualSha },
        );
      }
      const currentAudit = computeDatasetAudit({
        datasetId: summary.dataset_id,
        datasetSha256: actualSha,
        bytes,
        fileName: summary.file_name,
        protocolVersion: protocol.version,
        skillId: descriptor.skillId,
        args,
        roles: approval.audit_roles,
        now,
      });
      if (currentAudit.status === 'blocked') {
        await options.sessionStore.appendDatasetAudit(input.sessionId, currentAudit);
        throw new ResearchWorkflowError(
          'ResearchAuditBlocked',
          '运行前复核发现数据阻断项，执行已拒绝。',
          409,
          blockerDetails(currentAudit),
        );
      }
      if (currentAudit.audit_sha256 !== approval.audit_sha256) {
        await options.sessionStore.appendDatasetAudit(input.sessionId, currentAudit);
        throw new ResearchWorkflowError(
          'ResearchApprovalStale',
          '当前数据审计结果与批准时不一致，请重新审批。',
          409,
        );
      }
      const approvedAudit = (session.dataset_audits ?? []).find((audit) => (
        audit.audit_id === approval.audit_id
        && audit.audit_sha256 === approval.audit_sha256
      ));
      if (!approvedAudit) {
        throw new ResearchWorkflowError(
          'ResearchApprovalStale',
          '方案引用的服务端审计记录不存在或已失效，请重新审计并审批。',
          409,
        );
      }
      // Final linearization check: protocol/session changes that happened while
      // bytes and audit were being recomputed must win before runner dispatch.
      const latestSession = await options.sessionStore.get(input.sessionId);
      requireActive(latestSession);
      const latestProtocol = requireApprovedProtocol(latestSession);
      if (
        latestProtocol.version !== protocol.version
        || latestProtocol.content_sha256 !== protocol.content_sha256
        || latestProtocol.approval_id !== protocol.approval_id
        || !(latestSession.analysis_plan_approvals ?? []).some((candidate) => candidate.plan_id === approval.plan_id)
      ) {
        throw new ResearchWorkflowError(
          'ResearchApprovalStale',
          '运行复核期间协议或审批状态已变化，执行已拒绝。',
          409,
        );
      }

      const startedAt = now().toISOString();
      try {
        const rawResult = await options.runner.run(descriptor, { ...args, dataset_id: summary.dataset_id }, {
          datasetBytes: bytes,
          datasetSummary: summary,
        });
        const finishedAt = now().toISOString();
        const resultRecord = rawResult && typeof rawResult === 'object'
          ? rawResult as Record<string, unknown>
          : { payload: rawResult };
        const analysis = resultRecord.analysis && typeof resultRecord.analysis === 'object'
          ? resultRecord.analysis as Record<string, unknown>
          : {};
        const runId = typeof analysis.run_id === 'string' ? analysis.run_id : randomUUID();
        const result = {
          ...resultRecord,
          analysis: {
            ...analysis,
            run_id: runId,
            plan_id: approval.plan_id,
            research_workflow: {
              protocol_version: protocol.version,
              protocol_approval_id: protocol.approval_id,
              plan_id: approval.plan_id,
              plan_approval_id: approval.approval_id,
              plan_approved_at: approval.approved_at,
              audit_id: approval.audit_id,
              audit_sha256: approval.audit_sha256,
            },
          },
        };
        await options.sessionStore.appendSkillRun(input.sessionId, {
          run_id: runId,
          skill_id: descriptor.skillId,
          args,
          started_at: startedAt,
          finished_at: finishedAt,
          outcome: { Ok: result as never },
        });
        options.snapshotRunRecorder?.register({
          runId,
          sessionId: input.sessionId,
          algorithmId: typeof analysis.algorithm_id === 'string' ? analysis.algorithm_id : descriptor.skillId,
          params: args,
          result,
          datasetSummary: summary,
          researchProtocol: protocol,
          analysisPlanApproval: approval,
          datasetAudit: approvedAudit,
          startedAtUtc: startedAt,
          endedAtUtc: finishedAt,
        });
        return result;
      } catch (error) {
        if (error instanceof ResearchWorkflowError) throw error;
        throw error;
      }
    },
  };
}
