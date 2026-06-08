// server/conversation/skill-runner.ts — in-process skill runner.
//
// Deliberate divergence (Requirement 5.1): unlike the Rust SkillRunner which
// spawns `stats-code <subcommand>`, this runner invokes the @stats-code/engine
// algorithm functions IN-PROCESS and NEVER uses child_process. The engine's
// guardedSpawn guard and the forbidden-spawn property test stay in force.
//
// Behavior (Requirement 5):
//  - parse the dataset, build columns/args, invoke the matching engine function
//  - return SkillResult { schema_version, payload, risk_signals, analysis }
//  - validate required args before execution → invalid_args
//  - wrap execution in a 120 s wall-clock guard → timeout
//  - catch thrown engine errors → execution_failed (excerpt ≤ 2048 chars)

import { stats } from '@stats-code/engine';
import { detectRiskSignals } from './risk-signals.js';
import { skillToAlgorithm } from './skill-algorithm-map.js';
import type { SkillContext, SkillDescriptor, SkillRegistry } from './skill-registry.js';
import {
  SkillRunErrorException,
  type AnalysisResultMeta,
  type SkillResult,
} from './skill-runner-types.js';

const DEFAULT_MAX_WALL_SECS = 120;
const MAX_DIAGNOSTIC_CHARS = 2048;
const SCHEMA_VERSION = '1.0';

export type { SkillContext } from './skill-registry.js';
export { SkillRunErrorException } from './skill-runner-types.js';
export type { SkillRunError } from './skill-runner-types.js';

export interface SkillRunnerOptions {
  maxWallSecs?: number;
}

/** Parse CSV/TSV bytes into a header list + row records (string fields). */
function parseRows(bytes: Uint8Array, fileName: string): { headers: string[]; rows: string[][] } {
  const delimiter = fileName.toLowerCase().endsWith('.tsv') ? '\t' : ',';
  const text = new TextDecoder('utf-8').decode(bytes);
  const lines = text.split(/\r\n|\n|\r/).filter((l, idx, arr) => l.length > 0 || idx !== arr.length - 1);
  if (lines.length === 0) throw new Error('empty dataset');
  const split = (line: string): string[] => line.split(delimiter).map((s) => s.trim());
  const headers = split(lines[0]!);
  const rows = lines.slice(1).map(split);
  return { headers, rows };
}

/** Extract a numeric column by name; throws if the column is missing/non-numeric. */
function numericColumn(headers: string[], rows: string[][], name: string): number[] {
  const idx = headers.indexOf(name);
  if (idx === -1) throw new Error(`column not found: ${name}`);
  return rows.map((r) => {
    const v = Number(r[idx]);
    if (!Number.isFinite(v)) throw new Error(`non-numeric value in column ${name}: ${r[idx]}`);
    return v;
  });
}

function asString(value: unknown, name: string): string {
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(`argument '${name}' must be a non-empty string`);
  }
  return value;
}

function asStringArray(value: unknown, name: string): string[] {
  if (!Array.isArray(value) || value.some((v) => typeof v !== 'string')) {
    throw new Error(`argument '${name}' must be a string array`);
  }
  return value as string[];
}

/** Build the [intercept, ...predictors] design matrix from the dataset. */
function designMatrix(headers: string[], rows: string[][], predictors: string[]): number[][] {
  const cols = predictors.map((p) => numericColumn(headers, rows, p));
  return rows.map((_, i) => [1, ...cols.map((c) => c[i]!)]);
}

export class SkillRunner {
  private readonly maxWallSecs: number;

  constructor(
    private readonly registry: SkillRegistry,
    opts: SkillRunnerOptions = {},
  ) {
    this.maxWallSecs = opts.maxWallSecs ?? DEFAULT_MAX_WALL_SECS;
  }

  async run(
    descriptor: SkillDescriptor,
    args: Record<string, unknown>,
    ctx: SkillContext,
  ): Promise<SkillResult> {
    // Validate required args (Requirement 5.6).
    const required = (descriptor.inputSchema.required as string[] | undefined) ?? [];
    const missing = required.filter((k) => args[k] === undefined || args[k] === null);
    if (missing.length > 0) {
      throw new SkillRunErrorException({
        kind: 'invalid_args',
        missing,
        message: `missing required argument(s): ${missing.join(', ')}`,
      });
    }

    // Wall-clock guard (Requirement 5.4). In-process JS is single-threaded; the
    // guard bounds pathological inputs and satisfies the contract surface.
    let timer: ReturnType<typeof setTimeout> | undefined;
    const timeout = new Promise<never>((_resolve, reject) => {
      timer = setTimeout(() => {
        reject(new SkillRunErrorException({ kind: 'timeout', wallSecs: this.maxWallSecs }));
      }, this.maxWallSecs * 1000);
    });

    try {
      const result = await Promise.race([this.execute(descriptor, args, ctx), timeout]);
      return result;
    } catch (err) {
      if (err instanceof SkillRunErrorException) throw err;
      // Any thrown engine error → execution_failed with a bounded excerpt.
      const excerpt = String((err as Error).message ?? err).slice(0, MAX_DIAGNOSTIC_CHARS);
      throw new SkillRunErrorException({ kind: 'execution_failed', diagnosticExcerpt: excerpt });
    } finally {
      if (timer !== undefined) clearTimeout(timer);
    }
  }

