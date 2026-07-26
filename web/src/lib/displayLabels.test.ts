import { describe, it, expect } from 'vitest';
import { columnTypeLabel, humanizeIdentifier, methodShortLabel } from './displayLabels';

describe('methodShortLabel', () => {
  it('maps every registered skill id to a Chinese label', () => {
    // 与 ts-backend skill-registry.ts 注册的 skillId 一一对应
    const ids = [
      'tableone', 'ttest', 'anova', 'correlation',
      'model_linear', 'model_logistic', 'model_cox',
      'survival_km', 'power', 'inspect',
    ];
    for (const id of ids) {
      const label = methodShortLabel(id);
      expect(label).not.toBe(id);
      expect(label).not.toMatch(/_/); // 界面上不得出现下划线标识符
    }
  });

  it('maps the algorithm_id spelling the server actually returns', () => {
    // 服务端 analysis.algorithm_id 走 skill-algorithm-map.ts，与 skill_id 不同：
    // model_linear → linear、survival_km → kaplan_meier。只收 skill_id 时
    // 方法胶囊会 humanize 成 'Linear'，再被 text-transform:uppercase 变成
    // 真机上看到的「LINEAR」。
    expect(methodShortLabel('linear')).toBe('线性回归');
    expect(methodShortLabel('logistic')).toBe('Logistic 回归');
    expect(methodShortLabel('cox')).toBe('Cox 回归');
    expect(methodShortLabel('kaplan_meier')).toBe('KM 生存分析');
  });

  it('agrees between the skill_id and algorithm_id spelling of the same method', () => {
    for (const [skillId, algorithmId] of [
      ['model_linear', 'linear'],
      ['model_logistic', 'logistic'],
      ['model_cox', 'cox'],
      ['survival_km', 'kaplan_meier'],
    ]) {
      expect(methodShortLabel(skillId!)).toBe(methodShortLabel(algorithmId!));
    }
  });

  it('never falls back to a bare latin word for a known method', () => {
    // .pro-method-tag 有 text-transform: uppercase，纯拉丁兜底会显示成
    // 「LINEAR」这种全大写英文。专有名词（Logistic/Cox/KM）保留原拼写，
    // 但必须带中文词缀，说明它是被映射过的而不是兜底结果。
    for (const id of ['linear', 'logistic', 'cox', 'kaplan_meier', 'tableone', 'anova', 'correlation', 'power']) {
      const label = methodShortLabel(id);
      expect(label).toMatch(/[一-鿿]/);
      expect(label).not.toMatch(/_/);
    }
  });

  it('humanizes an unregistered id instead of leaking snake_case', () => {
    expect(methodShortLabel('model_poisson')).toBe('Model poisson');
    expect(methodShortLabel('model_poisson')).not.toMatch(/_/);
  });

  it('survives a missing algorithm_id rather than throwing', () => {
    // 契约上 algorithm_id 必填，但历史会话里实测存在缺该字段的 analysis 记录；
    // 旧代码 analysis.algorithm_id.replace(...) 会让 ProModeView 整页白屏。
    expect(() => methodShortLabel(undefined)).not.toThrow();
    expect(methodShortLabel(undefined)).toBe('未知');
    expect(methodShortLabel(null)).toBe('未知');
    expect(methodShortLabel('')).toBe('未知');
    expect(methodShortLabel('   ')).toBe('未知');
  });
});

describe('columnTypeLabel', () => {
  it('maps contract column types to Chinese without English parentheses', () => {
    expect(columnTypeLabel('Numeric')).toBe('数值');
    expect(columnTypeLabel('Categorical')).toBe('分类');
    expect(columnTypeLabel('Date')).toBe('日期');
    expect(columnTypeLabel('String')).toBe('文本');
    for (const type of ['Numeric', 'Categorical', 'Date', 'String']) {
      expect(columnTypeLabel(type)).not.toMatch(/[A-Za-z]/);
    }
  });

  it('survives an absent type', () => {
    expect(() => columnTypeLabel(undefined)).not.toThrow();
    expect(columnTypeLabel(undefined)).toBe('未知');
  });
});

describe('humanizeIdentifier', () => {
  it('replaces underscores and capitalizes', () => {
    expect(humanizeIdentifier('some_new_thing')).toBe('Some new thing');
  });

  it('leaves an already-humane string readable', () => {
    expect(humanizeIdentifier('power')).toBe('Power');
  });

  it('returns a placeholder for non-string input', () => {
    expect(humanizeIdentifier(undefined)).toBe('未知');
    expect(humanizeIdentifier(42)).toBe('未知');
  });
});
