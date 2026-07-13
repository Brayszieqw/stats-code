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
  if (algorithmId === 'linear' || algorithmId === 'logistic') {
    const outcome = params.outcome ?? columns[0]?.name ?? '';
    const predictors =
      parseNameList(params.predictors) ?? (columns[1] ? [columns[1].name] : []);

    return {
      ...params,
      outcome,
      outcome_quoted: quoteString(outcome),
      predictors_space: predictors.join(' '),
      predictors_r: predictors.join(' + '),
      predictors_quoted: predictors.map(quoteString).join(', '),
    };
  }

  if (algorithmId === 'cox') {
    const time = params.time ?? columns[0]?.name ?? '';
    const event = params.event ?? columns[1]?.name ?? '';
    const predictors = parseNameList(params.predictors) ?? columns.slice(2).map((column) => column.name);
    return {
      ...params,
      time,
      event,
      time_quoted: quoteString(time),
      event_quoted: quoteString(event),
      predictors_r: predictors.join(' + '),
      predictors_space: predictors.join(' '),
      predictors_quoted: predictors.map(quoteString).join(', '),
      cox_spss_method: predictors.length > 0
        ? `/METHOD=ENTER ${predictors.join(' ')}`
        : '/* No predictors selected. */',
    };
  }

  if (algorithmId === 'tableone') {
    const group =
      params.group ?? params.strata ?? params.by ?? columns[1]?.name ?? columns[0]?.name ?? '';
    const continuous =
      parseNameList(params.continuous) ??
      parseNameList(params.vars) ??
      (columns[0] ? [columns[0].name] : []);
    const categorical = parseNameList(params.categorical) ?? [];
    const categoricalSasBlock = categorical.length > 0
      ? `proc freq data=work.data;\n  tables ${categorical.map((name) => `${group}*${name}`).join(' ')} / chisq expected;\n  exact fisher;\nrun;`
      : '/* No categorical variables requested. */';
    const categoricalSpssBlock = categorical.length > 0
      ? categorical.map((name) => [
          'CROSSTABS',
          `  /TABLES=${group} BY ${name}`,
          '  /STATISTICS=CHISQ',
          '  /CELLS=COUNT EXPECTED ROW',
          '  /METHOD=EXACT.',
        ].join('\n')).join('\n')
      : '* No categorical variables requested.';

    return {
      ...params,
      group,
      group_quoted: quoteString(group),
      continuous_space: continuous.join(' '),
      continuous_quoted: continuous.map(quoteString).join(', '),
      categorical_space: categorical.join(' '),
      categorical_quoted: categorical.map(quoteString).join(', '),
      categorical_sas_block: categoricalSasBlock,
      categorical_spss_block: categoricalSpssBlock,
    };
  }

  if (algorithmId === 'kaplan_meier') {
    const time = params.time ?? columns[0]?.name ?? '';
    const event = params.event ?? columns[1]?.name ?? '';
    const group = params.group?.trim() ?? '';
    const pythonBlock = group.length > 0
      ? `for label, frame in data.groupby(${quoteString(group)}):\n    fitter.fit(frame[${quoteString(time)}], event_observed=frame[${quoteString(event)}], label=str(label))\n    print(fitter.survival_function_)\ncomparison = multivariate_logrank_test(data[${quoteString(time)}], data[${quoteString(group)}], data[${quoteString(event)}])\nprint(comparison.summary)`
      : `fitter.fit(durations=data[${quoteString(time)}], event_observed=data[${quoteString(event)}], label="Overall")\nprint(fitter.survival_function_)`;
    return {
      ...params,
      time,
      event,
      group,
      time_quoted: quoteString(time),
      event_quoted: quoteString(event),
      group_quoted: quoteString(group),
      km_r_formula: group.length > 0 ? group : '1',
      km_r_logrank_block: group.length > 0
        ? `comparison <- survival::survdiff(survival::Surv(${time}, ${event}) ~ ${group}, data = data)\nprint(comparison)`
        : '# No group requested; log-rank comparison is not applicable.',
      km_python_block: pythonBlock,
      km_sas_strata_block: group.length > 0
        ? `   strata ${group} / test=logrank;`
        : '   /* No group requested; log-rank comparison is not applicable. */',
      km_spss_block: group.length > 0
        ? `KM ${time} BY ${group}\n  /STATUS=${event}(1)\n  /COMPARE OVERALL.`
        : `KM ${time} /STATUS=${event}(1).`,
    };
  }

  return params;
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
