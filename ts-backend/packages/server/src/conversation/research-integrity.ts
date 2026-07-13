import { createHash } from 'node:crypto';

const RESERVED_WORKFLOW_KEYS = new Set([
  'workflow_approval',
  'plan_id',
  'plan_approval_id',
  'approval_id',
  'approved_at',
  'plan_approved_at',
  'protocol_approved_at',
  'audit_status',
  'audit_sha256',
  'blockers',
]);

function canonicalize(value: unknown): unknown {
  if (value === null || typeof value === 'string' || typeof value === 'boolean') return value;
  if (typeof value === 'number') {
    if (!Number.isFinite(value)) throw new Error('non-finite numbers cannot be canonicalized');
    return Object.is(value, -0) ? 0 : value;
  }
  if (Array.isArray(value)) return value.map(canonicalize);
  if (typeof value === 'object') {
    const record = value as Record<string, unknown>;
    return Object.fromEntries(
      Object.keys(record)
        .filter((key) => record[key] !== undefined)
        .sort()
        .map((key) => [key, canonicalize(record[key])]),
    );
  }
  throw new Error(`unsupported canonical value: ${typeof value}`);
}

export function canonicalJson(value: unknown): string {
  return JSON.stringify(canonicalize(value));
}

export function sha256Canonical(value: unknown): string {
  return createHash('sha256').update(canonicalJson(value), 'utf8').digest('hex');
}

export function sha256Bytes(bytes: Uint8Array): string {
  return createHash('sha256').update(bytes).digest('hex');
}

export function normalizedRunArgs(args: Record<string, unknown>): Record<string, unknown> {
  const { dataset_id: _datasetId, ...rest } = args;
  return canonicalize(rest) as Record<string, unknown>;
}

export function runSpecSha256(
  skillId: string,
  datasetId: string,
  args: Record<string, unknown>,
): string {
  return sha256Canonical({
    skill_id: skillId,
    dataset_id: datasetId,
    args: normalizedRunArgs(args),
  });
}

/** Returns the first reserved server-owned key found anywhere in the args tree. */
export function findReservedWorkflowKey(value: unknown): string | null {
  if (Array.isArray(value)) {
    for (const item of value) {
      const found = findReservedWorkflowKey(item);
      if (found) return found;
    }
    return null;
  }
  if (!value || typeof value !== 'object') return null;
  for (const [key, nested] of Object.entries(value as Record<string, unknown>)) {
    if (RESERVED_WORKFLOW_KEYS.has(key)) return key;
    const found = findReservedWorkflowKey(nested);
    if (found) return found;
  }
  return null;
}

export function protocolContentSha256(fields: Record<string, unknown>): string {
  const {
    status: _status,
    expected_version: _expectedVersion,
    version: _version,
    content_sha256: _contentSha256,
    state_sha256: _stateSha256,
    approval_id: _approvalId,
    approved_at: _approvedAt,
    updated_at: _updatedAt,
    ...content
  } = fields;
  return sha256Canonical(content);
}

/** Detects persisted edits to status/version/approval metadata as well as content. */
export function protocolStateSha256(fields: Record<string, unknown>): string {
  const { state_sha256: _stateSha256, ...state } = fields;
  return sha256Canonical(state);
}