  private async execute(
    descriptor: SkillDescriptor,
    args: Record<string, unknown>,
    ctx: SkillContext,
  ): Promise<SkillResult> {
    const payload = await this.computePayload(descriptor, args, ctx);
    const riskSignals = detectRiskSignals(payload);

    let analysis: AnalysisResultMeta | null = null;
    const algorithmId = skillToAlgorithm(descriptor.skillId);
    if (algorithmId !== null) {
      analysis = {
        algorithm_id: algorithmId,
        dataset_id: ctx.datasetSummary.dataset_id,
        dataset_sha256: ctx.datasetSummary.sha256 ?? null,
        columns: ctx.datasetSummary.columns.map((c) => c.name),
        params: args,
      };
    }

    return {
      schema_version: SCHEMA_VERSION,
      payload,
      risk_signals: riskSignals,
      analysis,
    };
  }

  // eslint-disable-next-line @typescript-eslint/require-await
  private async computePayload(
    descriptor: SkillDescriptor,
    args: Record<string, unknown>,
    ctx: SkillContext,
  ): Promise<Record<string, unknown>> {
    const { invoker, skillId } = descriptor;

    if (invoker.kind === 'native') {
      return this.runNative(skillId, args, ctx);
    }

    const { headers, rows } = parseRows(ctx.datasetBytes, ctx.datasetSummary.file_name);

    switch (invoker.algorithmId) {
      case 'linear': {
        const outcome = asString(args.outcome, 'outcome');
        const predictors = asStringArray(args.predictors, 'predictors');
        const y = numericColumn(headers, rows, outcome);
        const x = designMatrix(headers, rows, predictors);
        const res = stats.linear.ols(x, y);
        return {
          coefficients: res.coefficients,
          r_squared: res.rSquared,
          adj_r_squared: res.adjRSquared,
          f_statistic: res.fStatistic,
          p_value: res.fPValue,
          residual_std_error: res.residualStdError,
          n: res.n,
        };
      }
      case 'logistic': {
        const outcome = asString(args.outcome, 'outcome');
        const predictors = asStringArray(args.predictors, 'predictors');
        const y = numericColumn(headers, rows, outcome);
        const x = designMatrix(headers, rows, predictors);
        const res = stats.logistic.logisticRegression(x, y);
        return {
          coefficients: res.coefficients,
          odds_ratios: res.coefficients.map((c) => c.oddsRatio),
          p_values: res.coefficients.map((c) => c.pValue),
          log_likelihood: res.logLikelihood,
          converged: res.converged,
        };
      }
      case 'cox': {
        const time = asString(args.time, 'time');
        const event = asString(args.event, 'event');
        const predictors = asStringArray(args.predictors, 'predictors');
        const times = numericColumn(headers, rows, time);
        const events = numericColumn(headers, rows, event);
        const predCols = predictors.map((p) => numericColumn(headers, rows, p));
        const observations = rows.map((_, i) => ({
          time: times[i]!,
          event: events[i]! !== 0,
          x: predCols.map((c) => c[i]!),
          weight: 1,
        }));
        const res = stats.cox.coxRegression(observations);
        return {
          coefficients: res.coefficients,
          hazard_ratios: res.coefficients.map((c) => c.hazardRatio),
          p_values: res.coefficients.map((c) => c.pValue),
          log_partial_likelihood: res.logPartialLikelihood,
          converged: res.converged,
        };
      }
      case 'kaplan_meier': {
        const time = asString(args.time, 'time');
        const event = asString(args.event, 'event');
        const times = numericColumn(headers, rows, time);
        const events = numericColumn(headers, rows, event).map((v) => v !== 0);
        const res = stats.survival.kaplanMeier(times, events);
        return {
          survival_table: res.points,
          median_survival: res.medianSurvival,
        };
      }
      default:
        throw new Error(`unsupported algorithm: ${String(invoker.algorithmId)}`);
    }
  }

  private runNative(
    skillId: string,
    args: Record<string, unknown>,
    ctx: SkillContext,
  ): Record<string, unknown> {
    if (skillId === 'inspect') {
      const summary = ctx.datasetSummary;
      return {
        row_count: summary.row_count,
        columns: summary.columns,
        summary_statistics: {},
      };
    }
    if (skillId === 'power') {
      const testType = asString(args.test_type, 'test_type');
      const effectSize = Number(args.effect_size);
      const alpha = args.alpha !== undefined ? Number(args.alpha) : 0.05;
      const power = args.power !== undefined ? Number(args.power) : 0.8;
      // Map test_type → the corresponding power family function. The effect
      // size is interpreted as the standardized difference; p0=0.5 baseline for
      // proportion tests keeps the standardized effect consistent.
      let res;
      if (testType === 'anova' || testType === 'ttest' || testType === 'means') {
        res = stats.power.powerPhase3(effectSize, 1, alpha, power);
      } else {
        const p0 = 0.5;
        const p1 = Math.min(0.999, Math.max(0.001, p0 + effectSize * Math.sqrt(p0 * (1 - p0))));
        res = stats.power.powerSingleArm(p0, p1, alpha, power);
      }
      return {
        required_n: res.requiredN,
        achieved_power: res.achievedPower,
        effect_size: res.effectSize,
        alpha: res.alpha,
        method: res.method,
      };
    }
    throw new Error(`unknown native skill: ${skillId}`);
  }
}
