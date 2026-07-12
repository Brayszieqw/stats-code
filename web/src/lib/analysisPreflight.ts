import type { DatasetSummary, RunRequest } from '../api/types';

export const ANALYSIS_TRUST_STATEMENT = '本机确定性引擎 · 数值非 LLM 生成 · 可审计';

export type PreflightWarningCode =
  | 'small-sample'
  | 'missing-data'
  | 'causal-language'
  | 'paired-design';

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
  survival_km: 'Kaplan–Meier 生存分析',
  model_linear: '多元线性回归',
  model_logistic: 'Logistic 回归',
  model_cox: 'Cox 比例风险回归',
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
  const orderedKeys = request.skill_id === 'tableone'
    ? ['group', 'continuous', 'categorical']
    : request.skill_id === 'ttest'
      ? ['group', 'testVar']
      : request.skill_id === 'survival_km'
        ? ['time', 'event', 'group']
        : request.skill_id === 'model_cox'
          ? ['time', 'event', 'predictors']
          : ['outcome', 'predictors'];

  return [...new Set(orderedKeys.flatMap((key) => collectStrings(args[key])))];
}

export function buildAnalysisPreflight(
  dataset: DatasetSummary,
  request: RunRequest,
  promptText: string,
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

  return {
    methodLabel: METHOD_LABELS[request.skill_id] ?? request.skill_id,
    datasetName: dataset.file_name,
    rowCount: dataset.row_count,
    variables,
    missingRates,
    warnings,
    trustStatement: ANALYSIS_TRUST_STATEMENT,
  };
}
