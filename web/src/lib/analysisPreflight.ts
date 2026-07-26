import type { DatasetSummary, ResearchProtocol, RunRequest } from '../api/types';
import { humanizeIdentifier } from './displayLabels';

export const ANALYSIS_TRUST_STATEMENT = '本机确定性引擎 · 数值非 LLM 生成 · 可审计';

export type PreflightWarningCode =
  | 'small-sample'
  | 'missing-data'
  | 'causal-language'
  | 'paired-design'
  | 'protocol-outcome-mismatch'
  | 'protocol-method-mismatch';

export interface PreflightWarning {
  code: PreflightWarningCode;
  severity: 'warning' | 'high';
  message: string;
}

export interface VariableMissingRate {
  variable: string;
  missingCount: number;
  rate: number;
}

export interface AnalysisPreflight {
  methodLabel: string;
  datasetName: string;
  rowCount: number;
  variables: string[];
  missingRates: VariableMissingRate[];
  warnings: PreflightWarning[];
  trustStatement: typeof ANALYSIS_TRUST_STATEMENT;
}

const METHOD_LABELS: Record<string, string> = {
  tableone: '基线特征表（Table One）',
  ttest: '双独立样本 T 检验',
  anova: '单因素方差分析',
  correlation: '相关分析',
  survival_km: 'Kaplan–Meier 生存分析',
  model_linear: '多元线性回归',
  model_logistic: 'Logistic 回归',
  model_cox: 'Cox 比例风险回归',
  power: '功效与样本量分析',
};

function requestArgs(request: RunRequest): Record<string, unknown> {
  return request.args && typeof request.args === 'object' && !Array.isArray(request.args)
    ? request.args as Record<string, unknown>
    : {};
}

function collectStrings(value: unknown): string[] {
  if (typeof value === 'string' && value.trim()) return [value];
  if (!Array.isArray(value)) return [];
  return value.filter((item): item is string => typeof item === 'string' && item.trim().length > 0);
}

function selectedVariables(request: RunRequest): string[] {
  const args = requestArgs(request);
  // 功效分析是设计阶段工具，参数全是标量（test_type/effect_size/alpha/power），
  // 不引用任何数据列。显式返回空数组，而不是靠默认分支查不到 outcome/predictors
  // 碰巧得到空结果。
  if (request.skill_id === 'power') return [];
  const orderedKeys = request.skill_id === 'tableone'
    ? ['group', 'continuous', 'categorical']
    : request.skill_id === 'ttest' || request.skill_id === 'anova'
      ? ['group', 'testVar']
      : request.skill_id === 'correlation'
        ? ['x', 'y']
        : request.skill_id === 'survival_km'
          ? ['time', 'event', 'group']
          : request.skill_id === 'model_cox'
            ? ['time', 'event', 'predictors']
            : ['outcome', 'predictors'];

  return [...new Set(orderedKeys.flatMap((key) => collectStrings(args[key])))];
}

function requestedOutcome(request: RunRequest): string | null {
  const args = requestArgs(request);
  const key = request.skill_id === 'ttest'
    ? 'testVar'
    : request.skill_id === 'survival_km' || request.skill_id === 'model_cox'
      ? 'event'
      : request.skill_id.startsWith('model_')
        ? 'outcome'
        : null;
  if (!key) return null;
  const value = args[key];
  return typeof value === 'string' && value.trim() ? value.trim() : null;
}

function protocolMentionsVariable(protocolOutcome: string, variable: string): boolean {
  const outcome = protocolOutcome.toLocaleLowerCase();
  const target = variable.toLocaleLowerCase();
  if (!/^[a-z0-9_]+$/i.test(target)) return outcome.includes(target);
  const escaped = target.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  return new RegExp(`(^|[^a-z0-9_])${escaped}([^a-z0-9_]|$)`, 'i').test(outcome);
}

