// server/conversation/skill-registry.ts — SkillRegistry + default skill set.
//
// Mirrors crates/agent-core/src/skill/registry.rs: the 6-skill default set with
// identical ids, display names (Chinese), and input/output JSON schemas. The
// registry preserves deterministic insertion order via an internal ordered key
// list (the Rust `order: Vec<String>`).
//
// Output-level skills (model_*, survival_km) use {kind:'algorithm'} invokers
// resolved through skillToAlgorithm; inspect/power use {kind:'native'}.

import type { AlgorithmId } from '@stats-code/engine';
import type { SkillResult } from './skill-runner-types.js';

export interface SkillContext {
  datasetBytes: Uint8Array;
  datasetSummary: import('../state.js').DatasetSummary;
}

export type SkillInvoker =
  | { kind: 'algorithm'; algorithmId: AlgorithmId }
  | { kind: 'native'; run: (args: Record<string, unknown>, ctx: SkillContext) => Promise<SkillResult> };

export interface SkillDescriptor {
  skillId: string;
  displayName: string;
  /** JSON Schema (Draft 2020-12) describing accepted input parameters. */
  inputSchema: Record<string, unknown>;
  /** JSON Schema describing the structured output. */
  outputSchema: Record<string, unknown>;
  invoker: SkillInvoker;
}

export class SkillRegistry {
  private readonly skills = new Map<string, SkillDescriptor>();
  private readonly order: string[] = [];

  /** Register a descriptor; overwrites in place but preserves first-seen order. */
  register(desc: SkillDescriptor): void {
    if (!this.skills.has(desc.skillId)) {
      this.order.push(desc.skillId);
    }
    this.skills.set(desc.skillId, desc);
  }

  get(skillId: string): SkillDescriptor | undefined {
    return this.skills.get(skillId);
  }

  /** Enumerate registered skills in deterministic insertion order. */
  list(): SkillDescriptor[] {
    return this.order
      .map((id) => this.skills.get(id))
      .filter((d): d is SkillDescriptor => d !== undefined);
  }

  get size(): number {
    return this.skills.size;
  }

