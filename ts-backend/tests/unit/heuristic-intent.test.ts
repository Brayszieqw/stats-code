import { describe, it, expect } from 'vitest';
import { heuristicIntent } from '@stats-code/server';

describe('heuristicIntent', () => {
  it('maps 线性回归 to model_linear', () => {
    const r = heuristicIntent('帮我做线性回归');
    expect(r?.skill_ids).toEqual(['model_linear']);
  });

  it('maps logistic / 逻辑回归', () => {
    expect(heuristicIntent('Logistic 回归')?.skill_ids).toEqual(['model_logistic']);
    expect(heuristicIntent('逻辑回归风险因素')?.skill_ids).toEqual(['model_logistic']);
  });

  it('maps survival and cox', () => {
    expect(heuristicIntent('Kaplan-Meier 生存分析')?.skill_ids).toEqual(['survival_km']);
    expect(heuristicIntent('Cox 比例风险回归')?.skill_ids).toEqual(['model_cox']);
  });

  it('maps t-test and tableone', () => {
    expect(heuristicIntent('做一下 T 检验')?.skill_ids).toEqual(['ttest']);
    expect(heuristicIntent('生成基线特征表')?.skill_ids).toEqual(['tableone']);
  });

  it('returns help text for capability questions', () => {
    const r = heuristicIntent('你会什么');
    expect(r?.skill_ids).toEqual([]);
    expect(r?.text_response).toMatch(/专业/);
  });

  it('returns null for unrelated chatter', () => {
    expect(heuristicIntent('今天天气不错')).toBeNull();
  });
});
