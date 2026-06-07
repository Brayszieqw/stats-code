// snapshot/workflow_yaml.ts — Workflow YAML byte-level round-trip (task 13.6).
// Transcribed from crates/stats-code/src/snapshot/workflow_yaml.rs.
//
// Round-trip contract (Requirement 12):
//   - parse(bytes) → (Workflow, WorkflowYamlDoc); prettyPrint(_, doc) === bytes
//     (Requirement 12.1, 12.2 — document-side byte round-trip).
//   - prettyPrint(W, undefined) then parse → Workflow structurally equal to W
//     (Requirement 12.3 — model-side round-trip).
//   - prettyPrint(_, undefined) is a fixpoint (Requirement 12.4 canonical form).
//
// The document-side handle stores the original bytes verbatim, so re-emission
// is byte-identical by construction (comments, key order, blank lines, and line
// endings are all preserved because they are never tokenized away). The
// canonical emitter is hand-rolled for full control over the byte sequence.

import { parse as parseYaml } from 'yaml';

// --- model ----------------------------------------------------------------

export interface ArtifactRef {
  path: string;
  sha256: string;
}

export interface ReferenceSoftwareRef {
  name: string;
  version: string;
}

export interface LlmRef {
  provider: string;
  model: string;
}

export interface InputDataset {
  path: string;
  sha256: string;
}

/** Opaque algorithm params (any JSON value). */
export type Params = unknown;

export interface WorkflowStep {
  id: string;
  algorithm: string;
  params: Params;
  inputs: ArtifactRef[];
  outputs: ArtifactRef[];
  referenceSoftware?: ReferenceSoftwareRef;
  llm?: LlmRef;
  startedAtUtc: string;
  endedAtUtc: string;
}

export interface Workflow {
  schemaVersion: number;
  inputDataset: InputDataset;
  steps: WorkflowStep[];
}

/** Document-side handle: holds the verbatim source for byte-identical re-emit. */
export interface WorkflowYamlDoc {
  readonly originalText: string;
}

// --- errors (closed-set rule + field) -------------------------------------

export const RULE = {
  SIZE_CAP_EXCEEDED: 'size_cap_exceeded',
  NON_UTF8: 'non_utf8',
  YAML_SYNTAX_ERROR: 'yaml_syntax_error',
  EMPTY_DOCUMENT: 'empty_document',
  MISSING_FIELD: 'missing_field',
  WRONG_TYPE: 'wrong_type',
  SCHEMA_VERSION_UNSUPPORTED: 'schema_version_unsupported',
  INVALID_SHA256: 'invalid_sha256',
  DUPLICATE_STEP_ID: 'duplicate_step_id',
} as const;

export type RuleViolated = (typeof RULE)[keyof typeof RULE];

