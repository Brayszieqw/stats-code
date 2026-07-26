// File-backed SessionStore for production/dev launcher history persistence.

import { randomUUID } from 'node:crypto';
import {
  closeSync,
  existsSync,
  fsyncSync,
  mkdirSync,
  openSync,
  readFileSync,
  renameSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { homedir } from 'node:os';
import { dirname, join } from 'node:path';
import {
  StoreError,
  type DatasetSummary,
  type DatasetAudit,
  type AnalysisPlanApproval,
  type Message,
  type Session,
  type SessionSettings,
  type SessionStore,
  type SessionSummary,
  type ResearchProtocol,
  type SkillRun,
} from '../state.js';
import { domain } from '../contract/index.js';
import { sanitizeTitleText } from '../title-text.js';
import {
  protocolContentSha256,
  protocolStateSha256,
  runSpecSha256,
  sha256Canonical,
} from './research-integrity.js';

const APP_DIR = 'stats-code';
const SESSIONS_FILE = 'sessions.json';
const TITLE_MAX_CHARS = 20;
const DEFAULT_TITLE = '新对话';

export interface FileSessionStoreOptions {
  filePath?: string;
  now?: () => Date;
  onIntegrityWarning?: (warning: FileSessionIntegrityWarning) => void;
}

export interface FileSessionIntegrityWarning {
  event: 'file_session_integrity_warning';
  action: 'downgraded' | 'discarded';
  record_type: 'research_protocol' | 'dataset_audit' | 'analysis_plan_approval';
  session_id: string;
  reason: string;
}

export function defaultSessionStorePath(): string {
  if (process.platform === 'win32') {
    const appData = process.env.APPDATA ?? join(homedir(), 'AppData', 'Roaming');
    return join(appData, APP_DIR, SESSIONS_FILE);
  }
  const xdg = process.env.XDG_CONFIG_HOME;
  const base = xdg && xdg.length > 0 ? xdg : join(homedir(), '.config');
  return join(base, APP_DIR, SESSIONS_FILE);
}

function deriveTitle(session: Session): string {
  for (const msg of session.messages) {
    if ('User' in msg) {
      const content = msg.User.content;
      if ('Text' in content) {
        const text = sanitizeTitleText(content.Text);
        if (text.length > 0) {
          return [...text].slice(0, TITLE_MAX_CHARS).join('');
        }
      }
    }
  }
  return DEFAULT_TITLE;
}

function cloneSession(session: Session): Session {
  return JSON.parse(JSON.stringify(session)) as Session;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

function isSession(value: unknown): value is Session {
  if (!isRecord(value)) return false;
  return (
    typeof value.id === 'string' &&
    (value.status === 'Active' || value.status === 'Archived') &&
    typeof value.created_at === 'string' &&
    typeof value.last_active_at === 'string' &&
    isRecord(value.settings) &&
    typeof value.settings.decision_assistant === 'boolean' &&
    Array.isArray(value.messages) &&
    Array.isArray(value.datasets) &&
    Array.isArray(value.skill_runs) &&
    typeof value.uploaded_bytes === 'number'
  );
}

function parseSessions(
  raw: string,
  onIntegrityWarning: (warning: FileSessionIntegrityWarning) => void,
): Session[] {
  const parsed = JSON.parse(raw) as unknown;
  const sessions = Array.isArray(parsed)
    ? parsed
    : isRecord(parsed) && Array.isArray(parsed.sessions)
      ? parsed.sessions
      : [];
  return sessions
    .filter(isSession)
    .map((session) => {
      const integrityWarnings = Array.isArray(session.integrity_warnings)
        ? session.integrity_warnings.flatMap((value) => {
          const parsedWarning = domain.sessionIntegrityWarning.safeParse(value);
          return parsedWarning.success && parsedWarning.data.session_id === session.id
            ? [parsedWarning.data]
            : [];
        })
        : [];
      const warn = (
        action: FileSessionIntegrityWarning['action'],
        recordType: FileSessionIntegrityWarning['record_type'],
        reason: string,
      ): void => {
        const warning: FileSessionIntegrityWarning = {
          event: 'file_session_integrity_warning',
          action,
          record_type: recordType,
          session_id: session.id,
          reason,
        };
        integrityWarnings.push(warning);
        onIntegrityWarning(warning);
      };
      const rawProtocol: Record<string, unknown> = isRecord(session.research_protocol)
        ? session.research_protocol
        : {};
      const parsedProtocol = domain.researchProtocol.safeParse(session.research_protocol);
      const {
        version: _legacyVersion,
        content_sha256: _legacyContentSha,
        state_sha256: _legacyStateSha,
        approval_id: _legacyApprovalId,
        approved_at: _legacyApprovedAt,
        updated_at: _legacyUpdatedAt,
        expected_version: _legacyExpectedVersion,
        ...legacyInput
      } = rawProtocol;
      const legacyProtocol = parsedProtocol.success
        ? null
        : domain.researchProtocolInput.safeParse(legacyInput);
      const migratedProtocol = legacyProtocol?.success
        ? (() => {
          const { expected_version: _expectedVersion, ...fields } = legacyProtocol.data;
          // Legacy "Approved" records predate server approval ids/versioning;
          // never mint trust for them or reuse their client-origin timestamp.
          const migratedStatus = fields.status === 'Approved' ? 'Draft' : fields.status;
          if (fields.status === 'Approved') {
            warn('downgraded', 'research_protocol', 'legacy_approved_without_server_trust');
          }
          const migratedState = {
            ...fields,
            status: migratedStatus,
            version: typeof rawProtocol.version === 'number'
              && Number.isInteger(rawProtocol.version)
              && rawProtocol.version > 0
              ? rawProtocol.version
              : 1,
            content_sha256: protocolContentSha256(fields),
            approval_id: null,
            approved_at: null,
            updated_at: typeof rawProtocol.updated_at === 'string'
              ? rawProtocol.updated_at
              : session.last_active_at,
          };
          const migrated = domain.researchProtocol.safeParse({
            ...migratedState,
            state_sha256: protocolStateSha256(migratedState),
          });
          return migrated.success ? migrated.data : null;
        })()
        : null;
      const trustedProtocol = parsedProtocol.success
        ? (() => {
          const recomputedHash = protocolContentSha256(parsedProtocol.data);
          const recomputedStateHash = protocolStateSha256(parsedProtocol.data);
          if (
            recomputedHash === parsedProtocol.data.content_sha256
            && recomputedStateHash === parsedProtocol.data.state_sha256
            && (parsedProtocol.data.status !== 'Approved' || parsedProtocol.data.approval_id !== null)
          ) return parsedProtocol.data;
          const reason = recomputedHash !== parsedProtocol.data.content_sha256
            ? 'content_hash_mismatch'
            : recomputedStateHash !== parsedProtocol.data.state_sha256
              ? 'state_hash_mismatch'
              : 'approved_protocol_missing_approval_id';
          warn('downgraded', 'research_protocol', reason);
          const downgraded = {
            ...parsedProtocol.data,
            status: 'Draft' as const,
            content_sha256: recomputedHash,
            approval_id: null,
            approved_at: null,
          };
          return {
            ...downgraded,
            state_sha256: protocolStateSha256(downgraded),
          };
        })()
        : migratedProtocol;
      const parsedAudits = Array.isArray(session.dataset_audits)
        ? session.dataset_audits.flatMap((value) => {
          const parsedAudit = domain.datasetAudit.safeParse(value);
          if (!parsedAudit.success) {
            warn('discarded', 'dataset_audit', 'schema_invalid');
            return [];
          }
          const {
            audit_id: _auditId,
            audit_sha256: _auditSha256,
            created_at: _createdAt,
            ...content
          } = parsedAudit.data;
          if (sha256Canonical(content) === parsedAudit.data.audit_sha256) return [parsedAudit.data];
          warn('discarded', 'dataset_audit', 'content_hash_mismatch');
          return [];
        })
        : [];
      const parsedApprovals = Array.isArray(session.analysis_plan_approvals)
        ? session.analysis_plan_approvals.flatMap((value) => {
          const parsedApproval = domain.analysisPlanApproval.safeParse(value);
          if (!parsedApproval.success) {
            warn('discarded', 'analysis_plan_approval', 'schema_invalid');
            return [];
          }
          if (!trustedProtocol || trustedProtocol.status !== 'Approved') {
            warn('discarded', 'analysis_plan_approval', 'protocol_not_approved');
            return [];
          }
          const approval = parsedApproval.data;
          const dataset = session.datasets.find((candidate) => candidate.dataset_id === approval.dataset_id);
          const audit = parsedAudits.find((candidate) => candidate.audit_id === approval.audit_id);
          const valid = approval.protocol_version === trustedProtocol.version
            && approval.protocol_sha256 === trustedProtocol.content_sha256
            && approval.protocol_approval_id === trustedProtocol.approval_id
            && dataset?.sha256?.toLowerCase() === approval.dataset_sha256
            && audit?.audit_sha256 === approval.audit_sha256
            && approval.run_spec_sha256 === runSpecSha256(
              approval.skill_id,
              approval.dataset_id,
              approval.args,
            );
          if (valid) return [approval];
          warn('discarded', 'analysis_plan_approval', 'binding_mismatch');
          return [];
        })
        : [];
      return cloneSession({
        ...session,
        research_protocol: trustedProtocol,
        dataset_audits: parsedAudits,
        analysis_plan_approvals: parsedApprovals,
        integrity_warnings: integrityWarnings,
      });
    });
}

export function createFileSessionStore(opts: FileSessionStoreOptions = {}): SessionStore {
  const filePath = opts.filePath ?? defaultSessionStorePath();
  const now = opts.now ?? (() => new Date());
  const warningSink = opts.onIntegrityWarning
    ?? ((warning: FileSessionIntegrityWarning) => { console.warn(JSON.stringify(warning)); });
  const onIntegrityWarning = (warning: FileSessionIntegrityWarning): void => {
    try {
      warningSink(warning);
    } catch {
      // Observability must never turn a recoverable integrity downgrade into data loss.
    }
  };
  let loaded = false;
  const sessions = new Map<string, Session>();

  function backupCorrupt(): void {
    const stamp = now().toISOString().replace(/[:.]/g, '-');
    try {
      renameSync(filePath, `${filePath}.corrupt-${stamp}`);
    } catch {
      // Best effort only; startup should still continue with an empty store.
    }
  }

  function writeDurable(targetPath: string, payload: string): void {
    const fd = openSync(targetPath, 'wx', 0o600);
    try {
      writeFileSync(fd, payload, { encoding: 'utf8' });
      fsyncSync(fd);
    } finally {
      closeSync(fd);
    }
  }

  function quarantine(raw: string): void {
    const stamp = now().toISOString().replace(/[:.]/g, '-');
    const basePath = `${filePath}.quarantine-${stamp}`;
    const quarantinePath = existsSync(basePath) ? `${basePath}-${randomUUID()}` : basePath;
    try {
      writeDurable(quarantinePath, raw);
    } catch {
      try {
        rmSync(quarantinePath, { force: true });
      } catch {
        // Keep the primary load path available even when quarantine cleanup fails.
      }
      // Best effort only; fail-closed loading and structured warnings still apply.
    }
  }

  function ensureLoaded(): void {
    if (loaded) return;
    loaded = true;
    if (!existsSync(filePath)) return;
    try {
      const raw = readFileSync(filePath, 'utf8');
      let integrityWarningObserved = false;
      const parsedSessions = parseSessions(raw, (warning) => {
        integrityWarningObserved = true;
        onIntegrityWarning(warning);
      });
      if (integrityWarningObserved) quarantine(raw);
      for (const session of parsedSessions) {
        sessions.set(session.id, session);
      }
    } catch {
      backupCorrupt();
      sessions.clear();
    }
  }

  function save(): void {
    mkdirSync(dirname(filePath), { recursive: true });
    const tmpPath = `${filePath}.tmp-${process.pid}-${now().getTime()}-${randomUUID()}`;
    const payload = JSON.stringify({ version: 1, sessions: [...sessions.values()] }, null, 2);
    try {
      writeDurable(tmpPath, payload);
      renameSync(tmpPath, filePath);
    } catch (error) {
      try {
        rmSync(tmpPath, { force: true });
      } catch {
        // Keep the original write error; temp cleanup is secondary.
      }
      throw error;
    }
  }

  function mustGet(id: string): Session {
    ensureLoaded();
    const session = sessions.get(id);
    if (!session) {
      throw new StoreError('not_found', 'session not found');
    }
    return session;
  }

  return {
    async create(): Promise<Session> {
      ensureLoaded();
      const timestamp = now().toISOString();

      // Reuse the newest empty Active shell and purge other empty shells so
      // repeated「新对话」does not bloat sessions.json with "新对话 / 0" rows.
      const empties = [...sessions.values()]
        .filter(
          (s) =>
            s.status === 'Active' &&
            s.messages.length === 0 &&
            s.datasets.length === 0 &&
            s.skill_runs.length === 0 &&
            s.research_protocol == null,
        )
        .sort((a, b) => b.last_active_at.localeCompare(a.last_active_at));

      if (empties.length > 0) {
        const keep = empties[0]!;
        for (const extra of empties.slice(1)) {
          sessions.delete(extra.id);
        }
        keep.last_active_at = timestamp;
        save();
        return cloneSession(keep);
      }

      const session: Session = {
        id: randomUUID(),
        status: 'Active',
        created_at: timestamp,
        last_active_at: timestamp,
        settings: { decision_assistant: true },
        research_protocol: null,
        dataset_audits: [],
        analysis_plan_approvals: [],
        messages: [],
        datasets: [],
        skill_runs: [],
        uploaded_bytes: 0,
      };
      sessions.set(session.id, session);
      save();
      return cloneSession(session);
    },

    async get(id: string): Promise<Session> {
      return cloneSession(mustGet(id));
    },

    async updateSettings(id: string, settings: SessionSettings): Promise<void> {
      const session = mustGet(id);
      session.settings = settings;
      session.last_active_at = now().toISOString();
      save();
    },

    async updateResearchProtocol(id: string, protocol: ResearchProtocol, expectedVersion?: number): Promise<boolean> {
      const session = mustGet(id);
      if (session.research_protocol?.version !== expectedVersion) return false;
      session.research_protocol = JSON.parse(JSON.stringify(protocol)) as ResearchProtocol;
      session.last_active_at = now().toISOString();
      save();
      return true;
    },

    async appendDatasetAudit(id: string, audit: DatasetAudit): Promise<void> {
      const session = mustGet(id);
      if (session.status !== 'Active') throw new StoreError('archived', 'session is archived');
      (session.dataset_audits ??= []).push(JSON.parse(JSON.stringify(audit)) as DatasetAudit);
      session.last_active_at = now().toISOString();
      save();
    },

    async appendAnalysisPlanApproval(id: string, approval: AnalysisPlanApproval): Promise<boolean> {
      const session = mustGet(id);
      const protocol = session.research_protocol;
      if (
        session.status !== 'Active'
        || protocol?.status !== 'Approved'
        || protocol.version !== approval.protocol_version
        || protocol.content_sha256 !== approval.protocol_sha256
        || protocol.approval_id !== approval.protocol_approval_id
      ) return false;
      (session.analysis_plan_approvals ??= []).push(JSON.parse(JSON.stringify(approval)) as AnalysisPlanApproval);
      session.last_active_at = now().toISOString();
      save();
      return true;
    },

    async appendSkillRun(id: string, run: SkillRun): Promise<void> {
      const session = mustGet(id);
      if (session.status !== 'Active') throw new StoreError('archived', 'session is archived');
      session.skill_runs.push(JSON.parse(JSON.stringify(run)) as SkillRun);
      session.last_active_at = now().toISOString();
      save();
    },

    async appendMessages(id: string, messages: Message[]): Promise<void> {
      const session = mustGet(id);
      session.messages.push(...(JSON.parse(JSON.stringify(messages)) as Message[]));
      session.last_active_at = now().toISOString();
      save();
    },

    async appendDataset(id: string, dataset: DatasetSummary): Promise<void> {
      const session = mustGet(id);
      session.datasets.push(JSON.parse(JSON.stringify(dataset)) as DatasetSummary);
      session.last_active_at = now().toISOString();
      save();
    },

    async deleteSession(id: string): Promise<void> {
      ensureLoaded();
      if (!sessions.delete(id)) {
        throw new StoreError('not_found', 'session not found');
      }
      save();
    },

    async list(): Promise<SessionSummary[]> {
      ensureLoaded();
      const summaries: SessionSummary[] = [...sessions.values()].map((session) => ({
        id: session.id,
        status: session.status,
        created_at: session.created_at,
        last_active_at: session.last_active_at,
        message_count: session.messages.length,
        title: deriveTitle(session),
        dataset_count: session.datasets.length,
      }));
      summaries.sort((a, b) => b.last_active_at.localeCompare(a.last_active_at));
      return summaries;
    },
  };
}