function protocolMethodConflicts(request: RunRequest, protocol: ResearchProtocol): boolean {
  const declared = `${protocol.primary_analysis} ${protocol.estimand}`;
  const conflicts: Partial<Record<RunRequest['skill_id'], RegExp>> = {
    model_linear: /(?:\bOR\b|odds|比值比|优势比|几率比|\bHR\b|hazard|风险比|logistic|cox|生存)/i,
    model_logistic: /(?:\bHR\b|hazard|风险比|cox|生存|均值差|mean\s+difference|线性回归)/i,
    model_cox: /(?:\bOR\b|odds|比值比|优势比|几率比|logistic|均值差|mean\s+difference|线性回归)/i,
    ttest: /(?:\bOR\b|odds|比值比|优势比|几率比|\bHR\b|hazard|风险比|logistic|cox|生存)/i,
    survival_km: /(?:\bOR\b|odds|比值比|优势比|几率比|logistic|均值差|mean\s+difference|t[ -]?test|t\s*检验)/i,
  };
  return conflicts[request.skill_id]?.test(declared) ?? false;
}

export function buildAnalysisPreflight(
  dataset: DatasetSummary,
  request: RunRequest,
  promptText: string,
  protocol: ResearchProtocol | null = null,
): AnalysisPreflight {
  const variables = selectedVariables(request);
  const columnsByName = new Map(dataset.columns.map((column) => [column.name, column]));
  const missingRates = variables.flatMap<VariableMissingRate>((variable) => {
    const column = columnsByName.get(variable);
    if (!column) return [];
    const rate = dataset.row_count > 0
      ? Number(((column.missing_count / dataset.row_count) * 100).toFixed(1))
      : 0;
    return [{ variable, missingCount: column.missing_count, rate }];
  });

  const warnings: PreflightWarning[] = [];
  if (dataset.row_count < 30) {
    warnings.push({
      code: 'small-sample',
      severity: 'warning',
      message: `当前样本量 n=${dataset.row_count}，估计可能不稳定，请谨慎解释效应与置信区间。`,
    });
  }

  for (const item of missingRates) {
    if (item.rate < 5) continue;
    warnings.push({
      code: 'missing-data',
      severity: item.rate >= 20 ? 'high' : 'warning',
      message: `${item.variable} 缺失 ${item.missingCount} 例（${item.rate.toFixed(1)}%）${item.rate >= 20 ? '，属于高缺失风险' : ''}。`,
    });
  }

  if (/(导致|因果|造成|causal|cause)/i.test(promptText)) {
    warnings.push({
      code: 'causal-language',
      severity: 'warning',
      message: '检测到因果措辞；观察性统计关联不能直接证明因果关系。',
    });
  }

  if (request.skill_id === 'ttest' && /(前后|配对|重复测量|before|after|paired|repeated)/i.test(promptText)) {
    warnings.push({
      code: 'paired-design',
      severity: 'high',
      message: '当前方法是独立样本 T 检验，但描述疑似配对或重复测量设计，请确认研究设计。',
    });
  }

  if (protocol?.status === 'Approved' && request.skill_id !== 'tableone') {
    const outcome = requestedOutcome(request);
    if (outcome && !protocolMentionsVariable(protocol.outcome, outcome)) {
      warnings.push({
        code: 'protocol-outcome-mismatch',
        severity: 'high',
        message: `本次结局变量 ${outcome} 未在已审批协议结局“${protocol.outcome}”中明确出现，请先确认变量映射或修订协议。`,
      });
    }
    if (protocolMethodConflicts(request, protocol)) {
      warnings.push({
        code: 'protocol-method-mismatch',
        severity: 'high',
        message: `本次${(METHOD_LABELS[request.skill_id] ?? humanizeIdentifier(request.skill_id))}与已审批协议的方法或目标估计量不一致，请修订协议，或确认本次仅为预先声明的次要分析。`,
      });
    }
  }

  return {
    methodLabel: (METHOD_LABELS[request.skill_id] ?? humanizeIdentifier(request.skill_id)),
    datasetName: dataset.file_name,
    rowCount: dataset.row_count,
    variables,
    missingRates,
    warnings,
    trustStatement: ANALYSIS_TRUST_STATEMENT,
  };
}
