import { randomUUID } from 'node:crypto';
import type {
  DatasetAudit,
  DatasetAuditFinding,
  DatasetAuditRoles,
} from '../state.js';
import { parseDelimitedTable } from './delimited-table.js';
import { runSpecSha256, sha256Canonical } from './research-integrity.js';
import {
  isSensitiveFieldName,
  looksLikeDirectIdentifier,
  normalizeFieldName,
} from './sensitive-data.js';

const MAX_SAMPLE_ROWS = 5;

export interface AuditDatasetInput {
  datasetId: string;
  datasetSha256: string;
  bytes: Uint8Array;
  fileName: string;
  protocolVersion: number;
  skillId: string;
  args: Record<string, unknown>;
  roles?: unknown;
  now?: () => Date;
}

function normalizedName(value: string): string {
  return normalizeFieldName(value);
}

function findHeader(headers: string[], candidates: readonly string[]): string | undefined {
  const byNormalized = new Map(headers.map((header) => [normalizedName(header), header]));
  for (const candidate of candidates) {
    const found = byNormalized.get(candidate);
    if (found) return found;
  }
  return undefined;
}

function findSemanticHeader(
  headers: string[],
  candidates: readonly string[],
  patterns: readonly RegExp[] = [],
): string | undefined {
  const exact = findHeader(headers, candidates);
  if (exact) return exact;
  return headers.find((header) => patterns.some((pattern) => pattern.test(normalizedName(header))));
}

function explicitString(value: unknown): string | undefined {
  return typeof value === 'string' && value.trim().length > 0 ? value.trim() : undefined;
}

function explicitStringArray(value: unknown): string[] | undefined {
  if (!Array.isArray(value)) return undefined;
  const values = value.filter((item): item is string => typeof item === 'string' && item.trim().length > 0);
  return values.length > 0 ? values.map((item) => item.trim()) : undefined;
}

function resolveRoles(
  headers: string[],
  skillId: string,
  args: Record<string, unknown>,
): DatasetAuditRoles {
  const eventFromArgs = skillId === 'model_logistic'
    ? explicitString(args.outcome)
    : explicitString(args.event);
  const personTimeFromArgs = skillId === 'model_cox' || skillId === 'survival_km'
    ? explicitString(args.time)
    : undefined;
  const primaryKeyHeader = findSemanticHeader(
    headers,
    ['participant_id', 'subject_id', 'patient_id', 'person_id', 'record_id', 'study_id', 'case_id'],
    [/^(participant|subject|patient|person|record|study|case)_(id|key)$/],
  );
  const primaryKey = (primaryKeyHeader
      ? [primaryKeyHeader]
      : undefined);
  const timeZero = findSemanticHeader(
    headers,
    ['time_zero', 'start_date', 'start_dt', 'baseline_date', 'baseline_dt', 'entry_date', 'entry_dt', 'index_date', 'index_dt', 'enrollment_date', 'enrollment_dt'],
    [/^(time_zero|start|baseline|entry|index|enrollment)_(date|dt|time)$/],
  );
  const exposureTime = findSemanticHeader(
    headers,
    ['exposure_date', 'exposure_dt', 'treatment_date', 'treatment_dt', 'exposure_time'],
    [/^(exposure|treatment|therapy|intervention)_(date|dt|time)$/],
  );
  const followUpEnd = findSemanticHeader(
    headers,
    ['follow_up_end', 'followup_end', 'end_date', 'end_dt', 'exit_date', 'exit_dt', 'outcome_date', 'outcome_dt', 'death_date', 'death_dt'],
    [/^(follow_?up_end|end|exit|outcome|death)_(date|dt|time)$/],
  );
  const personTime = personTimeFromArgs ?? findSemanticHeader(
    headers,
    ['person_time', 'person_years', 'person_months', 'fu_pt', 'follow_up_time', 'followup_time', 'time_at_risk'],
  );
  // Only unambiguous sampling/analysis-weight names auto-bind (D12): a bare
  // `weight`/`wt` column is far more often an outcome or covariate (birth
  // weight, plant weight) than a survey weight, and the blocker it used to
  // trigger could not be argued with. Declaring roles.weight still binds.
  const weight = findSemanticHeader(
    headers,
    ['survey_weight', 'sampling_weight', 'sample_weight', 'propensity_weight', 'analysis_weight', 'survey_wt', 'sampling_wt', 'sample_wt', 'iptw', 'ipcw'],
    [/^(survey|sampling|sample|analysis|propensity)_(weight|weights|wt)$/],
  );
  const psu = findSemanticHeader(headers, ['psu', 'psu_id', 'primary_sampling_unit', 'primary_sampling_unit_id']);
  const cluster = findSemanticHeader(
    headers,
    ['cluster_id', 'site_id', 'center_id', 'centre_id', 'clinic_id', 'hospital_id', 'facility_id'],
    [/^(cluster|site|center|centre|clinic|hospital|facility)_(id|code)$/],
  );
  const pairId = findSemanticHeader(headers, ['pair_id', 'matched_pair_id', 'match_id', 'matched_id', 'stratum_id']);
  const repeatIndex = findSemanticHeader(
    headers,
    ['repeat_index', 'visit', 'visit_number', 'visit_no', 'timepoint', 'wave', 'round', 'occasion'],
    [/^(repeat|visit|wave|round|occasion|timepoint)_(index|number|no|id)$/],
  );

  return {
    ...(primaryKey ? { primary_key: primaryKey } : {}),
    ...(timeZero ? { time_zero: timeZero } : {}),
    ...(exposureTime ? { exposure_time: exposureTime } : {}),
    ...(followUpEnd ? { follow_up_end: followUpEnd } : {}),
    ...(eventFromArgs ? { event: eventFromArgs } : {}),
    ...(personTime ? { person_time: personTime } : {}),
    ...(weight ? { weight } : {}),
    ...(psu ? { psu } : {}),
    ...(cluster ? { cluster } : {}),
    ...(pairId ? { pair_id: pairId } : {}),
    ...(repeatIndex ? { repeat_index: repeatIndex } : {}),
  };
}