export class WorkflowYamlError extends Error {
  constructor(
    public readonly ruleViolated: RuleViolated,
    public readonly field?: string,
  ) {
    super(`workflow.yaml: ${ruleViolated}${field ? ` at field \`${field}\`` : ''}`);
    this.name = 'WorkflowYamlError';
  }
}

const SIZE_CAP_BYTES = 10 * 1024 * 1024;

// --- parse ----------------------------------------------------------------

function isRecord(v: unknown): v is Record<string, unknown> {
  return typeof v === 'object' && v !== null && !Array.isArray(v);
}

function requireString(obj: Record<string, unknown>, key: string, field: string): string {
  if (!(key in obj)) {
    throw new WorkflowYamlError(RULE.MISSING_FIELD, field);
  }
  const v = obj[key];
  if (typeof v !== 'string') {
    throw new WorkflowYamlError(RULE.WRONG_TYPE, field);
  }
  return v;
}

export function isValidSha256(s: string): boolean {
  return s.length === 64 && /^[0-9a-f]{64}$/.test(s);
}

function parseArtifactRef(node: unknown, field: string): ArtifactRef {
  if (!isRecord(node)) {
    throw new WorkflowYamlError(RULE.WRONG_TYPE, 'artifact');
  }
  const path = requireString(node, 'path', 'artifact.path');
  const sha256 = requireString(node, 'sha256', 'artifact.sha256');
  if (!isValidSha256(sha256)) {
    throw new WorkflowYamlError(RULE.INVALID_SHA256, field);
  }
  return { path, sha256 };
}

function parseStep(node: unknown): WorkflowStep {
  if (!isRecord(node)) {
    throw new WorkflowYamlError(RULE.WRONG_TYPE, 'step');
  }
  const id = requireString(node, 'id', 'step.id');
  const algorithm = requireString(node, 'algorithm', 'step.algorithm');

  if (!('params' in node)) {
    throw new WorkflowYamlError(RULE.MISSING_FIELD, 'step.params');
  }
  const params = node['params'];

  if (!('inputs' in node) || !Array.isArray(node['inputs'])) {
    throw new WorkflowYamlError(
      'inputs' in node ? RULE.WRONG_TYPE : RULE.MISSING_FIELD,
      'step.inputs',
    );
  }
  if (!('outputs' in node) || !Array.isArray(node['outputs'])) {
    throw new WorkflowYamlError(
      'outputs' in node ? RULE.WRONG_TYPE : RULE.MISSING_FIELD,
      'step.outputs',
    );
  }

  const inputs = (node['inputs'] as unknown[]).map((n) => parseArtifactRef(n, 'artifact.sha256'));
  const outputs = (node['outputs'] as unknown[]).map((n) => parseArtifactRef(n, 'artifact.sha256'));

  let referenceSoftware: ReferenceSoftwareRef | undefined;
  const rs = node['reference_software'];
  if (rs !== undefined && rs !== null) {
    if (!isRecord(rs)) {
      throw new WorkflowYamlError(RULE.WRONG_TYPE, 'step.reference_software');
    }
    referenceSoftware = {
      name: requireString(rs, 'name', 'reference_software.name'),
      version: requireString(rs, 'version', 'reference_software.version'),
    };
  }

  let llm: LlmRef | undefined;
  const llmNode = node['llm'];
  if (llmNode !== undefined && llmNode !== null) {
    if (!isRecord(llmNode)) {
      throw new WorkflowYamlError(RULE.WRONG_TYPE, 'step.llm');
    }
    llm = {
      provider: requireString(llmNode, 'provider', 'llm.provider'),
      model: requireString(llmNode, 'model', 'llm.model'),
    };
  }

  const startedAtUtc = requireString(node, 'started_at_utc', 'step.started_at_utc');
  const endedAtUtc = requireString(node, 'ended_at_utc', 'step.ended_at_utc');

  return {
    id,
    algorithm,
    params,
    inputs,
    outputs,
    referenceSoftware,
    llm,
    startedAtUtc,
    endedAtUtc,
  };
}

/**
 * Parse a workflow.yaml string into a normalized Workflow plus a doc handle for
 * byte-identical re-serialization. Rejects oversized / malformed / schema-
 * violating input with a structured WorkflowYamlError and no partial model.
 */
export function parse(text: string): { workflow: Workflow; doc: WorkflowYamlDoc } {
  // Gate 1 — size cap (byte length of the UTF-8 encoding).
  if (Buffer.byteLength(text, 'utf8') > SIZE_CAP_BYTES) {
    throw new WorkflowYamlError(RULE.SIZE_CAP_EXCEEDED);
  }

  // Gate 2 — YAML well-formedness.
  let root: unknown;
  try {
    root = parseYaml(text);
  } catch (err) {
    throw new WorkflowYamlError(RULE.YAML_SYNTAX_ERROR, (err as Error).message.slice(0, 0) || undefined);
  }

  if (root === null || root === undefined) {
    throw new WorkflowYamlError(RULE.EMPTY_DOCUMENT, '<root>');
  }
  if (!isRecord(root)) {
    throw new WorkflowYamlError(RULE.WRONG_TYPE, '<root>');
  }

  // schema_version
  if (!('schema_version' in root)) {
    throw new WorkflowYamlError(RULE.MISSING_FIELD, 'schema_version');
  }
  if (typeof root['schema_version'] !== 'number' || !Number.isInteger(root['schema_version'])) {
    throw new WorkflowYamlError(RULE.WRONG_TYPE, 'schema_version');
  }
  if (root['schema_version'] !== 1) {
    throw new WorkflowYamlError(RULE.SCHEMA_VERSION_UNSUPPORTED, 'schema_version');
  }

  // input_dataset
  if (!('input_dataset' in root)) {
    throw new WorkflowYamlError(RULE.MISSING_FIELD, 'input_dataset');
  }
  const inputDatasetNode = root['input_dataset'];
  if (!isRecord(inputDatasetNode)) {
    throw new WorkflowYamlError(RULE.WRONG_TYPE, 'input_dataset');
  }
  const inputDataset: InputDataset = {
    path: requireString(inputDatasetNode, 'path', 'input_dataset.path'),
    sha256: requireString(inputDatasetNode, 'sha256', 'input_dataset.sha256'),
  };
  if (!isValidSha256(inputDataset.sha256)) {
    throw new WorkflowYamlError(RULE.INVALID_SHA256, 'input_dataset.sha256');
  }

  // steps
  if (!('steps' in root)) {
    throw new WorkflowYamlError(RULE.MISSING_FIELD, 'steps');
  }
  if (!Array.isArray(root['steps'])) {
    throw new WorkflowYamlError(RULE.WRONG_TYPE, 'steps');
  }
  const steps: WorkflowStep[] = [];
  const seenIds = new Set<string>();
  for (const stepNode of root['steps'] as unknown[]) {
    const step = parseStep(stepNode);
    if (seenIds.has(step.id)) {
      throw new WorkflowYamlError(RULE.DUPLICATE_STEP_ID, 'step.id');
    }
    seenIds.add(step.id);
    steps.push(step);
  }

  return {
    workflow: { schemaVersion: 1, inputDataset, steps },
    doc: { originalText: text },
  };
}

// --- canonical pretty-printer ---------------------------------------------

const INDENT = '  ';

function pushIndent(parts: string[], level: number): void {
  for (let i = 0; i < level; i += 1) {
    parts.push(INDENT);
  }
}

const YAML_RESERVED = new Set([
  'null', 'Null', 'NULL', '~',
  'true', 'True', 'TRUE', 'false', 'False', 'FALSE',
  'yes', 'Yes', 'YES', 'no', 'No', 'NO',
  'on', 'On', 'ON', 'off', 'Off', 'OFF',
]);

function looksNumeric(s: string): boolean {
  if (/^[+-]?\d+$/.test(s)) return true;
  if (/^[+-]?(\d+\.\d*|\.\d+|\d+)([eE][+-]?\d+)?$/.test(s)) return true;
  if (/^0[xX][0-9a-fA-F]+$/.test(s)) return true;
  if (/^0[oO][0-7]+$/.test(s)) return true;
  return false;
}

function needsQuoting(s: string): boolean {
  if (s.length === 0) return true;
  const first = s[0]!;
  const last = s[s.length - 1]!;
  if (first === ' ' || first === '\t' || last === ' ' || last === '\t') return true;
  if ('-?:,[]{}#&*!|>\'"%@`'.includes(first)) return true;
  for (const ch of s) {
    const code = ch.codePointAt(0)!;
    if (code < 0x20) return true;
    if (ch === ':' || ch === '#' || code === 0x85 || code === 0x2028 || code === 0x2029) return true;
  }
  if (YAML_RESERVED.has(s)) return true;
  if (looksNumeric(s)) return true;
  return false;
}

function pushYamlString(parts: string[], s: string): void {
  if (!needsQuoting(s)) {
    parts.push(s);
    return;
  }
  let out = '"';
  for (const ch of s) {
    const code = ch.codePointAt(0)!;
    if (ch === '\\') out += '\\\\';
    else if (ch === '"') out += '\\"';
    else if (ch === '\n') out += '\\n';
    else if (ch === '\r') out += '\\r';
    else if (ch === '\t') out += '\\t';
    else if (code < 0x20) out += `\\x${code.toString(16).padStart(2, '0')}`;
    else out += ch;
  }
  out += '"';
  parts.push(out);
}

function emitScalarField(parts: string[], level: number, key: string, value: string): void {
  pushIndent(parts, level);
  parts.push(`${key}: `);
  pushYamlString(parts, value);
  parts.push('\n');
}

function emitJsonValueAfterKey(parts: string[], value: unknown, level: number): void {
  if (value === null || value === undefined) {
    parts.push(' null\n');
  } else if (typeof value === 'boolean') {
    parts.push(` ${value ? 'true' : 'false'}\n`);
  } else if (typeof value === 'number') {
    parts.push(` ${String(value)}\n`);
  } else if (typeof value === 'string') {
    parts.push(' ');
    pushYamlString(parts, value);
    parts.push('\n');
  } else if (Array.isArray(value)) {
    if (value.length === 0) {
      parts.push(' []\n');
      return;
    }
    parts.push('\n');
    emitJsonBlockSeq(parts, value, level);
  } else if (isRecord(value)) {
    const keys = Object.keys(value);
    if (keys.length === 0) {
      parts.push(' {}\n');
      return;
    }
    parts.push('\n');
    emitJsonBlockMap(parts, value, level + 1);
  } else {
    parts.push(' null\n');
  }
}

function emitJsonBlockSeq(parts: string[], arr: unknown[], level: number): void {
  for (const item of arr) {
    pushIndent(parts, level);
    if (item === null || item === undefined) {
      parts.push('- null\n');
    } else if (typeof item === 'boolean') {
      parts.push(`- ${item ? 'true' : 'false'}\n`);
    } else if (typeof item === 'number') {
      parts.push(`- ${String(item)}\n`);
    } else if (typeof item === 'string') {
      parts.push('- ');
      pushYamlString(parts, item);
      parts.push('\n');
    } else if (Array.isArray(item)) {
      if (item.length === 0) {
        parts.push('- []\n');
      } else {
        parts.push('-\n');
        emitJsonBlockSeq(parts, item, level + 1);
      }
    } else if (isRecord(item)) {
      const keys = Object.keys(item).sort();
      if (keys.length === 0) {
        parts.push('- {}\n');
      } else {
        const firstKey = keys[0]!;
        parts.push(`- ${firstKey}:`);
        emitJsonValueAfterKey(parts, item[firstKey], level + 1);
        for (const k of keys.slice(1)) {
          pushIndent(parts, level + 1);
          parts.push(`${k}:`);
          emitJsonValueAfterKey(parts, item[k], level + 1);
        }
      }
    }
  }
}

function emitJsonBlockMap(parts: string[], map: Record<string, unknown>, level: number): void {
  for (const k of Object.keys(map).sort()) {
    pushIndent(parts, level);
    parts.push(`${k}:`);
    emitJsonValueAfterKey(parts, map[k], level);
  }
}

function emitArtifactList(parts: string[], level: number, key: string, list: ArtifactRef[]): void {
  pushIndent(parts, level);
  parts.push(key);
  if (list.length === 0) {
    parts.push(': []\n');
    return;
  }
  parts.push(':\n');
  for (const art of list) {
    pushIndent(parts, level);
    parts.push('- path: ');
    pushYamlString(parts, art.path);
    parts.push('\n');
    emitScalarField(parts, level + 1, 'sha256', art.sha256);
  }
}

function emitStep(parts: string[], step: WorkflowStep, level: number): void {
  pushIndent(parts, level);
  parts.push('- id: ');
  pushYamlString(parts, step.id);
  parts.push('\n');

  const inner = level + 1;
  emitScalarField(parts, inner, 'algorithm', step.algorithm);

  pushIndent(parts, inner);
  parts.push('params:');
  emitJsonValueAfterKey(parts, step.params, inner);

  emitArtifactList(parts, inner, 'inputs', step.inputs);
  emitArtifactList(parts, inner, 'outputs', step.outputs);

  if (step.referenceSoftware) {
    pushIndent(parts, inner);
    parts.push('reference_software:\n');
    emitScalarField(parts, inner + 1, 'name', step.referenceSoftware.name);
    emitScalarField(parts, inner + 1, 'version', step.referenceSoftware.version);
  }
  if (step.llm) {
    pushIndent(parts, inner);
    parts.push('llm:\n');
    emitScalarField(parts, inner + 1, 'provider', step.llm.provider);
    emitScalarField(parts, inner + 1, 'model', step.llm.model);
  }

  emitScalarField(parts, inner, 'started_at_utc', step.startedAtUtc);
  emitScalarField(parts, inner, 'ended_at_utc', step.endedAtUtc);
}

/**
 * Serialize a Workflow back to YAML bytes (as a string).
 *
 * - doc provided → document-side byte round-trip: returns the verbatim source
 *   captured at parse time (Requirement 12.1, 12.2).
 * - doc omitted → canonical form: fixed key order, two-space indent, LF
 *   endings, trailing newline; a fixpoint (Requirements 12.3, 12.4).
 */
export function prettyPrint(model: Workflow, doc?: WorkflowYamlDoc): string {
  if (doc) {
    return doc.originalText;
  }

  const parts: string[] = [];
  parts.push(`schema_version: ${model.schemaVersion}\n`);
  parts.push('input_dataset:\n');
  emitScalarField(parts, 1, 'path', model.inputDataset.path);
  emitScalarField(parts, 1, 'sha256', model.inputDataset.sha256);

  if (model.steps.length === 0) {
    parts.push('steps: []\n');
  } else {
    parts.push('steps:\n');
    for (const step of model.steps) {
      emitStep(parts, step, 1);
    }
  }

  return parts.join('');
}
