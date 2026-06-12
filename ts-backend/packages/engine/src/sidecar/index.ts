// sidecar/ — deterministic equivalent-code snippet generator (Phase 6, task 13.3).
// Transcribed from crates/stats-code/src/sidecar/mod.rs.
//
// Pipeline: coverage lookup → (none → Uncovered sentinel) | (render → redact →
// header+body). Runs inside guardedSpawn so any forbidden runtime spawn aborts.
// Pure: output depends only on inputs (no clock/env/random/host).

import { getLoadedMatrix, type ReferenceSoftware, type CoverageState } from '../coverage/index.js';
import { redactPure, redactionPolicy } from '../redact.js';
import { guardedSpawn } from '../spawn_policy.js';
import { renderPure, formatHeader, type Column, type RenderParams } from './render.js';
import { SIDECAR_TEMPLATES } from './templates-data.js';

export type { Column, ColumnDtype, RenderParams } from './render.js';
export { renderPure, formatHeader, RenderError } from './render.js';

export interface Snippet {
  language: ReferenceSoftware;
  body: string;
  copyable: boolean;
}

export type SidecarSnippet =
  | {
      kind: 'snippet';
      software: ReferenceSoftware;
      algorithmId: string;
      text: string;
      sha256OfDataset: string;
      releaseVersion: string;
    }
  | {
      kind: 'uncovered';
      algorithmId: string;
      software: ReferenceSoftware;
      coverageValue: 'none';
    };

export class GenerateError extends Error {
  constructor(
    public readonly kind: 'unknown_algorithm' | 'missing_template',
    message: string,
  ) {
    super(message);
    this.name = 'GenerateError';
  }
}

function templateKey(algorithmId: string, software: ReferenceSoftware): string {
  return `${algorithmId}\u0000${software}`;
}

export interface GenerateOptions {
  apiKeys?: readonly string[];
  workingDirectory?: string;
}

function parseNameList(value: string | undefined): string[] | undefined {
  if (value === undefined || value.trim() === '') {
    return undefined;
  }
  try {
    const parsed = JSON.parse(value) as unknown;
    if (Array.isArray(parsed)) {
      const names = parsed.filter((item): item is string => typeof item === 'string' && item.trim() !== '');
      return names.length > 0 ? names : undefined;
    }
  } catch {
    // Fall back to comma-separated strings below.
  }
  const names = value
    .split(',')
    .map((part) => part.trim().replace(/^["']|["']$/g, ''))
    .filter((part) => part.length > 0);
  return names.length > 0 ? names : undefined;
}

function quoteString(value: string): string {
  return `"${value.replace(/\\/g, '\\\\').replace(/"/g, '\\"')}"`;
}

function normalizeRenderParams(
  algorithmId: string,
  params: RenderParams,
  columns: readonly Column[],
): RenderParams {
  if (algorithmId !== 'tableone') {
    return params;
  }

  const group =
    params.group ?? params.strata ?? params.by ?? columns[1]?.name ?? columns[0]?.name ?? '';
  const continuous =
    parseNameList(params.continuous) ??
    parseNameList(params.vars) ??
    (columns[0] ? [columns[0].name] : []);

  return {
    ...params,
    group,
    group_quoted: quoteString(group),
    continuous_space: continuous.join(' '),
    continuous_quoted: continuous.map(quoteString).join(', '),
  };
}

/**
 * Generate a sidecar snippet for one (algorithm, software) cell.
 * - `none` coverage → structured Uncovered sentinel (no body, copy disabled).
 * - live/recorded/sidecar_only → rendered, redacted, header-prefixed snippet.
 */
export function generateSnippet(
  algorithmId: string,
  software: ReferenceSoftware,
  params: RenderParams,
  columns: readonly Column[],
  datasetSha256: string,
  opts: GenerateOptions = {},
): SidecarSnippet {
  return guardedSpawn(() => {
    const matrix = getLoadedMatrix();
    const entry = matrix.algorithms.find((e) => e.id === algorithmId);
    if (!entry) {
      throw new GenerateError('unknown_algorithm', `unknown algorithm: ${algorithmId}`);
    }
    const coverage: CoverageState = entry.coverage[software];

    if (coverage === 'none') {
      return {
        kind: 'uncovered',
        algorithmId,
        software,
        coverageValue: 'none',
      };
    }

    const template = SIDECAR_TEMPLATES[templateKey(algorithmId, software)];
    if (template === undefined) {
      throw new GenerateError(
        'missing_template',
        `missing template for (${algorithmId}, ${software})`,
      );
    }

    const releaseVersion = matrix.release_version;
    const renderParams = normalizeRenderParams(algorithmId, params, columns);
    const body = renderPure(template, renderParams, columns, datasetSha256, releaseVersion);

    const policy = redactionPolicy({
      secrets: opts.apiKeys ?? [],
      workingDirectory: opts.workingDirectory,
    });
    const redactedBody = redactPure(body, policy);

    const header = formatHeader(columns, datasetSha256, releaseVersion);
    const text = header + redactedBody;

    return {
      kind: 'snippet',
      software,
      algorithmId,
      text,
      sha256OfDataset: datasetSha256,
      releaseVersion,
    };
  });
}
