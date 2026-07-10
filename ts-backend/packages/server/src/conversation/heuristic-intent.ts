// Heuristic intent recognition when the cloud LLM is unreachable.
// Covers the common Chinese / English analysis phrases used in the welcome
// cards and analysis templates so Simple mode stays usable offline.

import type { IntentResult } from './orchestrator.js';

interface Rule {
  skillId: string;
  pattern: RegExp;
}

const RULES: Rule[] = [
  { skillId: 'model_linear', pattern: /线性回归|多元线性|linear\s*reg|linreg/i },
  {
    skillId: 'model_logistic',
    pattern: /logistic|逻辑回归|二分类回归|logit/i,
  },
  {
    skillId: 'model_cox',
    pattern: /cox\s*(比例|回归|model)?|比例风险/i,
  },
  {
    skillId: 'survival_km',
    pattern: /kaplan|生存分析|生存曲线|km\s*生存|log[-\s]?rank/i,
  },
  {
    skillId: 'ttest',
    pattern: /t\s*检验|t检验|ttest|t-test|组间对比|双样本/i,
  },
  {
    skillId: 'tableone',
    pattern: /table\s*one|tableone|基线特征|基线表|描述性统计|三线表/i,
  },
];

/**
 * Map free-form user text to a skill intent without calling the LLM.
 * Returns null when no confident rule matches.
 */
export function heuristicIntent(userText: string): IntentResult | null {
  const text = userText.trim();
  if (!text) return null;

  for (const rule of RULES) {
    if (rule.pattern.test(text)) {
      return {
        skill_ids: [rule.skillId],
        resolved_args: {},
        has_query_intent: true,
        text_response: null,
      };
    }
  }

  // Generic “help / what can you do” without a skill.
  if (/你会什么|能做什么|帮助|help|功能/.test(text)) {
    return {
      skill_ids: [],
      resolved_args: {},
      has_query_intent: true,
      text_response:
        '我可以帮你做基线特征表、T 检验、线性/Logistic/Cox 回归和 Kaplan-Meier 生存分析。' +
        '也可以切换到「专业」模式用可视化配置直接跑统计引擎。',
    };
  }

  return null;
}