function rowNumbers(rows: number[]): number[] {
  return [...new Set(rows)].slice(0, MAX_SAMPLE_ROWS);
}

function makeFinding(
  code: DatasetAuditFinding['code'],
  severity: DatasetAuditFinding['severity'],
  columns: string[],
  rows: number[],
  message: string,
): DatasetAuditFinding {
  return {
    code,
    severity,
    columns,
    affected_rows: new Set(rows).size,
    sample_row_numbers: rowNumbers(rows),
    message,
  };
}

const ROLE_KEYS = [
  'primary_key',
  'time_zero',
  'exposure_time',
  'follow_up_end',
  'event',
  'person_time',
  'weight',
  'psu',
  'cluster',
  'pair_id',
  'repeat_index',
] as const;

function roleValueColumns(value: unknown): string[] {
  return explicitStringArray(value) ?? (explicitString(value) ? [explicitString(value)!] : []);
}

/**
 * Fold client-declared roles into the server inference (D11/D12 redesign):
 * a declared role whose columns exist is ADOPTED — it becomes the binding of
 * record (returned in `roles`, therefore hashed into audit_sha256) and is
 * validated by the data gates below. Declaring a nonexistent column stays a
 * blocker. The former blanket AUDIT_ROLE_OVERRIDE_REJECTED (any mismatch with
 * the inference) is removed: it made audit_roles useless — every value either
 * matched the inference (no-op) or dead-locked the dataset. Tamper resistance
 * now rests on the data gates (primary-key checks run on the union of adopted
 * and inferred keys) plus the audit-hash chain the approval binds.
 */
function applyRequestedRoles(
  headers: string[],
  inferred: DatasetAuditRoles,
  requested: unknown,
): { roles: DatasetAuditRoles; findings: DatasetAuditFinding[] } {
  if (!requested || typeof requested !== 'object' || Array.isArray(requested)) {
    return { roles: inferred, findings: [] };
  }
  const raw = requested as Record<string, unknown>;
  const findings: DatasetAuditFinding[] = [];
  const roles: DatasetAuditRoles = { ...inferred };
  for (const key of ROLE_KEYS) {
    if (!(key in raw)) continue;
    const requestedColumns = roleValueColumns(raw[key]);
    if (requestedColumns.length === 0) continue;
    const missingColumns = requestedColumns.filter((column) => !headers.includes(column));
    if (missingColumns.length > 0) {
      findings.push(makeFinding(
        'AUDIT_ROLE_COLUMN_MISSING',
        'blocker',
        missingColumns,
        [],
        `客户端提交的 ${key} 角色包含不存在的列；该角色声明未被采纳。`,
      ));
      continue;
    }
    if (key === 'primary_key') {
      roles.primary_key = requestedColumns;
    } else {
      (roles as Record<string, string | string[] | undefined>)[key] = requestedColumns[0];
    }
  }
  return { roles, findings };
}

