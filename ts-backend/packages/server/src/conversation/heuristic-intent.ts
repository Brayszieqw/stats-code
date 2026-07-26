// Heuristic intent recognition when the cloud LLM is unreachable.
// Covers the common Chinese / English analysis phrases used in the welcome
// cards and analysis templates so Simple mode stays usable offline.
// Also extracts obvious column names from free text so missing-arg prompts
// are skipped when the user already named variables.

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
    skillId: 'anova',
    pattern: /anova|方差分析|单因素方差|one[-\s]?way/i,
  },
  {
    skillId: 'correlation',
    pattern: /相关分析|相关性|pearson|spearman|相关系数|correlation/i,
  },
  {
    skillId: 'tableone',
    pattern: /table\s*one|tableone|基线特征|基线表|描述性统计|三线表/i,
  },
  {
    skillId: 'inspect',
    pattern: /数据概览|查看变量|列名|inspect|变量类型|摸底/i,
  },
  {
    skillId: 'power',
    pattern: /功效|样本量|sample\s*size|power\s*analysis/i,
  },
];

const IDENT = '[A-Za-z_][\\w.]*';

/**
 * Pull common analysis arguments out of free-form Chinese/English text.
 * Conservative: only Latin-ish identifiers that look like column names.
 */
export function extractArgsFromText(userText: string): Record<string, unknown> {
  const text = userText.trim();
  if (!text) return {};
  const args: Record<string, unknown> = {};

  const outcome = text.match(
    new RegExp(`(?:结局|因变量|outcome)\\s*(?:变量)?\\s*[：:=是为]?\\s*[\`「『"']?(${IDENT})`, 'i'),
  );
  if (outcome?.[1]) args.outcome = outcome[1];

  const group = text.match(
    new RegExp(`(?:分组|group)\\s*(?:变量)?\\s*[：:=是为]?\\s*[\`「『"']?(${IDENT})`, 'i'),
  );
  if (group?.[1]) args.group = group[1];

  const testVar = text.match(
    new RegExp(`(?:检验变量|test\\s*var(?:iable)?)\\s*[：:=是为]?\\s*[\`「『"']?(${IDENT})`, 'i'),
  );
  if (testVar?.[1]) args.testVar = testVar[1];

  const time = text.match(
    new RegExp(`(?:时间|time|随访时间)\\s*(?:变量)?\\s*[：:=是为]?\\s*[\`「『"']?(${IDENT})`, 'i'),
  );
  if (time?.[1]) args.time = time[1];

  const event = text.match(
    new RegExp(`(?:事件|event|删失指示|结局指示)\\s*(?:变量)?\\s*[：:=是为]?\\s*[\`「『"']?(${IDENT})`, 'i'),
  );
  if (event?.[1]) args.event = event[1];

  const xCol = text.match(
    new RegExp(`(?:变量\\s*[xX]|x\\s*变量|自变量\\s*[xX])\\s*[：:=是为]?\\s*[\`「『"']?(${IDENT})`, 'i'),
  );
  if (xCol?.[1]) args.x = xCol[1];
  const yCol = text.match(
    new RegExp(`(?:变量\\s*[yY]|y\\s*变量|因变量\\s*[yY])\\s*[：:=是为]?\\s*[\`「『"']?(${IDENT})`, 'i'),
  );
  if (yCol?.[1]) args.y = yCol[1];
  // "相关分析 age 与 bmi" / "correlation between age and bmi"
  const pair = text.match(
    new RegExp(`(?:相关|correlation)[^A-Za-z_0-9]{0,12}(${IDENT})\\s*(?:与|和|and|,|，)\\s*(${IDENT})`, 'i'),
  );
  if (pair?.[1] && pair[2]) {
    if (args.x === undefined) args.x = pair[1];
    if (args.y === undefined) args.y = pair[2];
  }
  if (/spearman/i.test(text)) args.method = 'spearman';
  if (/pearson/i.test(text)) args.method = 'pearson';

  const pred = text.match(
    new RegExp(
      `(?:预测|自变|协变|predictors?)\\s*(?:变量)?\\s*[：:=是为]?\\s*[\`「『"']?([A-Za-z_][\\w.\\s,，、]*)`,
      'i',
    ),
  );
  if (pred?.[1]) {
    const list = pred[1]
      .split(/[,，、\s]+/)
      .map((s) => s.trim())
      .filter((s) => new RegExp(`^${IDENT}$`).test(s));
    if (list.length > 0) args.predictors = list;
  }

  return args;
}

/** Fill only keys that the model/heuristic left empty. */
export function mergeMissingArgs(
  base: Record<string, unknown>,
  extra: Record<string, unknown>,
): Record<string, unknown> {
  const out: Record<string, unknown> = { ...base };
  for (const [key, value] of Object.entries(extra)) {
    if (out[key] === undefined || out[key] === null) {
      out[key] = value;
    }
  }
  return out;
}

/**
 * Map free-form user text to a skill intent without calling the LLM.
 * Returns null when no confident rule matches.
 */
export function heuristicIntent(userText: string): IntentResult | null {
  const text = userText.trim();
  if (!text) return null;

  const extracted = extractArgsFromText(text);

  for (const rule of RULES) {
    if (rule.pattern.test(text)) {
      return {
        skill_ids: [rule.skillId],
        resolved_args: extracted,
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
        '我可以帮你做基线特征表、T 检验、方差分析、相关分析、线性/Logistic/Cox 回归和 Kaplan-Meier 生存分析。' +
        '也可以切换到「专业」模式用可视化配置直接跑统计引擎。',
    };
  }

  return null;
}
