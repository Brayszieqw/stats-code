/**
 * Preset suggestion prompts for the simple-mode welcome screen (R2.4).
 */

export interface SuggestionPrompt {
  icon: string;
  title: string;
  prompt: string;
}

export const SUGGESTION_PROMPTS: SuggestionPrompt[] = [
  { icon: '📈', title: '线性回归', prompt: '帮我分析血压与年龄的关系' },
  { icon: '📊', title: 'Logistic 回归', prompt: '分析风险因素对某事件的影响' },
  { icon: '⏱️', title: '生存分析', prompt: '比较不同治疗方案的生存率差异' },
  { icon: '⚡', title: '功效分析', prompt: '估算检测出目标效应所需的样本量' },
];