function analysisColumns(args: Record<string, unknown>): string[] {
  const scalarKeys = ['outcome', 'time', 'event', 'group', 'testVar', 'x', 'y'] as const;
  const arrayKeys = ['predictors', 'continuous', 'categorical'] as const;
  const columns: string[] = [];
  for (const key of scalarKeys) {
    const value = explicitString(args[key]);
    if (value) columns.push(value);
  }
  for (const key of arrayKeys) {
    columns.push(...(explicitStringArray(args[key]) ?? []));
  }
  return [...new Set(columns)];
}

function columnIndex(headers: string[], column: string | undefined): number {
  return column ? headers.indexOf(column) : -1;
}

function parseDate(raw: string): number | null {
  const text = raw.trim();
  if (text.length === 0) return null;
  const match = /^(\d{4})-(\d{2})-(\d{2})(?:T\d{2}:\d{2}(?::\d{2}(?:\.\d{1,3})?)?(?:Z|[+-]\d{2}:\d{2}))?$/.exec(text);
  if (!match) return Number.NaN;
  const year = Number(match[1]);
  const month = Number(match[2]);
  const day = Number(match[3]);
  const calendar = new Date(Date.UTC(year, month - 1, day));
  if (
    calendar.getUTCFullYear() !== year
    || calendar.getUTCMonth() !== month - 1
    || calendar.getUTCDate() !== day
  ) return Number.NaN;
  const value = Date.parse(text);
  return Number.isFinite(value) ? value : Number.NaN;
}

