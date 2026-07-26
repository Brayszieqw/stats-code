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

import { randomUUID } from 'node:crypto';
import { coverage, stats } from '@stats-code/engine';
import { detectRiskSignals } from './risk-signals.js';
import { skillToAlgorithm } from './skill-algorithm-map.js';
import { parseDelimitedTable } from './delimited-table.js';
import type { SkillContext, SkillDescriptor, SkillRegistry } from './skill-registry.js';
import {
  SkillRunErrorException,
  type AnalysisResultMeta,
  type ResultContractEstimate,
  type SkillResult,
  type StandardResultContract,
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

/** Extract a numeric column by name; throws if the column is missing/non-numeric. */
function numericColumn(headers: string[], rows: string[][], name: string): number[] {
  const idx = columnIndex(headers, name);
  return rows.map((r, rowIndex) => {
    const raw = r[idx]?.trim() ?? '';
    if (raw.length === 0) throw new Error(`missing numeric value in column ${name} at data row ${rowIndex + 1}`);
    const v = Number(raw);
    if (!Number.isFinite(v)) throw new Error(`non-numeric value in column ${name} at data row ${rowIndex + 1}`);
    return v;
  });
}

function columnIndex(headers: string[], name: string): number {
  const idx = headers.indexOf(name);
  if (idx === -1) throw new Error(`column not found: ${name}`);
  return idx;
}

function rowIndexes(rows: string[][]): number[] {
  return rows.map((_, i) => i);
}

function nullableNumericColumn(headers: string[], rows: string[][], name: string, indexes: number[]): (number | null)[] {
  const idx = columnIndex(headers, name);
  return indexes.map((rowIndex) => {
    const raw = rows[rowIndex]?.[idx]?.trim() ?? '';
    if (raw.length === 0) return null;
    const value = Number(raw);
    return Number.isFinite(value) ? value : null;
  });
}

function nullableStringColumn(headers: string[], rows: string[][], name: string, indexes: number[]): (string | null)[] {
  const idx = columnIndex(headers, name);
  return indexes.map((rowIndex) => {
    const raw = rows[rowIndex]?.[idx]?.trim() ?? '';
    return raw.length > 0 ? raw : null;
  });
}

function numericValuesByGroup(
  headers: string[],
  rows: string[][],
  group: string,
  testVar: string,
): Map<string, number[]> {
  const groupIdx = columnIndex(headers, group);
  const testIdx = columnIndex(headers, testVar);
  const buckets = new Map<string, number[]>();
  for (const [rowIndex, row] of rows.entries()) {
    const label = row[groupIdx]?.trim() ?? '';
    if (label.length === 0) continue;
    // Trim before Number(): `Number('')` is 0, so an empty cell used to enter
    // the analysis as a literal zero while `resultCounts()` — which tests
    // `.trim().length > 0` — reported that same row as excluded. The audit
    // metadata and the numbers it describes must come from one rule.
    const raw = row[testIdx]?.trim() ?? '';
    if (raw.length === 0) continue;
    const value = Number(raw);
    // Non-numeric text is an input error, not a missing value: dropping it
    // silently would again disagree with the complete-case count, which reads
    // any non-empty cell as present.
    if (!Number.isFinite(value)) {
      throw new Error(`non-numeric value in column ${testVar} at data row ${rowIndex + 1}`);
    }
    const values = buckets.get(label) ?? [];
    values.push(value);
    buckets.set(label, values);
  }
  return buckets;
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

function asOptionalStringArray(value: unknown, name: string): string[] {
  if (value === undefined || value === null) return [];
  return asStringArray(value, name);
}

/**
 * 分类预测变量的哑变量（one-hot，drop-first）上限。
 *
 * 每个水平吃掉一个自由度，水平过多会让设计矩阵接近秩亏、系数不可解释。
 * 12 个水平（→11 个哑变量）已经覆盖绝大多数临床分组；超限直接报错，
 * 而不是悄悄截断水平——截断会让部分观测被无声归到参考组，是数据造假。
 */
const DUMMY_MAX_LEVELS = 12;

interface DesignTerm {
  /** 展示用 term 名：数值列即列名，哑变量为 `列名=水平`。 */
  term: string;
  /** 哑变量的参考水平；数值列为 null。 */
  reference: string | null;
}

/**
 * 构建含哑变量编码的设计矩阵。
 *
 * 此前 designMatrix 直接对每个预测变量调 numericColumn，遇到 smoke
 * (never/former/current) 会抛 `non-numeric value in column smoke`——用户在
 * 前端选了分类变量做回归就整个跑不起来。这里按列的实际取值决定编码方式：
 *
 *  - 全部可转数字 → 原样作为一列连续预测变量（与旧行为逐字节一致）；
 *  - 否则视为分类 → drop-first one-hot：按水平名排序后取第一个作参考水平，
 *    其余每个水平出一列 0/1 指示变量，term 命名 `列名=水平`。
 *
 * 选「排序后第一个」作参考而不是「出现频次最高」：前者只依赖数据取值集合，
 * 同一份数据无论行序如何都得到同一个参考水平，可复现；频次最高在并列时
 * 需要额外破并规则，且行序变化会改变模型输出。参考水平会写进每个系数的
 * reference 字段，前端系数表已有展示位（coeffFields.ts 的 reference）。
 *
 * 空值一律报错而不是当作独立水平：缺失该由协议的缺失策略处理，
 * 静默把空串编码成一个「空水平」会让模型多出一个无意义的对照组。
 */
function encodedDesignMatrix(
  headers: string[],
  rows: string[][],
  predictors: string[],
): { matrix: number[][]; terms: DesignTerm[] } {
  const columns: Array<{ values: number[]; terms: DesignTerm[] }> = predictors.map((predictor) => {
    const idx = columnIndex(headers, predictor);
    const raw = rows.map((row, rowIndex) => {
      const value = row[idx]?.trim() ?? '';
      if (value.length === 0) {
        throw new Error(`missing value in column ${predictor} at data row ${rowIndex + 1}`);
      }
      return value;
    });

    const allNumeric = raw.every((value) => Number.isFinite(Number(value)));
    if (allNumeric) {
      return {
        values: raw.map(Number),
        terms: [{ term: predictor, reference: null }],
      };
    }

    const levels = [...new Set(raw)].sort();
    if (levels.length < 2) {
      throw new Error(`categorical predictor ${predictor} has no variation (single level ${levels[0]}).`);
    }
    if (levels.length > DUMMY_MAX_LEVELS) {
      throw new Error(
        `categorical predictor ${predictor} has ${levels.length} levels (limit ${DUMMY_MAX_LEVELS}); collapse levels before modelling.`,
      );
    }
    const reference = levels[0]!;
    const dummyLevels = levels.slice(1);
    return {
      // 逐行展开成 dummyLevels.length 个 0/1 值，稍后按行重组。
      values: rows.flatMap((_, rowIndex) => dummyLevels.map((level) => (raw[rowIndex] === level ? 1 : 0))),
      terms: dummyLevels.map((level) => ({ term: `${predictor}=${level}`, reference })),
    };
  });

  const terms = columns.flatMap((column) => column.terms);
  const matrix = rows.map((_, rowIndex) => {
    const row: number[] = [1];
    for (const column of columns) {
      const width = column.terms.length;
      for (let offset = 0; offset < width; offset += 1) {
        row.push(column.values[rowIndex * width + offset]!);
      }
    }
    return row;
  });
  return { matrix, terms };
}

/** Attach term names and reference levels produced by encodedDesignMatrix. */
function withDesignTerms<T extends object>(
  coefficients: readonly T[],
  terms: readonly DesignTerm[],
): Array<T & { term: string; reference: string | null }> {
  // 系数顺序是 [intercept, ...terms]；截距没有参考水平。
  const named: DesignTerm[] = [{ term: '(Intercept)', reference: null }, ...terms];
  return coefficients.map((coeff, index) => {
    const fallback = named[index] ?? { term: `β${index}`, reference: null };
    const existing = (coeff as { term?: unknown }).term;
    const term = typeof existing === 'string' && existing.length > 0 ? existing : fallback.term;
    return { ...coeff, term, reference: fallback.reference };
  });
}

const COLLINEARITY_CONDITION_WARNING = 30;
const COLLINEARITY_VIF_WARNING = 10;
const SPARSE_INFORMATION_WARNING = 10;

function modelDesignDiagnostic(
  predictorRows: readonly (readonly number[])[],
  predictors: readonly string[],
): Record<string, unknown> {
  const diagnostic = stats.modelDiagnostics.diagnosePredictorDesign(predictorRows, predictors);
  if (diagnostic.rankDeficient) {
    const implicated = diagnostic.constantTerms.length > 0
      ? ` Constant predictors: ${diagnostic.constantTerms.join(', ')}.`
      : ' Remove redundant or linearly dependent predictors.';
    throw new Error(
      `Design matrix is rank deficient (rank ${diagnostic.rank} of ${diagnostic.predictorCount}).${implicated}`,
    );
  }
  const highVif = diagnostic.maxVif !== null && diagnostic.maxVif > COLLINEARITY_VIF_WARNING;
  const highCondition = diagnostic.conditionIndex > COLLINEARITY_CONDITION_WARNING;
  const warning = highVif || highCondition;
  const maxVifLabel = diagnostic.maxVif === null ? '不可用' : diagnostic.maxVif.toPrecision(4);
  const conditionLabel = Number.isFinite(diagnostic.conditionIndex)
    ? diagnostic.conditionIndex.toPrecision(4)
    : '∞';
  return {
    status: warning ? 'warning' : 'passed',
    rank: diagnostic.rank,
    predictor_count: diagnostic.predictorCount,
    condition_index: Number.isFinite(diagnostic.conditionIndex) ? diagnostic.conditionIndex : null,
    condition_warning_threshold: COLLINEARITY_CONDITION_WARNING,
    vif: diagnostic.vif,
    max_vif: diagnostic.maxVif,
    vif_warning_threshold: COLLINEARITY_VIF_WARNING,
    message: warning
      ? `共线性筛查触发：最大 VIF=${maxVifLabel}，条件指数=${conditionLabel}；应检查变量编码、冗余与估计稳定性。`
      : `共线性筛查未触发：秩 ${diagnostic.rank}/${diagnostic.predictorCount}，最大 VIF=${maxVifLabel}，条件指数=${conditionLabel}。`,
  };
}

function sparseInformationDiagnostic(
  informationKind: 'rarer_outcome_class' | 'events' | 'not_applicable',
  informationN: number | null,
  predictorCount: number,
): Record<string, unknown> {
  if (informationKind === 'not_applicable' || informationN === null || predictorCount === 0) {
    return {
      status: 'not_applicable',
      information_kind: informationKind,
      information_n: informationN,
      predictor_count: predictorCount,
      information_per_predictor: null,
      warning_threshold: SPARSE_INFORMATION_WARNING,
      message: predictorCount === 0
        ? '无预测变量，事件/参数经验性筛查不适用。'
        : '当前模型不使用事件/参数经验性筛查。',
    };
  }
  const ratio = informationN / predictorCount;
  const warning = ratio < SPARSE_INFORMATION_WARNING;
  const label = informationKind === 'events' ? '事件数' : '较少结局类别数';
  return {
    status: warning ? 'warning' : 'passed',
    information_kind: informationKind,
    information_n: informationN,
    predictor_count: predictorCount,
    information_per_predictor: ratio,
    warning_threshold: SPARSE_INFORMATION_WARNING,
    message: warning
      ? `${label}/预测变量=${ratio.toPrecision(4)}，低于经验性筛查阈值 10；应精简模型、增加信息量或采用适当惩罚方法。该阈值不是硬有效性判据。`
      : `${label}/预测变量=${ratio.toPrecision(4)}，未触发经验性稀疏信息警告；这不保证模型充分。`,
  };
}

function convergenceDiagnostic(converged: boolean | null, iterations?: number): Record<string, unknown> {
  if (converged === null) {
    return { status: 'not_applicable', iterations: null, message: '闭式解模型不适用迭代收敛诊断。' };
  }
  return {
    status: converged ? 'passed' : 'failed',
    iterations: iterations ?? null,
    message: converged
      ? `迭代拟合已收敛${iterations === undefined ? '' : `（${iterations} 次迭代）`}。`
      : `迭代拟合未收敛${iterations === undefined ? '' : `（达到 ${iterations} 次迭代）`}；系数、区间和检验不应解释。`,
  };
}

function numericalDegeneracyDiagnostic(
  coefficients: readonly { degenerate?: boolean }[],
): Record<string, unknown> {
  const coefficientIndexes = coefficients
    .map((coefficient, index) => coefficient.degenerate === true ? index : -1)
    .filter((index) => index >= 0);
  const warning = coefficientIndexes.length > 0;
  return {
    status: warning ? 'warning' : 'passed',
    coefficient_indexes: coefficientIndexes,
    message: warning
      ? `系数 ${coefficientIndexes.join(', ')} 的标准误或检验统计量退化；p 值仅作为边界结果，解释时必须结合该标记。`
      : '未检测到标准误为零或非有限检验统计量。',
  };
}

function ridgeRegularizationDiagnostic(regularized: boolean, ridgeValue: number): Record<string, unknown> {
  return {
    status: regularized ? 'warning' : 'passed',
    regularized,
    ridge_value: regularized ? ridgeValue : 0,
    message: regularized
      ? `信息矩阵求逆使用 ridge=${ridgeValue.toExponential(3)}；标准误和区间受正则化影响。`
      : '信息矩阵求逆未使用 ridge 正则化。',
  };
}

function logisticSeparationDiagnostic(
  coefficients: readonly { beta: number; stdError: number }[],
  converged: boolean,
): Record<string, unknown> {
  const slopes = coefficients.slice(1);
  const maxAbsBeta = slopes.reduce((max, coefficient) => Math.max(max, Math.abs(coefficient.beta)), 0);
  const maxStdError = slopes.reduce((max, coefficient) => Math.max(max, coefficient.stdError), 0);
  const warning = !converged || maxAbsBeta > 10 || maxStdError > 10;
  return {
    status: warning ? 'warning' : 'passed',
    max_abs_beta: maxAbsBeta,
    max_standard_error: maxStdError,
    beta_warning_threshold: 10,
    standard_error_warning_threshold: 10,
    message: warning
      ? '分离数值筛查触发（未收敛、|β|>10 或标准误>10）；需检查完全/准完全分离与稀疏单元格。'
      : '分离数值筛查未触发；该筛查不能证明完全/准完全分离不存在。',
  };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function finiteNumber(record: Record<string, unknown>, ...keys: string[]): number | null {
  for (const key of keys) {
    const value = record[key];
    if (typeof value === 'number' && Number.isFinite(value)) return value;
  }
  return null;
}

function selectedRunVariables(
  skillId: string,
  args: Record<string, unknown>,
  ctx: SkillContext,
): string[] {
  const strings = (...values: unknown[]): string[] => values.flatMap((value) => {
    if (typeof value === 'string' && value.length > 0) return [value];
    if (Array.isArray(value)) return value.filter((item): item is string => typeof item === 'string' && item.length > 0);
    return [];
  });
  const variables = skillId === 'tableone'
    ? strings(args.group, args.continuous, args.categorical)
    : skillId === 'ttest' || skillId === 'anova'
      ? strings(args.group, args.testVar)
      : skillId === 'correlation'
        ? strings(args.x, args.y)
        : skillId === 'survival_km'
          ? strings(args.time, args.event, args.group)
          : skillId === 'model_cox'
            ? strings(args.time, args.event, args.predictors)
            : strings(args.outcome, args.predictors);
  return [...new Set(variables.length > 0 ? variables : ctx.datasetSummary.columns.map((column) => column.name))];
}

function resultCounts(
  skillId: string,
  args: Record<string, unknown>,
  ctx: SkillContext,
): StandardResultContract['counts'] {
      const { headers, rows } = parseDelimitedTable(ctx.datasetBytes, ctx.datasetSummary.file_name);
  const variables = selectedRunVariables(skillId, args, ctx);
  const variableIndexes = variables.map((variable) => columnIndex(headers, variable));
  const completeCaseN = rows.filter((row) => variableIndexes.every((index) => (row[index] ?? '').trim().length > 0)).length;
  const eventVariable = skillId === 'model_logistic'
    ? (typeof args.outcome === 'string' ? args.outcome : null)
    : skillId === 'model_cox' || skillId === 'survival_km'
      ? (typeof args.event === 'string' ? args.event : null)
      : null;
  const eventN = eventVariable
    ? rows.reduce((sum, row) => {
        const value = Number(row[columnIndex(headers, eventVariable)]);
        return sum + (Number.isFinite(value) && value !== 0 ? 1 : 0);
      }, 0)
    : null;
  const timeVariable = skillId === 'model_cox' || skillId === 'survival_km'
    ? (typeof args.time === 'string' ? args.time : null)
    : null;
  const personTime = timeVariable
    ? rows.reduce((sum, row) => {
        const value = Number(row[columnIndex(headers, timeVariable)]);
        return sum + (Number.isFinite(value) ? value : 0);
      }, 0)
    : null;
  return {
    input_n: rows.length,
    complete_case_n: completeCaseN,
    missing_n: rows.length - completeCaseN,
    event_n: eventN,
    person_time: personTime,
  };
}

function coefficientEstimates(
  algorithmId: string,
  payload: Record<string, unknown>,
  adjustment: ResultContractEstimate['adjustment'],
): ResultContractEstimate[] {
  if (!Array.isArray(payload.coefficients)) return [];
  const unit: ResultContractEstimate['effect_unit'] = algorithmId === 'logistic'
    ? 'OR'
    : algorithmId === 'cox'
      ? 'HR'
      : 'Beta';
  return payload.coefficients.flatMap<ResultContractEstimate>((value, index) => {
    if (!isRecord(value)) return [];
    const estimate = algorithmId === 'logistic'
      ? finiteNumber(value, 'oddsRatio', 'odds_ratio')
      : algorithmId === 'cox'
        ? finiteNumber(value, 'hazardRatio', 'hazard_ratio')
        : finiteNumber(value, 'estimate', 'beta');
    if (estimate === null) return [];
    const lower = finiteNumber(value, 'ciLower', 'ci_lower');
    const upper = finiteNumber(value, 'ciUpper', 'ci_upper');
    return [{
      term: typeof value.term === 'string' ? value.term : `term_${index}`,
      estimate,
      ci_95: lower !== null && upper !== null ? { lower, upper } : null,
      p_value: finiteNumber(value, 'pValue', 'p_value'),
      effect_unit: unit,
      adjustment,
    }];
  });
}

function contractEstimates(
  skillId: string,
  algorithmId: string,
  args: Record<string, unknown>,
  payload: Record<string, unknown>,
): ResultContractEstimate[] {
  if (['linear', 'logistic', 'cox'].includes(algorithmId)) {
    const predictors = Array.isArray(args.predictors) ? args.predictors : [];
    return coefficientEstimates(algorithmId, payload, predictors.length > 1 ? 'adjusted' : 'unadjusted');
  }
  if (skillId === 'ttest') {
    const estimate = finiteNumber(payload, 'mean_diff');
    if (estimate === null) return [];
    const lower = finiteNumber(payload, 'ci_lower');
    const upper = finiteNumber(payload, 'ci_upper');
    return [{
      term: typeof args.testVar === 'string' ? args.testVar : 'mean_difference',
      estimate,
      ci_95: lower !== null && upper !== null ? { lower, upper } : null,
      p_value: finiteNumber(payload, 'p_value'),
      effect_unit: 'Mean difference',
      adjustment: 'unadjusted',
    }];
  }
  if (skillId === 'correlation') {
    const estimate = finiteNumber(payload, 'r');
    if (estimate === null) return [];
    const lower = finiteNumber(payload, 'ci_lower');
    const upper = finiteNumber(payload, 'ci_upper');
    const x = typeof args.x === 'string' ? args.x : 'x';
    const y = typeof args.y === 'string' ? args.y : 'y';
    return [{
      term: `${x}~${y}`,
      estimate,
      ci_95: lower !== null && upper !== null ? { lower, upper } : null,
      p_value: finiteNumber(payload, 'p_value'),
      effect_unit: 'Correlation coefficient',
      adjustment: 'unadjusted',
    }];
  }
  if (skillId === 'anova') {
    const estimate = finiteNumber(payload, 'eta_squared');
    if (estimate === null) return [];
    return [{
      term: typeof args.testVar === 'string' ? args.testVar : 'eta_squared',
      estimate,
      ci_95: null,
      p_value: finiteNumber(payload, 'p_value'),
      effect_unit: 'Eta squared',
      adjustment: 'unadjusted',
    }];
  }
  if (skillId === 'survival_km') {
    const estimate = finiteNumber(payload, 'median_survival');
    return estimate === null ? [] : [{
      term: 'median_survival',
      estimate,
      ci_95: null,
      p_value: null,
      effect_unit: 'Median survival',
      adjustment: 'descriptive',
    }];
  }
  return [];
}

function assumptionDiagnostics(
  algorithmId: string,
  payload: Record<string, unknown>,
): StandardResultContract['assumption_diagnostics'] {
  const diagnostics: StandardResultContract['assumption_diagnostics'] = [];
  const modelDiagnostics = isRecord(payload.model_diagnostics) ? payload.model_diagnostics : null;
  const appendModelDiagnostic = (key: string, code: string): void => {
    const value = modelDiagnostics && isRecord(modelDiagnostics[key]) ? modelDiagnostics[key] as Record<string, unknown> : null;
    if (value === null || typeof value.message !== 'string') return;
    const rawStatus = value.status;
    if (rawStatus === 'not_applicable') return;
    const status = rawStatus === 'passed' || rawStatus === 'failed' || rawStatus === 'warning'
      ? rawStatus
      : 'not_evaluated';
    diagnostics.push({ code, status, message: value.message });
  };
  appendModelDiagnostic('convergence', 'model-convergence');
  appendModelDiagnostic('sparse_data', 'model-sparse-data');
  appendModelDiagnostic('collinearity', 'model-collinearity');
  appendModelDiagnostic('numerical_degeneracy', 'model-numerical-degeneracy');
  appendModelDiagnostic('regularization', 'model-regularization');
  if (algorithmId === 'logistic') appendModelDiagnostic('separation_screen', 'logistic-separation-screen');

  if (algorithmId === 'cox') {
    const phTest = payload.ph_test && typeof payload.ph_test === 'object'
      ? payload.ph_test as Record<string, unknown>
      : null;
    if (phTest?.status === 'computed' && typeof phTest.p_value === 'number') {
      const violated = phTest.violated === true;
      diagnostics.push({
        code: 'cox-ph',
        status: violated ? 'failed' : 'passed',
        message: violated
          ? `时间交互 score 诊断提示比例风险假设可能违反（p=${phTest.p_value.toPrecision(4)}）；建议评估分层 Cox、时间交互项或分时段 HR。`
          : `时间交互 score 诊断未发现比例风险假设违反（p=${phTest.p_value.toPrecision(4)}）；这不等同于证明假设成立。`,
      });
    } else {
      diagnostics.push({
        code: 'cox-ph',
        status: 'not_evaluated',
        message: '比例风险诊断未计算；请检查事件数、模型收敛和信息矩阵。',
      });
    }
  }
  const definitions: Record<string, Array<[string, string]>> = {
    tableone: [
      ['tableone-continuous-normality', '连续变量组间检验的正态性未在当前运行中自动检验。'],
      ['tableone-continuous-variance', '连续变量组间检验的方差结构未在当前运行中自动诊断；两组比较使用 Welch 校正。'],
      ['tableone-multiple-comparisons', 'Table One 对多个变量同时进行组间检验，p 值未做多重比较校正，仅供描述性参考，不应据此得出主分析结论。'],
    ],
    ttest: [
      ['ttest-normality', '正态性未在当前运行中自动检验。'],
      ['ttest-variance', '方差结构未在当前运行中自动诊断；当前方法使用 Welch 校正。'],
    ],
    anova: [
      ['anova-normality', '组内正态性未在当前运行中自动检验。'],
      ['anova-homogeneity', '方差齐性未在当前运行中自动诊断。'],
    ],
    correlation: [
      ['correlation-linearity', '线性相关假设未在当前运行中自动诊断；Spearman 对单调关系更稳健。'],
    ],
    linear: [
      ['linear-linearity', '线性关系未在当前运行中自动诊断。'],
      ['linear-residuals', '残差正态性与同方差性未在当前运行中自动诊断。'],
    ],
    logistic: [
      ['logistic-calibration', '模型校准未在当前运行中自动诊断。'],
    ],
    kaplan_meier: [['km-censoring', '独立删失假设无法由当前数据自动验证。']],
  };
  diagnostics.push(...(definitions[algorithmId] ?? []).map(([code, message]) => ({
    code,
    status: 'not_evaluated' as const,
    message,
  })));
  return diagnostics;
}

function buildStandardResultContract(
  skillId: string,
  algorithmId: string,
  args: Record<string, unknown>,
  ctx: SkillContext,
  payload: Record<string, unknown>,
): StandardResultContract {
  const predictors = Array.isArray(args.predictors) ? args.predictors : [];
  const isRegression = ['linear', 'logistic', 'cox'].includes(algorithmId);
  const adjustedAvailable = isRegression && predictors.length > 1;
  const estimates = contractEstimates(skillId, algorithmId, args, payload);
  const counts = resultCounts(skillId, args, ctx);
  const matrix = coverage.getLoadedMatrix();
  const matrixEntry = matrix.algorithms.find((entry) => entry.id === algorithmId);
  const convergence = algorithmId === 'logistic' || algorithmId === 'cox'
    ? payload.converged === true
      ? 'converged'
      : payload.converged === false
        ? 'failed'
        : 'unknown'
    : 'not_applicable';
  const unsupportedConclusions = [
    '不能仅凭观察性统计关联作出因果结论。',
    '不能用于独立诊断、治疗或个体决策。',
  ];
  if (adjustedAvailable) {
    unsupportedConclusions.push('当前运行未同时计算未调整模型，不能把调整前后差异作为既得结论。');
  }

  return {
    schema_version: '1.0',
    method: { algorithm_id: algorithmId, method_version: SCHEMA_VERSION },
    estimates,
    counts,
    analysis_availability: isRegression
      ? {
          unadjusted: adjustedAvailable ? 'not_computed' : 'available',
          adjusted: adjustedAvailable ? 'available' : 'not_computed',
        }
      : { unadjusted: skillId === 'tableone' ? 'not_applicable' : 'available', adjusted: 'not_applicable' },
    effect_unit: estimates[0]?.effect_unit ?? null,
    convergence: { status: convergence },
    assumption_diagnostics: assumptionDiagnostics(algorithmId, payload),
    exclusions: counts.missing_n > 0
      ? [{ reason: '所用变量至少一项缺失；具体排除与可用案例规则见算法结果。', n: counts.missing_n }]
      : [],
    interpretation: {
      statistical: null,
      practical_significance: null,
      unsupported_conclusions: unsupportedConclusions,
    },
    provenance: {
      engine_name: '@stats-code/engine',
      engine_version: matrix.release_version,
      validation_coverage: matrixEntry ? { ...matrixEntry.coverage } : {},
    },
  };
}

function tableOneColumns(args: Record<string, unknown>, ctx: SkillContext): { continuous: string[]; categorical: string[] } {
  const continuous = asOptionalStringArray(args.continuous, 'continuous');
  const categorical = asOptionalStringArray(args.categorical, 'categorical');
  if (continuous.length > 0 || categorical.length > 0) {
    return { continuous, categorical };
  }
  return {
    continuous: ctx.datasetSummary.columns
      .filter((column) => column.inferred_type === 'Numeric')
      .map((column) => column.name),
    categorical: ctx.datasetSummary.columns
      .filter((column) => column.inferred_type === 'Categorical' || column.inferred_type === 'String')
      .map((column) => column.name),
  };
}

type TableOneGroupSummary = {
  label: string;
  n: number;
  continuous: Array<ReturnType<typeof stats.tableone.summarizeContinuous>>;
  categorical: Array<ReturnType<typeof stats.tableone.summarizeCategorical>>;
};

function tableOneStandardizedDifferences(
  groups: TableOneGroupSummary[],
  continuous: string[],
  categorical: string[],
) {
  const pair = groups.length === 2 ? [groups[0]!, groups[1]!] as const : null;
  const findContinuous = (group: TableOneGroupSummary, variable: string) =>
    group.continuous.find((summary) => summary.variable === variable);
  const findCategorical = (group: TableOneGroupSummary, variable: string) =>
    group.categorical.find((summary) => summary.variable === variable);

  return {
    comparison: pair ? { first: pair[0].label, second: pair[1].label } : null,
    continuous: continuous.map((variable) => {
      const first = pair ? findContinuous(pair[0], variable) : undefined;
      const second = pair ? findContinuous(pair[1], variable) : undefined;
      return {
        variable,
        smd: first && second ? stats.tableone.standardizedMeanDifference(first, second) : null,
      };
    }),
    categorical: categorical.map((variable) => {
      const first = pair ? findCategorical(pair[0], variable) : undefined;
      const second = pair ? findCategorical(pair[1], variable) : undefined;
      const levels = [...new Set([
        ...(first?.levels.map((level) => level.level) ?? []),
        ...(second?.levels.map((level) => level.level) ?? []),
      ])].sort();
      const levelDifferences = levels.map((level) => ({
        level,
        smd: first && second
          ? stats.tableone.standardizedProportionDifference(
              first.levels.find((entry) => entry.level === level)?.count ?? 0,
              first.n,
              second.levels.find((entry) => entry.level === level)?.count ?? 0,
              second.n,
            )
          : null,
      }));
      const finiteDifferences = levelDifferences
        .map((entry) => entry.smd)
        .filter((value): value is number => value !== null && Number.isFinite(value));
      return {
        variable,
        smd: finiteDifferences.length > 0 ? Math.max(...finiteDifferences) : null,
        levels: levelDifferences,
      };
    }),
  };
}

const CATEGORICAL_TEST_REASON = {
  empty_table: '没有可用于比较的非缺失观测。',
  insufficient_dimensions: '至少需要 2 个组和 2 个非缺失水平。',
  zero_margin: '至少一个组或水平没有非缺失观测，未计算检验。',
  sparse_non_2x2: '存在期望频数小于 5；当前仅对 2×2 表提供 Fisher 精确检验。',
} as const;

function tableOneCategoricalTests(
  groups: TableOneGroupSummary[],
  categorical: string[],
  strata: string | null,
) {
  const findCategorical = (group: TableOneGroupSummary, variable: string) =>
    group.categorical.find((summary) => summary.variable === variable);

  return categorical.map((variable) => {
    if (variable === strata) {
      return {
        variable,
        status: 'not_applicable' as const,
        method: null,
        statistic: null,
        degrees_of_freedom: null,
        p_value: null,
        min_expected_count: null,
        expected_below_5: 0,
        observed_zero_cells: 0,
        groups: groups.map((group) => group.label),
        levels: [],
        observed: [],
        reason: '分组变量自身不进行组间独立性检验。',
      };
    }

    const levels = [...new Set(groups.flatMap((group) =>
      findCategorical(group, variable)?.levels.map((level) => level.level) ?? []))].sort();
    const table = groups.map((group) => {
      const summary = findCategorical(group, variable);
      return levels.map((level) => summary?.levels.find((entry) => entry.level === level)?.count ?? 0);
    });
    const result = stats.contingency.categoricalIndependenceTest(table);
    if (result.status === 'not_computed') {
      return {
        variable,
        status: result.status,
        method: null,
        statistic: null,
        degrees_of_freedom: null,
        p_value: null,
        min_expected_count: result.minExpectedCount,
        expected_below_5: result.expectedBelowFive,
        observed_zero_cells: result.observedZeroCells,
        groups: groups.map((group) => group.label),
        levels,
        observed: table,
        reason: CATEGORICAL_TEST_REASON[result.reason],
      };
    }
    return {
      variable,
      status: result.status,
      method: result.method,
      statistic: result.statistic,
      degrees_of_freedom: result.degreesOfFreedom,
      p_value: result.pValue,
      min_expected_count: result.minExpectedCount,
      expected_below_5: result.expectedBelowFive,
      observed_zero_cells: result.observedZeroCells,
      groups: groups.map((group) => group.label),
      levels,
      observed: table,
      reason: result.method === 'fisher_exact'
        ? '期望频数小于 5 或出现零格，已自动使用 Fisher 精确检验。'
        : null,
    };
  });
}

const CONTINUOUS_TEST_REASON = {
  strata_self: '分组变量自身不进行组间差异检验。',
  insufficient_groups: '有效组数（该变量非缺失观测所在的组）不足 2 个，未计算组间差异检验。',
  underpowered_group: '至少一个参与组该变量的非缺失观测数小于 2，无法估计组内方差，未计算检验。',
  degenerate: '检验统计量或 p 值数值退化（如组内方差为零导致标准误退化），结果不可解释。',
} as const;

/**
 * Continuous-variable group-difference tests for Table One: Welch's t-test for
 * exactly 2 effective groups, one-way ANOVA for ≥3. Reuses the same
 * (label, indexes) pairs that built `groups`/the displayed n — NEVER
 * `numericValuesByGroup`, which independently rebuilds groups and would let the
 * test's N drift from the table's displayed n (see file header note above
 * `numericValuesByGroup`). The non-missing filter mirrors
 * `stats.tableone.summarizeContinuous`'s `v !== null && Number.isFinite(v)`
 * predicate exactly, so `group_ns` is provably consistent with each group's
 * displayed continuous summary `n`.
 *
 * Both engine calls are wrapped in try/catch: `welchTtest` throws (rather than
 * returning a degenerate result) when both groups have zero variance — this is
 * an empirically confirmed engine behavior (see task report), not a guess —
 * and `oneWayAnova` throws on some structurally-invalid inputs that our
 * pre-checks already rule out, but the guard stays as defense in depth so one
 * variable's numeric edge case never fails the entire tableone run.
 */
function tableOneContinuousTests(
  headers: string[],
  rows: string[][],
  groupEntries: Array<{ label: string; indexes: number[] }>,
  groups: TableOneGroupSummary[],
  continuous: string[],
  strata: string | null,
) {
  const groupLabels = groups.map((group) => group.label);

  const notComputed = (
    variable: string,
    groupNs: number[],
    reason: string,
    degenerate: boolean,
  ) => ({
    variable,
    status: 'not_computed' as const,
    method: null,
    statistic: null,
    degrees_of_freedom: null,
    degrees_of_freedom_denominator: null,
    p_value: null,
    groups: groupLabels,
    group_ns: groupNs,
    degenerate,
    reason,
  });

  return continuous.map((variable) => {
    // group_ns is read directly from the already-computed table summaries
    // (the exact values rendered in `groups[].continuous`), not recomputed
    // independently, so it cannot drift from the displayed per-group n.
    const groupNs = groups.map((group) =>
      group.continuous.find((summary) => summary.variable === variable)?.n ?? 0);

    if (variable === strata) {
      return {
        variable,
        status: 'not_applicable' as const,
        method: null,
        statistic: null,
        degrees_of_freedom: null,
        degrees_of_freedom_denominator: null,
        p_value: null,
        groups: groupLabels,
        group_ns: groupNs,
        degenerate: false,
        reason: CONTINUOUS_TEST_REASON.strata_self,
      };
    }

    // Recompute the exact filtered arrays `summarizeContinuous` used for this
    // variable, per group, from the SAME groupIndexes as the table.
    const perGroupValues = groupEntries.map(({ label, indexes }) => {
      const raw = nullableNumericColumn(headers, rows, variable, indexes);
      const values = raw.filter((v): v is number => v !== null && Number.isFinite(v));
      return { label, values };
    });
    const effective = perGroupValues.filter((entry) => entry.values.length >= 1);

    if (effective.length < 2) {
      return notComputed(variable, groupNs, CONTINUOUS_TEST_REASON.insufficient_groups, false);
    }
    if (effective.some((entry) => entry.values.length < 2)) {
      return notComputed(variable, groupNs, CONTINUOUS_TEST_REASON.underpowered_group, false);
    }

    if (effective.length === 2) {
      try {
        const result = stats.ttest.welchTtest(effective[0]!.values, effective[1]!.values, 0.05);
        if (!Number.isFinite(result.tStatistic) || !Number.isFinite(result.pValue)) {
          return notComputed(variable, groupNs, CONTINUOUS_TEST_REASON.degenerate, true);
        }
        return {
          variable,
          status: 'computed' as const,
          method: 'welch_t' as const,
          statistic: result.tStatistic,
          degrees_of_freedom: result.df,
          degrees_of_freedom_denominator: null,
          p_value: result.pValue,
          groups: groupLabels,
          group_ns: groupNs,
          degenerate: false,
          reason: null,
        };
      } catch {
        return notComputed(variable, groupNs, CONTINUOUS_TEST_REASON.degenerate, true);
      }
    }

    try {
      const result = stats.anova.oneWayAnova(effective.map((entry) => entry.values));
      if (!Number.isFinite(result.fStatistic) || !Number.isFinite(result.pValue) || result.degenerate) {
        return notComputed(variable, groupNs, CONTINUOUS_TEST_REASON.degenerate, true);
      }
      return {
        variable,
        status: 'computed' as const,
        method: 'one_way_anova' as const,
        statistic: result.fStatistic,
        degrees_of_freedom: result.dfBetween,
        degrees_of_freedom_denominator: result.dfWithin,
        p_value: result.pValue,
        groups: groupLabels,
        group_ns: groupNs,
        degenerate: false,
        reason: null,
      };
    } catch {
      return notComputed(variable, groupNs, CONTINUOUS_TEST_REASON.degenerate, true);
    }
  });
}

function runTableOne(headers: string[], rows: string[][], args: Record<string, unknown>, ctx: SkillContext): Record<string, unknown> {
  const { continuous, categorical } = tableOneColumns(args, ctx);
  const group = typeof args.group === 'string' && args.group.length > 0 ? args.group : null;
  const allIndexes = rowIndexes(rows);
  const groupIndexes = new Map<string, number[]>();

  if (group) {
    const groupIdx = columnIndex(headers, group);
    for (const index of allIndexes) {
      const label = rows[index]?.[groupIdx]?.trim() ?? '';
      if (label.length === 0) continue;
      const indexes = groupIndexes.get(label) ?? [];
      indexes.push(index);
      groupIndexes.set(label, indexes);
    }
  } else {
    groupIndexes.set('Overall', allIndexes);
  }

  const groupEntries = [...groupIndexes.entries()].map(([label, indexes]) => ({ label, indexes }));

  const groups: TableOneGroupSummary[] = groupEntries.map(({ label, indexes }) => ({
    label,
    n: indexes.length,
    continuous: continuous.map((name) =>
      stats.tableone.summarizeContinuous(name, nullableNumericColumn(headers, rows, name, indexes)),
    ),
    categorical: categorical.map((name) =>
      stats.tableone.summarizeCategorical(name, nullableStringColumn(headers, rows, name, indexes)),
    ),
  }));

  return {
    strata: group,
    continuous,
    categorical,
    groups,
    standardized_differences: tableOneStandardizedDifferences(groups, continuous, categorical),
    categorical_tests: tableOneCategoricalTests(groups, categorical, group),
    continuous_tests: tableOneContinuousTests(headers, rows, groupEntries, groups, continuous, group),
  };
}

function runTtest(headers: string[], rows: string[][], args: Record<string, unknown>): Record<string, unknown> {
  const group = asString(args.group, 'group');
  const testVar = asString(args.testVar, 'testVar');
  const alpha = args.alpha !== undefined ? Number(args.alpha) : 0.05;
  const buckets = numericValuesByGroup(headers, rows, group, testVar);
  const entries = [...buckets.entries()].filter(([, values]) => values.length > 0);
  if (entries.length !== 2) {
    throw new Error(`T test requires exactly two non-empty groups in ${group}; got ${entries.length}`);
  }
  const [first, second] = entries;
  const result = stats.ttest.welchTtest(first![1], second![1], alpha);
  return {
    method: result.method,
    group_variable: group,
    test_variable: testVar,
    groups: [
      { label: first![0], n: first![1].length, mean: result.mean1 },
      { label: second![0], n: second![1].length, mean: result.mean2 },
    ],
    mean_diff: result.meanDiff,
    t_statistic: result.tStatistic,
    df: result.df,
    p_value: result.pValue,
    ci_lower: result.ciLower,
    ci_upper: result.ciUpper,
    alpha: result.alpha,
  };
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
      // Hand-thrown validation errors — the engine's input gates and the
      // column-extraction helpers all throw plain `Error` with a clear,
      // user-actionable message (single group, zero variance, collinearity,
      // non-numeric text, …) — are input rejections → invalid_args (422 at the
      // route, D15). Genuine defects surface as TypeError/RangeError/etc. and
      // stay execution_failed (500).
      if (err instanceof Error && err.constructor === Error) {
        throw new SkillRunErrorException({
          kind: 'invalid_args',
          missing: [],
          message: err.message.slice(0, MAX_DIAGNOSTIC_CHARS),
        });
      }
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
    const riskSignals = detectRiskSignals(payload, {
      phase: descriptor.skillId === 'power' ? 'design' : 'analysis',
    });

    let analysis: AnalysisResultMeta | null = null;
    const algorithmId = skillToAlgorithm(descriptor.skillId);
    if (algorithmId !== null) {
      analysis = {
        algorithm_id: algorithmId,
        dataset_id: ctx.datasetSummary.dataset_id,
        dataset_sha256: ctx.datasetSummary.sha256 ?? null,
        columns: ctx.datasetSummary.columns,
        params: args,
        run_id: randomUUID(),
        run_status: 'completed',
        result_contract: buildStandardResultContract(descriptor.skillId, algorithmId, args, ctx, payload),
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

    const { headers, rows } = parseDelimitedTable(ctx.datasetBytes, ctx.datasetSummary.file_name);

    switch (invoker.algorithmId) {
      case 'tableone':
        return runTableOne(headers, rows, args, ctx);
      case 'ttest':
        return runTtest(headers, rows, args);
      case 'anova': {
        const group = asString(args.group, 'group');
        const testVar = asString(args.testVar, 'testVar');
        const buckets = numericValuesByGroup(headers, rows, group, testVar);
        if (buckets.size < 2) {
          throw new Error('ANOVA requires at least 2 non-empty groups.');
        }
        const labels = [...buckets.keys()].sort();
        const groups = labels.map((label) => buckets.get(label)!);
        const res = stats.anova.oneWayAnova(groups);
        // Per-group mean/SD so the client can draw the group comparison without
        // recomputing anything from data it does not hold.
        const groupStats = labels.map((label, i) => {
          const values = groups[i]!;
          const mean = values.reduce((sum, v) => sum + v, 0) / values.length;
          const variance = values.length > 1
            ? values.reduce((sum, v) => sum + (v - mean) ** 2, 0) / (values.length - 1)
            : 0;
          return { group: label, n: values.length, mean, sd: Math.sqrt(variance) };
        });
        return {
          method: res.method,
          group_variable: group,
          test_variable: testVar,
          groups: labels,
          group_ns: Object.fromEntries(labels.map((label, i) => [label, groups[i]!.length])),
          group_stats: groupStats,
          k: res.k,
          n_total: res.nTotal,
          ss_between: res.ssBetween,
          ss_within: res.ssWithin,
          ss_total: res.ssTotal,
          df_between: res.dfBetween,
          df_within: res.dfWithin,
          ms_between: res.msBetween,
          ms_within: res.msWithin,
          f_statistic: res.fStatistic,
          p_value: res.pValue,
          eta_squared: res.etaSquared,
          degenerate: res.degenerate,
        };
      }
      case 'correlation': {
        const xName = asString(args.x, 'x');
        const yName = asString(args.y, 'y');
        const methodRaw = typeof args.method === 'string' ? args.method.trim().toLowerCase() : 'pearson';
        // Reject unknown methods instead of quietly substituting Pearson: asking
        // for Kendall and receiving a Pearson coefficient labelled `pearson` is
        // a silent answer to a question the user did not ask.
        if (methodRaw !== 'pearson' && methodRaw !== 'spearman') {
          throw new Error(`unsupported correlation method '${methodRaw}'; expected 'pearson' or 'spearman'`);
        }
        const method = methodRaw;
        const x = numericColumn(headers, rows, xName);
        const y = numericColumn(headers, rows, yName);
        const res = method === 'spearman'
          ? stats.correlation.spearmanCorrelation(x, y)
          : stats.correlation.pearsonCorrelation(x, y);
        return {
          method: res.method,
          x: xName,
          y: yName,
          n: res.n,
          r: res.r,
          t_statistic: res.tStatistic,
          df: res.df,
          p_value: res.pValue,
          ci_lower: res.ciLower,
          ci_upper: res.ciUpper,
          alpha: res.alpha,
        };
      }
      case 'linear': {
        const outcome = asString(args.outcome, 'outcome');
        const predictors = asStringArray(args.predictors, 'predictors');
        const y = numericColumn(headers, rows, outcome);
        const { matrix: x, terms } = encodedDesignMatrix(headers, rows, predictors);
        // 共线性诊断按展开后的列走：哑变量之间的相关性才是真实的设计矩阵结构，
        // 用原始 predictors 名会与列数不匹配。
        const collinearity = modelDesignDiagnostic(
          x.map((row) => row.slice(1)),
          terms.map((entry) => entry.term),
        );
        const res = stats.linear.ols(x, y);
        return {
          coefficients: withDesignTerms(res.coefficients, terms),
          r_squared: res.rSquared,
          adj_r_squared: res.adjRSquared,
          f_statistic: res.fStatistic,
          p_value: res.fPValue,
          residual_std_error: res.residualStdError,
          n: res.n,
          degenerate: res.degenerate,
          model_diagnostics: {
            convergence: convergenceDiagnostic(null),
            sparse_data: sparseInformationDiagnostic('not_applicable', null, predictors.length),
            collinearity,
            numerical_degeneracy: numericalDegeneracyDiagnostic(res.coefficients),
          },
        };
      }
      case 'logistic': {
        const outcome = asString(args.outcome, 'outcome');
        const predictors = asStringArray(args.predictors, 'predictors');
        const y = numericColumn(headers, rows, outcome);
        const { matrix: x, terms } = encodedDesignMatrix(headers, rows, predictors);
        const collinearity = modelDesignDiagnostic(
          x.map((row) => row.slice(1)),
          terms.map((entry) => entry.term),
        );
        const res = stats.logistic.logisticRegression(x, y);
        const eventN = y.reduce((sum, value) => sum + value, 0);
        const nonEventN = y.length - eventN;
        return {
          coefficients: withDesignTerms(res.coefficients, terms),
          odds_ratios: res.coefficients.map((c) => c.oddsRatio),
          p_values: res.coefficients.map((c) => c.pValue),
          log_likelihood: res.logLikelihood,
          converged: res.converged,
          iterations: res.iterations,
          regularized: res.regularized,
          ridge_value: res.ridgeValue,
          degenerate: res.degenerate,
          event_n: eventN,
          non_event_n: nonEventN,
          model_diagnostics: {
            convergence: convergenceDiagnostic(res.converged, res.iterations),
            sparse_data: sparseInformationDiagnostic('rarer_outcome_class', Math.min(eventN, nonEventN), predictors.length),
            collinearity,
            separation_screen: logisticSeparationDiagnostic(res.coefficients, res.converged),
            numerical_degeneracy: numericalDegeneracyDiagnostic(res.coefficients),
            regularization: ridgeRegularizationDiagnostic(res.regularized, res.ridgeValue),
          },
        };
      }
      case 'cox': {
        const time = asString(args.time, 'time');
        const event = asString(args.event, 'event');
        const predictors = asStringArray(args.predictors, 'predictors');
        const times = numericColumn(headers, rows, time);
        const events = numericColumn(headers, rows, event);
        if (events.some((value) => value !== 0 && value !== 1)) {
          throw new Error('Cox event indicator must be encoded as 0/1.');
        }
        // Cox 是无截距模型（基线风险被 partial likelihood 消掉），所以这里
        // 去掉 encodedDesignMatrix 铺的截距列，只保留展开后的预测变量列。
        const { matrix: coxDesign, terms } = encodedDesignMatrix(headers, rows, predictors);
        const predictorRows = coxDesign.map((row) => row.slice(1));
        const termNames = terms.map((entry) => entry.term);
        const collinearity = modelDesignDiagnostic(predictorRows, termNames);
        const observations = rows.map((_, i) => ({
          time: times[i]!,
          event: events[i]! !== 0,
          x: predictorRows[i]!,
          weight: 1,
        }));
        const res = stats.cox.coxRegression(observations);
        const eventN = observations.filter((observation) => observation.event).length;
        const phTest = res.converged
          ? stats.cox.coxProportionalHazardsTest(observations, res.coefficients.map((coefficient) => coefficient.beta))
          : null;
        const phPayload = phTest === null ? {
          status: 'not_computed',
          method: 'time_interaction_score',
          time_transform: 'centered_log1p',
          alpha: 0.05,
          statistic: null,
          degrees_of_freedom: null,
          p_value: null,
          violated: false,
          reason: 'model_not_converged',
          covariates: [],
          recommendation: '模型未收敛，不能解释 PH 诊断；应先修复模型拟合。',
        } : {
          status: phTest.status,
          method: phTest.method,
          time_transform: phTest.timeTransform,
          alpha: phTest.alpha,
          statistic: phTest.statistic,
          degrees_of_freedom: phTest.degreesOfFreedom,
          p_value: phTest.pValue,
          violated: phTest.violated,
          reason: phTest.status === 'not_computed' ? phTest.reason : null,
          covariates: phTest.covariates.map((covariate) => ({
            // 用展开后的 term 名：哑变量场景下 predictors[index] 会错位
            // （3 个水平的 smoke 占 2 列，索引不再与原列一一对应）。
            term: termNames[covariate.index] ?? `β${covariate.index}`,
            statistic: covariate.statistic,
            p_value: covariate.pValue,
            violated: covariate.violated,
          })),
          recommendation: phTest.status === 'computed' && phTest.violated
            ? '评估分层 Cox、显式时间交互项或分时段 HR，并由方法专家审核。'
            : phTest.status === 'computed'
              ? '未发现系统性时间趋势；仍需结合图形、设计和领域知识审核。'
              : '事件信息或信息矩阵不足，PH 诊断不可解释。',
        };
        // Cox design has no intercept column — terms are the expanded
        // predictor columns only (哑变量已在 encodedDesignMatrix 里展开)。
        return {
          coefficients: res.coefficients.map((coeff, index) => ({
            ...coeff,
            term: termNames[index] ?? `β${index}`,
            reference: terms[index]?.reference ?? null,
          })),
          hazard_ratios: res.coefficients.map((c) => c.hazardRatio),
          p_values: res.coefficients.map((c) => c.pValue),
          log_partial_likelihood: res.logPartialLikelihood,
          converged: res.converged,
          iterations: res.iterations,
          regularized: res.regularized,
          ridge_value: res.ridgeValue,
          degenerate: res.degenerate,
          event_n: eventN,
          ph_test: phPayload,
          cox_ph_violated: phPayload.violated,
          model_diagnostics: {
            convergence: convergenceDiagnostic(res.converged, res.iterations),
            sparse_data: sparseInformationDiagnostic('events', eventN, predictors.length),
            collinearity,
            numerical_degeneracy: numericalDegeneracyDiagnostic(res.coefficients),
            regularization: ridgeRegularizationDiagnostic(res.regularized, res.ridgeValue),
          },
        };
      }
      case 'kaplan_meier': {
        const time = asString(args.time, 'time');
        const event = asString(args.event, 'event');
        const times = numericColumn(headers, rows, time);
        const events = numericColumn(headers, rows, event).map((v) => v !== 0);
        const group = typeof args.group === 'string' && args.group.trim().length > 0 ? args.group : null;
        const groupValues = group
          ? rows.map((row) => row[columnIndex(headers, group)]?.trim() ?? '')
          : rows.map(() => 'Overall');
        if (groupValues.some((value) => value.length === 0)) {
          throw new Error(`Kaplan-Meier group column ${group ?? ''} contains missing values.`);
        }
        const labels = [...new Set(groupValues)].sort();
        const overall = stats.survival.kaplanMeier(times, events);
        const grouped = labels.map((label) => {
          const indexes = groupValues.map((value, index) => value === label ? index : -1).filter((index) => index >= 0);
          const groupTimes = indexes.map((index) => times[index]!);
          const groupEvents = indexes.map((index) => events[index]!);
          return {
            label,
            times: groupTimes,
            events: groupEvents,
            result: stats.survival.kaplanMeier(groupTimes, groupEvents),
          };
        });
        const logRank = group ? stats.survival.logRankTest(times, events, groupValues) : null;
        return {
          survival_table: overall.points,
          median_survival: overall.medianSurvival,
          groups: labels,
          steps: grouped.flatMap((entry) => entry.result.points.map((point) => ({
            group: entry.label,
            time: point.time,
            at_risk: point.atRisk,
            events: point.events,
            censored: point.censored,
            survival: point.survival,
            std_error: point.stdError,
          }))),
          group_summaries: grouped.map((entry) => {
            const eventCount = entry.events.filter(Boolean).length;
            return {
              group: entry.label,
              n: entry.times.length,
              event_n: eventCount,
              censored_n: entry.times.length - eventCount,
              median_survival: entry.result.medianSurvival,
            };
          }),
          log_rank: logRank === null ? null : {
            status: logRank.status,
            method: logRank.method,
            statistic: logRank.statistic,
            degrees_of_freedom: logRank.degreesOfFreedom,
            p_value: logRank.pValue,
            reason: logRank.status === 'not_computed' ? logRank.reason : null,
            groups: logRank.groups,
          },
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
        // `required_n` means "total" for one_proportion and "per arm" for the
        // two-group designs. Emitting the engine's own total removes the need
        // for the client to guess a multiplier from a label.
        total_n: res.totalN,
        achieved_power: res.achievedPower,
        effect_size: res.effectSize,
        alpha: res.alpha,
        method: res.method,
        converged: res.converged,
      };
    }
    throw new Error(`unknown native skill: ${skillId}`);
  }
}