  /**
   * Pre-populated registry with the default skill set:
   * tableone, ttest, anova, correlation, model_*, survival_km, power, inspect.
   * Output-level skills use algorithm invokers; power/inspect are native
   * invokers (wired by the SkillRunner factory, not here).
   */
  static withDefaults(): SkillRegistry {
    const reg = new SkillRegistry();

    reg.register({
      skillId: 'tableone',
      displayName: '基线特征表',
      inputSchema: {
        type: 'object',
        properties: {
          group: { type: 'string', description: '分组变量列名（可选）' },
          continuous: { type: 'array', items: { type: 'string' }, description: '连续变量列名列表' },
          categorical: { type: 'array', items: { type: 'string' }, description: '分类变量列名列表' },
          dataset_id: { type: 'string', description: '数据集 ID' },
        },
        required: ['dataset_id'],
      },
      outputSchema: {
        type: 'object',
        properties: {
          strata: { type: ['string', 'null'] },
          groups: { type: 'array' },
          standardized_differences: { type: 'object' },
          categorical_tests: { type: 'array' },
          continuous_tests: { type: 'array' },
        },
      },
      invoker: { kind: 'algorithm', algorithmId: 'tableone' },
    });

    reg.register({
      skillId: 'ttest',
      displayName: 'T 检验',
      inputSchema: {
        type: 'object',
        properties: {
          group: { type: 'string', description: '二分类分组变量列名' },
          testVar: { type: 'string', description: '待检验连续变量列名' },
          alpha: { type: 'number', description: '显著性水平，默认 0.05' },
          dataset_id: { type: 'string', description: '数据集 ID' },
        },
        required: ['group', 'testVar', 'dataset_id'],
      },
      outputSchema: {
        type: 'object',
        properties: {
          method: { type: 'string' },
          group_variable: { type: 'string' },
          test_variable: { type: 'string' },
          p_value: { type: 'number' },
        },
      },
      invoker: { kind: 'algorithm', algorithmId: 'ttest' },
    });

    reg.register({
      skillId: 'anova',
      displayName: '单因素方差分析',
      inputSchema: {
        type: 'object',
        properties: {
          group: { type: 'string', description: '分组变量列名（≥2 组）' },
          testVar: { type: 'string', description: '待比较的连续变量列名' },
          dataset_id: { type: 'string', description: '数据集 ID' },
        },
        required: ['group', 'testVar', 'dataset_id'],
      },
      outputSchema: {
        type: 'object',
        properties: {
          method: { type: 'string' },
          f_statistic: { type: 'number' },
          p_value: { type: 'number' },
          eta_squared: { type: 'number' },
        },
      },
      invoker: { kind: 'algorithm', algorithmId: 'anova' },
    });

    reg.register({
      skillId: 'correlation',
      displayName: '相关分析',
      inputSchema: {
        type: 'object',
        properties: {
          x: { type: 'string', description: '连续变量 X 列名' },
          y: { type: 'string', description: '连续变量 Y 列名' },
          method: {
            type: 'string',
            description: '相关方法：pearson（默认）或 spearman',
          },
          dataset_id: { type: 'string', description: '数据集 ID' },
        },
        required: ['x', 'y', 'dataset_id'],
      },
      outputSchema: {
        type: 'object',
        properties: {
          method: { type: 'string' },
          r: { type: 'number' },
          p_value: { type: 'number' },
          n: { type: 'number' },
        },
      },
      invoker: { kind: 'algorithm', algorithmId: 'correlation' },
    });

    reg.register({
      skillId: 'model_linear',
      displayName: '线性回归',
      inputSchema: {
        type: 'object',
        properties: {
          outcome: { type: 'string', description: '因变量列名' },
          predictors: { type: 'array', items: { type: 'string' }, description: '自变量列名列表' },
          dataset_id: { type: 'string', description: '数据集 ID' },
        },
        required: ['outcome', 'predictors', 'dataset_id'],
      },
      outputSchema: {
        type: 'object',
        properties: {
          coefficients: { type: 'array' },
          r_squared: { type: 'number' },
          adj_r_squared: { type: 'number' },
          f_statistic: { type: 'number' },
          p_value: { type: 'number' },
          aic: { type: 'number' },
          model_diagnostics: { type: 'object' },
        },
      },
      invoker: { kind: 'algorithm', algorithmId: 'linear' },
    });

    reg.register({
      skillId: 'model_logistic',
      displayName: 'Logistic 回归',
      inputSchema: {
        type: 'object',
        properties: {
          outcome: { type: 'string', description: '因变量列名（二分类）' },
          predictors: { type: 'array', items: { type: 'string' }, description: '自变量列名列表' },
          dataset_id: { type: 'string', description: '数据集 ID' },
        },
        required: ['outcome', 'predictors', 'dataset_id'],
      },
      outputSchema: {
        type: 'object',
        properties: {
          coefficients: { type: 'array' },
          odds_ratios: { type: 'array' },
          p_values: { type: 'array' },
          aic: { type: 'number' },
          concordance: { type: 'number' },
          model_diagnostics: { type: 'object' },
        },
      },
      invoker: { kind: 'algorithm', algorithmId: 'logistic' },
    });

    reg.register({
      skillId: 'model_cox',
      displayName: 'Cox 回归',
      inputSchema: {
        type: 'object',
        properties: {
          time: { type: 'string', description: '时间变量列名' },
          event: { type: 'string', description: '事件变量列名' },
          predictors: { type: 'array', items: { type: 'string' }, description: '协变量列名列表' },
          dataset_id: { type: 'string', description: '数据集 ID' },
        },
        required: ['time', 'event', 'predictors', 'dataset_id'],
      },
      outputSchema: {
        type: 'object',
        properties: {
          coefficients: { type: 'array' },
          hazard_ratios: { type: 'array' },
          p_values: { type: 'array' },
          concordance: { type: 'number' },
          ph_test: { type: 'object' },
          cox_ph_violated: { type: 'boolean' },
          event_n: { type: 'number' },
          model_diagnostics: { type: 'object' },
        },
      },
      invoker: { kind: 'algorithm', algorithmId: 'cox' },
    });

    reg.register({
      skillId: 'survival_km',
      displayName: 'Kaplan-Meier 生存分析',
      inputSchema: {
        type: 'object',
        properties: {
          time: { type: 'string', description: '时间变量列名' },
          event: { type: 'string', description: '事件变量列名' },
          group: { type: 'string', description: '分组变量列名（可选）' },
          dataset_id: { type: 'string', description: '数据集 ID' },
        },
        required: ['time', 'event', 'dataset_id'],
      },
      outputSchema: {
        type: 'object',
        properties: {
          survival_table: { type: 'array' },
          median_survival: { type: 'number' },
          groups: { type: 'array' },
          steps: { type: 'array' },
          group_summaries: { type: 'array' },
          log_rank: { type: ['object', 'null'] },
          log_rank_p: { type: 'number' },
        },
      },
      invoker: { kind: 'algorithm', algorithmId: 'kaplan_meier' },
    });

    reg.register({
      skillId: 'power',
      displayName: '功效分析',
      inputSchema: {
        type: 'object',
        properties: {
          test_type: { type: 'string', description: '检验类型（如 ttest, anova, proportion）' },
          effect_size: { type: 'number', description: '效应量' },
          alpha: { type: 'number', description: '显著性水平', default: 0.05 },
          power: { type: 'number', description: '目标功效', default: 0.8 },
          n: { type: 'integer', description: '样本量（可选，用于计算功效）' },
        },
        required: ['test_type', 'effect_size'],
      },
      outputSchema: {
        type: 'object',
        properties: {
          required_n: { type: 'integer' },
          achieved_power: { type: 'number' },
          effect_size: { type: 'number' },
          alpha: { type: 'number' },
        },
      },
      invoker: { kind: 'native', run: nativeNotWired('power') },
    });

    reg.register({
      skillId: 'inspect',
      displayName: '数据探索',
      inputSchema: {
        type: 'object',
        properties: {
          dataset_id: { type: 'string', description: '数据集 ID' },
          columns: { type: 'array', items: { type: 'string' }, description: '要探索的列名列表（可选，默认全部）' },
        },
        required: ['dataset_id'],
      },
      outputSchema: {
        type: 'object',
        properties: {
          row_count: { type: 'integer' },
          columns: { type: 'array' },
          summary_statistics: { type: 'object' },
        },
      },
      invoker: { kind: 'native', run: nativeNotWired('inspect') },
    });

    return reg;
  }
}

/**
 * Placeholder native invoker. The SkillRunner owns the concrete native
 * implementations (power/inspect) and dispatches by skill id, so the registry
 * descriptors only need to declare the invoker kind. If a native invoker is
 * ever called directly without the runner, it surfaces a clear error.
 */
function nativeNotWired(
  skillId: string,
): (args: Record<string, unknown>, ctx: SkillContext) => Promise<SkillResult> {
  return () => Promise.reject(new Error(`native skill '${skillId}' must be run via SkillRunner`));
}