export function auditDataset(input: AuditDatasetInput): DatasetAudit {
  const now = input.now ?? (() => new Date());
  const { headers, rows } = parseDelimitedTable(input.bytes, input.fileName);
  const inferredRoles = resolveRoles(headers, input.skillId, input.args);
  const { roles, findings: roleFindings } = applyRequestedRoles(headers, inferredRoles, input.roles);
  const findings: DatasetAuditFinding[] = [...roleFindings];

  if (rows.length === 0) {
    findings.push(makeFinding('DATASET_NO_ROWS', 'blocker', [], [], '数据集只有表头，没有可审计或分析的数据行。'));
  }
  const emptyHeaders = headers.filter((header) => header.length === 0);
  const duplicateHeaders = headers.filter((header, index) => headers.indexOf(header) !== index);
  if (emptyHeaders.length > 0 || duplicateHeaders.length > 0) {
    findings.push(makeFinding(
      'HEADER_INVALID',
      'blocker',
      [...new Set([...emptyHeaders, ...duplicateHeaders])],
      [],
      '表头存在空列名或重复列名，无法建立唯一字段映射。',
    ));
  }
  const widthMismatchRows = rows.flatMap((row, index) => row.length === headers.length ? [] : [index + 1]);
  if (widthMismatchRows.length > 0) {
    findings.push(makeFinding('ROW_WIDTH_MISMATCH', 'blocker', [], widthMismatchRows, '部分数据行的字段数与表头不一致。'));
  }

  const selectedColumns = analysisColumns(input.args);
  const missingAnalysisColumns = selectedColumns.filter((column) => !headers.includes(column));
  if (missingAnalysisColumns.length > 0) {
    findings.push(makeFinding(
      'ANALYSIS_COLUMN_MISSING',
      'blocker',
      missingAnalysisColumns,
      [],
      '方案引用了数据集中不存在的分析列。',
    ));
  }
  const selectedIndexes = selectedColumns.flatMap((column) => {
    const index = headers.indexOf(column);
    return index >= 0 ? [{ column, index }] : [];
  });
  const blankAnalysisRows: number[] = [];
  const blankAnalysisColumns = new Set<string>();
  rows.forEach((row, rowIndex) => {
    for (const selected of selectedIndexes) {
      if ((row[selected.index] ?? '').trim().length > 0) continue;
      blankAnalysisRows.push(rowIndex + 1);
      blankAnalysisColumns.add(selected.column);
    }
  });
  if (blankAnalysisRows.length > 0) {
    findings.push(makeFinding(
      'ANALYSIS_VALUE_MISSING',
      'blocker',
      [...blankAnalysisColumns],
      blankAnalysisRows,
      '本次方案使用的分析列存在空值；当前引擎不会隐式零填充，需先按协议处理缺失。',
    ));
  }

  // Primary-key gates run on the UNION of the adopted binding and the
  // server-inferred identifier (when they differ): rebinding the key must not
  // hide duplication that is visible under the dataset's own identifier.
  const primaryKeySets: string[][] = [];
  if (roles.primary_key) primaryKeySets.push(roles.primary_key);
  if (
    inferredRoles.primary_key
    && JSON.stringify(inferredRoles.primary_key) !== JSON.stringify(roles.primary_key ?? [])
  ) {
    primaryKeySets.push(inferredRoles.primary_key);
  }
  for (const keyColumns of primaryKeySets) {
    const keyIndexes = keyColumns.map((column) => columnIndex(headers, column));
    const missingRows: number[] = [];
    const duplicateRows: number[] = [];
    const firstByKey = new Map<string, number>();
    rows.forEach((row, index) => {
      const values = keyIndexes.map((column) => column >= 0 ? (row[column] ?? '').trim() : '');
      const rowNumber = index + 1;
      if (values.some((value) => value.length === 0)) missingRows.push(rowNumber);
      const key = JSON.stringify(values);
      const first = firstByKey.get(key);
      if (first !== undefined) duplicateRows.push(first, rowNumber);
      else firstByKey.set(key, rowNumber);
    });
    if (missingRows.length > 0) {
      findings.push(makeFinding('PRIMARY_KEY_MISSING', 'blocker', keyColumns, missingRows, '主键存在空值，无法确认每条观察记录的身份。'));
    }
    if (duplicateRows.length > 0) {
      findings.push(makeFinding('DUPLICATE_PRIMARY_KEY', 'blocker', keyColumns, duplicateRows, '主键不唯一；正式分析已阻断。'));
    }
  }
  if (!roles.primary_key) {
    findings.push(makeFinding('PRIMARY_KEY_UNBOUND', 'blocker', [], [], '未能识别主键；请在审计请求的 audit_roles.primary_key 中指定主键列（将按非空且唯一校验，并计入审计哈希链）。'));
  }

  if (roles.pair_id && roles.repeat_index) {
    const pairIndex = columnIndex(headers, roles.pair_id);
    const repeatIndex = columnIndex(headers, roles.repeat_index);
    const seen = new Map<string, number>();
    const duplicateRows: number[] = [];
    rows.forEach((row, index) => {
      const key = JSON.stringify([(row[pairIndex] ?? '').trim(), (row[repeatIndex] ?? '').trim()]);
      const rowNumber = index + 1;
      const first = seen.get(key);
      if (first !== undefined) duplicateRows.push(first, rowNumber);
      else seen.set(key, rowNumber);
    });
    if (duplicateRows.length > 0) {
      findings.push(makeFinding('DUPLICATE_OBSERVATION_KEY', 'blocker', [roles.pair_id, roles.repeat_index], duplicateRows, '配对/重复测量观察键仍有重复。'));
    }
  }

  const timeColumns = [roles.time_zero, roles.exposure_time, roles.follow_up_end].filter((value): value is string => Boolean(value));
  const invalidTimeRows: number[] = [];
  const invalidOrderRows: number[] = [];
  const immortalTimeRows: number[] = [];
  const startIndex = columnIndex(headers, roles.time_zero);
  const exposureIndex = columnIndex(headers, roles.exposure_time);
  const endIndex = columnIndex(headers, roles.follow_up_end);
  rows.forEach((row, index) => {
    const rowNumber = index + 1;
    const start = startIndex >= 0 ? parseDate(row[startIndex] ?? '') : null;
    const exposure = exposureIndex >= 0 ? parseDate(row[exposureIndex] ?? '') : null;
    const end = endIndex >= 0 ? parseDate(row[endIndex] ?? '') : null;
    if ([start, exposure, end].some((value) => typeof value === 'number' && Number.isNaN(value))) invalidTimeRows.push(rowNumber);
    if (typeof start === 'number' && Number.isFinite(start) && typeof end === 'number' && Number.isFinite(end) && end < start) invalidOrderRows.push(rowNumber);
    if (typeof start === 'number' && Number.isFinite(start) && typeof exposure === 'number' && Number.isFinite(exposure) && exposure > start) immortalTimeRows.push(rowNumber);
  });
  if (invalidTimeRows.length > 0) findings.push(makeFinding('TIME_VALUE_INVALID', 'blocker', timeColumns, invalidTimeRows, '时间角色列包含不可解析的非空值。'));
  if (invalidOrderRows.length > 0) findings.push(makeFinding('TIME_ORDER_INVALID', 'blocker', [roles.time_zero!, roles.follow_up_end!], invalidOrderRows, '随访结束早于时间零点。'));
  if (immortalTimeRows.length > 0) findings.push(makeFinding('IMMORTAL_TIME_RISK', 'blocker', [roles.time_zero!, roles.exposure_time!], immortalTimeRows, '暴露确定时间晚于随访起点，存在不死时间偏倚风险。'));

  if (roles.person_time) {
    const index = columnIndex(headers, roles.person_time);
    const invalidRows = rows.flatMap((row, rowIndex) => {
      const raw = index >= 0 ? (row[index] ?? '').trim() : '';
      if (raw.length === 0) return [];
      const value = Number(raw);
      return !Number.isFinite(value) || value <= 0 ? [rowIndex + 1] : [];
    });
    if (invalidRows.length > 0) findings.push(makeFinding('PERSON_TIME_NONPOSITIVE', 'blocker', [roles.person_time], invalidRows, '人时/随访时长必须为有限正数。'));
  }

  if (roles.event) {
    const index = columnIndex(headers, roles.event);
    const values = new Set<string>();
    const invalidRows: number[] = [];
    rows.forEach((row, rowIndex) => {
      const raw = index >= 0 ? (row[index] ?? '').trim() : '';
      if (raw.length === 0) return;
      values.add(raw);
      if (raw !== '0' && raw !== '1') invalidRows.push(rowIndex + 1);
    });
    if (invalidRows.length > 0) findings.push(makeFinding('EVENT_ENCODING_INVALID', 'blocker', [roles.event], invalidRows, '事件变量必须严格编码为 0/1。'));
    if (invalidRows.length === 0 && values.size < 2) findings.push(makeFinding('EVENT_NO_VARIATION', 'blocker', [roles.event], rows.map((_, indexValue) => indexValue + 1), '事件变量没有 0/1 两个水平，无法估计目标模型。'));
  }

  if (roles.weight || roles.psu) {
    const columns = [roles.weight, roles.psu].filter((value): value is string => Boolean(value));
    findings.push(makeFinding('SURVEY_DESIGN_UNSUPPORTED', 'blocker', columns, rows.map((_, index) => index + 1), '检测到抽样权重/PSU，但当前统计引擎未实现复杂抽样方差估计。'));
  }
  if (roles.cluster) {
    findings.push(makeFinding('CLUSTERING_UNSUPPORTED', 'blocker', [roles.cluster], rows.map((_, index) => index + 1), '检测到聚类角色，但当前方案按独立观察计算。'));
  }
  if (roles.pair_id || roles.repeat_index) {
    const columns = [roles.pair_id, roles.repeat_index].filter((value): value is string => Boolean(value));
    findings.push(makeFinding('PAIRED_REPEATED_UNSUPPORTED', 'blocker', columns, rows.map((_, index) => index + 1), '检测到配对或重复测量设计，当前方法不支持该相关结构。'));
  }

  const sensitiveColumns = new Set(headers.filter(isSensitiveFieldName));
  const sensitiveRows: number[] = [];
  rows.forEach((row, rowIndex) => {
    row.forEach((cell, columnIndexValue) => {
      const value = cell.trim();
      const looksSensitive = looksLikeDirectIdentifier(value);
      if (!looksSensitive) return;
      const column = headers[columnIndexValue];
      if (column) sensitiveColumns.add(column);
      sensitiveRows.push(rowIndex + 1);
    });
  });
  if (sensitiveColumns.size > 0) {
    findings.push(makeFinding(
      'SENSITIVE_FIELD_PRESENT',
      'blocker',
      [...sensitiveColumns],
      sensitiveRows.length > 0 ? sensitiveRows : rows.map((_, index) => index + 1),
      '数据中存在可能的直接标识符；去标识化前禁止正式分析和导出。',
    ));
  }
  const weakCluster = findSemanticHeader(headers, ['site', 'center', 'centre', 'clinic', 'hospital', 'facility']);
  if (weakCluster && !roles.cluster) {
    findings.push(makeFinding('POSSIBLE_CLUSTERING', 'blocker', [weakCluster], rows.map((_, index) => index + 1), '列名提示可能存在中心/站点聚类；确认设计并选择支持该结构的方法前禁止审批。'));
  }

  const status: DatasetAudit['status'] = findings.some((finding) => finding.severity === 'blocker')
    ? 'blocked'
    : findings.length > 0
      ? 'warning'
      : 'passed';
  const specHash = runSpecSha256(input.skillId, input.datasetId, input.args);
  const content = {
    schema_version: '1.0' as const,
    audit_rules_version: '1.2.0' as const,
    dataset_id: input.datasetId,
    dataset_sha256: input.datasetSha256,
    protocol_version: input.protocolVersion,
    skill_id: input.skillId,
    run_spec_sha256: specHash,
    roles,
    status,
    findings,
  };
  return {
    ...content,
    audit_id: randomUUID(),
    audit_sha256: sha256Canonical(content),
    created_at: now().toISOString(),
  };
}
