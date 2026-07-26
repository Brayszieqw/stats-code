import { Fragment } from 'react';
import { Typography } from 'antd';
import type { SkillResult } from '../api/types';
import {
  fmtNum,
  fmtP,
  normalizeCoefficients,
  termHintsFromAnalysis,
} from '../lib/coeffFields';

const { Text } = Typography;

export interface ThreeLineTableProps {
  /** If rendering standard Markdown-formatted tables */
  markdown?: string;
  /** If rendering structured SkillResult payloads */
  skillResult?: SkillResult;
  /**
   * 变量名筛选关键词（由 StatsTable 工具条收集）。空串或省略即不过滤。
   * 只匹配变量名，不匹配单元格数值——按数值筛统计表没有语义，
   * 且会把同一变量的分类水平行与其标题行拆散。
   */
  filterKeyword?: string;
}

/** 变量名是否命中筛选词（大小写不敏感；空词恒真）。 */
function matchesKeyword(variable: string, keyword: string): boolean {
  if (!keyword) return true;
  return variable.toLowerCase().includes(keyword.toLowerCase());
}

/**
 * Parses a markdown string table into rows and headers.
 */
function parseMarkdownTable(markdown: string) {
  const lines = markdown
    .split('\n')
    .map((l) => l.trim())
    .filter((l) => l.startsWith('|'));

  if (lines.length < 2) return null;

  // Split cell values, trimming whitespace
  const getCells = (line: string) => {
    return line
      .split('|')
      .slice(1, -1) // remove empty elements from starting and ending '|'
      .map((c) => c.trim());
  };

  const headers = getCells(lines[0]!);

  // Check if second line is a delimiter line (e.g. |---|---|)
  const hasDelimiter = lines[1]!.replace(/[\s|-|:|]/g, '').length === 0;
  const dataLines = hasDelimiter ? lines.slice(2) : lines.slice(1);

  const rows = dataLines.map((line) => getCells(line));

  return { headers, rows };
}

type ContinuousSummary = {
  variable: string;
  type?: string;
  n?: number;
  missing?: number;
  mean?: number;
  sd?: number;
  median?: number;
  q1?: number;
  q3?: number;
};

type CategoricalSummary = {
  variable: string;
  type?: string;
  n?: number;
  missing?: number;
  levels?: { level: string; count: number; percent: number }[];
};

type TableOneGroup = {
  label: string;
  n: number;
  continuous?: ContinuousSummary[];
  categorical?: CategoricalSummary[];
};

type TableOneStandardizedDifferences = {
  comparison: { first: string; second: string } | null;
  continuous: Array<{ variable: string; smd: number | null }>;
  categorical: Array<{
    variable: string;
    smd: number | null;
    levels: Array<{ level: string; smd: number | null }>;
  }>;
};

type TableOneCategoricalTest = {
  variable: string;
  status: 'computed' | 'not_computed' | 'not_applicable';
  method: 'pearson_chi_square' | 'fisher_exact' | null;
  statistic: number | null;
  degrees_of_freedom: number | null;
  p_value: number | null;
  min_expected_count: number | null;
  expected_below_5: number;
  observed_zero_cells: number;
  reason: string | null;
};

/** Continuous-variable group-difference test (Welch t / one-way ANOVA). */
type TableOneContinuousTest = {
  variable: string;
  status: 'computed' | 'not_computed' | 'not_applicable';
  method: 'welch_t' | 'one_way_anova' | null;
  statistic: number | null;
  degrees_of_freedom: number | null;
  degrees_of_freedom_denominator: number | null;
  p_value: number | null;
  groups: string[];
  group_ns: number[];
  degenerate: boolean;
  reason: string | null;
};

function formatContinuous(summary: ContinuousSummary | undefined): string {
  if (!summary) return '-';
  if (typeof summary.mean === 'number' && Number.isFinite(summary.mean)) {
    const meanSd = `${fmtNum(summary.mean)} ± ${fmtNum(summary.sd ?? null)}`;
    if (typeof summary.median === 'number' && Number.isFinite(summary.median)) {
      return `${meanSd}; 中位 ${fmtNum(summary.median)} (${fmtNum(summary.q1 ?? null)}–${fmtNum(summary.q3 ?? null)})`;
    }
    return meanSd;
  }
  return '-';
}

function formatLevel(level: { count: number; percent: number } | undefined): string {
  if (!level) return '-';
  return `${level.count} (${fmtNum(level.percent, 1)}%)`;
}

function formatCountMeta(summary: { n?: number; missing?: number } | undefined): string {
  if (!summary) return '有效 n=- · 缺失=-';
  return `有效 n=${summary.n ?? '-'} · 缺失=${summary.missing ?? '-'}`;
}

function formatSmd(value: number | null | undefined): string {
  return typeof value === 'number' && Number.isFinite(value) ? fmtNum(value) : '-';
}

function formatCategoricalTest(test: TableOneCategoricalTest | undefined): string | null {
  if (!test || test.status === 'not_applicable') return null;
  if (test.status === 'not_computed') return `组间检验未计算：${test.reason ?? '当前数据不满足方法要求。'}`;
  const method = test.method === 'fisher_exact' ? 'Fisher 精确检验' : 'Pearson χ² 检验';
  const zeroCell = test.observed_zero_cells > 0 ? ` · 零频单元 ${test.observed_zero_cells}` : '';
  return `${method}${zeroCell}`;
}

/** Method-label meta line for a continuous row (p-value itself renders in the dedicated p column). */
function formatContinuousTestMeta(test: TableOneContinuousTest | undefined): string | null {
  if (!test || test.status === 'not_applicable') return null;
  if (test.status === 'not_computed') return `组间检验未计算：${test.reason ?? '当前数据不满足方法要求。'}`;
  return test.method === 'one_way_anova' ? '单因素 ANOVA' : 'Welch t 检验';
}

/** Engine tableone payload: { strata, continuous, categorical, groups[] } */
function renderGroupsTableOne(payload: {
  strata?: string | null;
  continuous?: string[];
  categorical?: string[];
  groups: TableOneGroup[];
  standardized_differences?: TableOneStandardizedDifferences;
  categorical_tests?: TableOneCategoricalTest[];
  continuous_tests?: TableOneContinuousTest[];
}, filterKeyword = '') {
  const groups = payload.groups;
  if (!Array.isArray(groups) || groups.length === 0) return null;

  const first = groups[0]!;
  const allContinuousNames =
    first.continuous?.map((c) => c.variable) ??
    (Array.isArray(payload.continuous) ? payload.continuous : []);
  const allCategoricalNames =
    first.categorical?.map((c) => c.variable) ??
    (Array.isArray(payload.categorical) ? payload.categorical : []);

  // 筛选在变量层面做，分类变量的水平行随其标题行一起去留（水平名如 male/never
  // 不参与匹配，否则会出现「标题行被筛掉但水平行还在」的孤儿行）。
  const continuousNames = allContinuousNames.filter((name) => matchesKeyword(name, filterKeyword));
  const categoricalNames = allCategoricalNames.filter((name) => matchesKeyword(name, filterKeyword));
  const hiddenCount =
    (allContinuousNames.length - continuousNames.length)
    + (allCategoricalNames.length - categoricalNames.length);

  const findContinuous = (group: TableOneGroup, variable: string) =>
    group.continuous?.find((c) => c.variable === variable);
  const findCategorical = (group: TableOneGroup, variable: string) =>
    group.categorical?.find((c) => c.variable === variable);
  const standardizedDifferences = payload.standardized_differences;
  const showSmd = standardizedDifferences?.comparison !== null
    && standardizedDifferences?.comparison !== undefined;
  const findContinuousSmd = (variable: string) =>
    standardizedDifferences?.continuous.find((entry) => entry.variable === variable)?.smd;
  const findCategoricalSmd = (variable: string) =>
    standardizedDifferences?.categorical.find((entry) => entry.variable === variable);
  const findCategoricalTest = (variable: string) =>
    payload.categorical_tests?.find((entry) => entry.variable === variable);
  const findContinuousTest = (variable: string) =>
    payload.continuous_tests?.find((entry) => entry.variable === variable);

  // Collect all categorical levels across groups for stable row layout.
  const levelsByVar = new Map<string, string[]>();
  for (const name of categoricalNames) {
    const levelSet = new Set<string>();
    for (const group of groups) {
      const summary = findCategorical(group, name);
      for (const level of summary?.levels ?? []) {
        levelSet.add(level.level);
      }
    }
    levelsByVar.set(name, [...levelSet]);
  }

  const strataLabel = payload.strata ? `按 ${payload.strata} 分层` : '总体描述';
  const hasContinuousTests = Array.isArray(payload.continuous_tests) && payload.continuous_tests.length > 0;
  const hasCategoricalTests = Array.isArray(payload.categorical_tests) && payload.categorical_tests.length > 0;
  const showPColumn = hasContinuousTests || hasCategoricalTests;

  return (
    <div className="three-line-table-scroll">
      <table className="three-line-table three-line-table--grouped" aria-label="Table One 三线表">
        <thead>
          <tr>
            <th className="tlt-corner">特征变量</th>
            {groups.map((group) => (
              <th key={group.label} style={{ textAlign: 'center' }}>
                {group.label} (N={group.n})
              </th>
            ))}
            {showSmd ? <th style={{ textAlign: 'center' }}>SMD</th> : null}
            {showPColumn ? <th style={{ textAlign: 'right' }}>p 值</th> : null}
          </tr>
          <tr>
            <th colSpan={1 + groups.length + (showSmd ? 1 : 0) + (showPColumn ? 1 : 0)} style={{ fontWeight: 500, color: '#5a6e85', borderBottom: '1px solid #2b3b4c' }}>
              {strataLabel}
            </th>
          </tr>
        </thead>
        <tbody>
          {continuousNames.map((variable) => {
            const continuousTest = findContinuousTest(variable);
            const testMeta = formatContinuousTestMeta(continuousTest);
            const isComputed = continuousTest?.status === 'computed';
            const pSig = isComputed && typeof continuousTest!.p_value === 'number' && continuousTest!.p_value < 0.05;
            return (
              <tr key={`c-${variable}`}>
                <td>
                  <Text strong>{variable}</Text>
                  <div style={{ fontSize: 11, color: 'var(--ink-300)' }}>连续 · 均值±SD；中位数 (Q1–Q3)</div>
                  {testMeta ? <div className="table-one-cell-meta">{testMeta}</div> : null}
                </td>
                {groups.map((group) => (
                  <td key={`${group.label}-${variable}`} style={{ textAlign: 'center', fontFamily: 'ui-monospace, monospace', fontSize: 13 }}>
                    {formatContinuous(findContinuous(group, variable))}
                    <div className="table-one-cell-meta">{formatCountMeta(findContinuous(group, variable))}</div>
                  </td>
                ))}
                {showSmd ? (
                  <td className="table-one-smd">{formatSmd(findContinuousSmd(variable))}</td>
                ) : null}
                {showPColumn ? (
                  <td className={pSig ? 'sig' : undefined} style={{ textAlign: 'right' }}>
                    {isComputed ? fmtP(continuousTest!.p_value) : '-'}
                    {isComputed && continuousTest!.degenerate ? (
                      <span
                        className="table-one-degenerate-flag"
                        title={continuousTest!.reason ?? '检验统计量或 p 值数值退化，结果不可解释。'}
                      >
                        {' '}⚠
                      </span>
                    ) : null}
                  </td>
                ) : null}
              </tr>
            );
          })}
          {categoricalNames.map((variable) => {
            const levels = levelsByVar.get(variable) ?? [];
            const categoricalTest = findCategoricalTest(variable);
            const catTestMeta = formatCategoricalTest(categoricalTest);
            const catIsComputed = categoricalTest?.status === 'computed';
            const catPSig = catIsComputed && typeof categoricalTest!.p_value === 'number' && categoricalTest!.p_value < 0.05;
            return (
              <Fragment key={`cat-block-${variable}`}>
                <tr>
                  <td>
                    <Text strong>{variable}</Text>
                    <span style={{ marginLeft: 8, fontSize: 11, color: 'var(--ink-300)' }}>分类 · n (%)</span>
                    {catTestMeta ? (
                      <div className="table-one-cell-meta">
                        {catTestMeta}
                      </div>
                    ) : null}
                  </td>
                  {groups.map((group) => (
                    <td key={`${group.label}-${variable}-meta`} className="table-one-cell-meta table-one-cell-meta--standalone">
                      {formatCountMeta(findCategorical(group, variable))}
                    </td>
                  ))}
                  {showSmd ? (
                    <td className="table-one-smd table-one-smd--summary">
                      最大 |SMD| {formatSmd(findCategoricalSmd(variable)?.smd)}
                    </td>
                  ) : null}
                  {showPColumn ? (
                    <td className={catPSig ? 'sig' : undefined} style={{ textAlign: 'right' }}>
                      {catIsComputed ? fmtP(categoricalTest!.p_value) : '-'}
                    </td>
                  ) : null}
                </tr>
                {levels.map((level) => (
                  <tr key={`cat-${variable}-${level}`}>
                    <td style={{ paddingLeft: 24, color: '#5a6e85' }}>{level}</td>
                    {groups.map((group) => {
                      const summary = findCategorical(group, variable);
                      const cell = summary?.levels?.find((l) => l.level === level);
                      return (
                        <td key={`${group.label}-${variable}-${level}`} style={{ textAlign: 'center', fontFamily: 'ui-monospace, monospace', fontSize: 13 }}>
                          {formatLevel(cell)}
                        </td>
                      );
                    })}
                    {showSmd ? (
                      <td className="table-one-smd">
                        {formatSmd(findCategoricalSmd(variable)?.levels.find((entry) => entry.level === level)?.smd)}
                      </td>
                    ) : null}
                    {showPColumn ? <td /> : null}
                  </tr>
                ))}
              </Fragment>
            );
          })}
          {continuousNames.length === 0 && categoricalNames.length === 0 ? (
            <tr>
              <td colSpan={1 + groups.length + (showSmd ? 1 : 0) + (showPColumn ? 1 : 0)} style={{ textAlign: 'center', color: 'var(--ink-300)' }}>
                没有变量名匹配「{filterKeyword}」
              </td>
            </tr>
          ) : null}
        </tbody>
      </table>
      {hiddenCount > 0 ? (
        <p className="table-one-footnote">
          筛选「{filterKeyword}」中，已隐藏 {hiddenCount} 个变量；导出的审计快照仍包含全部变量。
        </p>
      ) : null}
      {showPColumn ? (
        <p className="table-one-footnote">
          注：p 值仅用于描述性组间比较，未针对多重检验进行校正；数据是否满足正态性、方差齐性等前提未自动检验，结论解读请以研究者的专业判断为准。
        </p>
      ) : null}
    </div>
  );
}

/**
 * One-way ANOVA payload → 标准方差分析表（组间/组内/总计 × SS/df/MS/F/p）。
 *
 * 后端 skill-runner 的 `anova` 分支返回扁平字段（ss_between/df_within/…），
 * 既没有 `coefficients` 也没有 markdown，此前会落到函数末尾返回 null——
 * ReportViewer 仍会渲染 StatsTable 外壳，用户看到的是一个只有标题的空壳，
 * F 值、自由度与 η² 全部丢失。这里补上专用分支。
 */
function renderAnova(payload: Record<string, unknown>) {
  const num = (key: string): number | null => {
    const value = payload[key];
    return typeof value === 'number' && Number.isFinite(value) ? value : null;
  };
  const f = num('f_statistic');
  const p = num('p_value');
  const dfB = num('df_between');
  const dfW = num('df_within');
  const ssB = num('ss_between');
  const ssW = num('ss_within');
  const ssT = num('ss_total');
  const msB = num('ms_between');
  const msW = num('ms_within');
  const eta = num('eta_squared');
  const groupVar = typeof payload.group_variable === 'string' ? payload.group_variable : null;
  const testVar = typeof payload.test_variable === 'string' ? payload.test_variable : null;
  const degenerate = payload.degenerate === true;

  const rows: Array<{ source: string; ss: number | null; df: number | null; ms: number | null; f: number | null; p: number | null }> = [
    { source: `组间（${groupVar ?? '分组'}）`, ss: ssB, df: dfB, ms: msB, f, p },
    { source: '组内（误差）', ss: ssW, df: dfW, ms: msW, f: null, p: null },
    { source: '总计', ss: ssT, df: dfB !== null && dfW !== null ? dfB + dfW : null, ms: null, f: null, p: null },
  ];

  return (
    <div className="three-line-table-scroll">
      <table className="three-line-table three-line-table--regression" aria-label="单因素方差分析表">
        <thead>
          <tr>
            <th className="tlt-corner">变异来源</th>
            <th style={{ textAlign: 'right' }}>平方和 (SS)</th>
            <th style={{ textAlign: 'right' }}>自由度 (df)</th>
            <th style={{ textAlign: 'right' }}>均方 (MS)</th>
            <th style={{ textAlign: 'right' }}>F</th>
            <th style={{ textAlign: 'right' }}>p 值</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((row) => (
            <tr key={row.source}>
              <td><Text strong>{row.source}</Text></td>
              <td style={{ textAlign: 'right', fontFamily: 'ui-monospace, monospace' }}>{fmtNum(row.ss)}</td>
              <td style={{ textAlign: 'right', fontFamily: 'ui-monospace, monospace' }}>{row.df ?? '-'}</td>
              <td style={{ textAlign: 'right', fontFamily: 'ui-monospace, monospace' }}>{row.ms !== null ? fmtNum(row.ms) : '-'}</td>
              <td style={{ textAlign: 'right', fontFamily: 'ui-monospace, monospace', fontWeight: row.f !== null ? 600 : 400 }}>
                {row.f !== null ? fmtNum(row.f) : '-'}
              </td>
              <td
                className={row.p !== null && row.p < 0.05 ? 'sig' : undefined}
                style={{ textAlign: 'right' }}
              >
                {row.p !== null ? fmtP(row.p) : '-'}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      <p className="table-one-footnote">
        因变量 {testVar ?? '-'}，按 {groupVar ?? '-'} 分为 {typeof payload.k === 'number' ? payload.k : '-'} 组，
        有效观测 n={typeof payload.n_total === 'number' ? payload.n_total : '-'}
        {eta !== null ? ` · 效应量 η²=${fmtNum(eta)}` : ''}
        {degenerate ? ' · ⚠ 统计量数值退化，结果不可解释' : ''}
        。ANOVA 仅检验「各组均值是否全部相等」，未做事后两两比较；正态性与方差齐性未自动检验。
      </p>
    </div>
  );
}

/**
 * Welch / 双样本 t 检验 payload → 组描述 + 检验统计量表。
 *
 * 后端 `runTtest`（skill-runner.ts:1004）返回 groups[{label,n,mean}] 与扁平的
 * mean_diff / t_statistic / df / ci_lower / ci_upper。groups 里没有
 * continuous/categorical，因此不会被 Table One 分支接走；但此前也没有专用分支，
 * 结果 UI 只剩指标卡的 p 与 n —— 真机验收记为「t 报告不完整」（缺 t、df、
 * 均值差与置信区间）。
 */
function renderTtest(payload: Record<string, unknown>) {
  const num = (key: string): number | null => {
    const value = payload[key];
    return typeof value === 'number' && Number.isFinite(value) ? value : null;
  };
  const groups = (Array.isArray(payload.groups) ? payload.groups : []) as Array<Record<string, unknown>>;
  const groupVar = typeof payload.group_variable === 'string' ? payload.group_variable : null;
  const testVar = typeof payload.test_variable === 'string' ? payload.test_variable : null;
  const meanDiff = num('mean_diff');
  const t = num('t_statistic');
  const df = num('df');
  const p = num('p_value');
  const ciLower = num('ci_lower');
  const ciUpper = num('ci_upper');
  const alpha = num('alpha');
  const ciLabel = alpha !== null ? `${fmtNum((1 - alpha) * 100, 0)}% 置信区间` : '95% 置信区间';

  return (
    <div className="three-line-table-scroll">
      <table className="three-line-table three-line-table--regression" aria-label="t 检验结果表">
        <thead>
          <tr>
            <th className="tlt-corner">统计量</th>
            <th style={{ textAlign: 'right' }}>数值</th>
          </tr>
        </thead>
        <tbody>
          {groups.map((group) => {
            const label = typeof group.label === 'string' ? group.label : String(group.label ?? '-');
            const n = typeof group.n === 'number' ? group.n : null;
            const mean = typeof group.mean === 'number' ? group.mean : null;
            return (
              <tr key={`g-${label}`}>
                <td><Text strong>{label}</Text> 组均值</td>
                <td style={{ textAlign: 'right', fontFamily: 'ui-monospace, monospace' }}>
                  {fmtNum(mean)}
                  <span className="table-one-cell-meta"> （n={n ?? '-'}）</span>
                </td>
              </tr>
            );
          })}
          <tr>
            <td><Text strong>均值差</Text></td>
            <td style={{ textAlign: 'right', fontFamily: 'ui-monospace, monospace', fontWeight: 600 }}>{fmtNum(meanDiff)}</td>
          </tr>
          <tr>
            <td><Text strong>{ciLabel}</Text></td>
            <td style={{ textAlign: 'right', fontFamily: 'ui-monospace, monospace' }}>
              {ciLower !== null && ciUpper !== null ? `[${fmtNum(ciLower)}, ${fmtNum(ciUpper)}]` : '-'}
            </td>
          </tr>
          <tr>
            <td><Text strong>t 统计量</Text></td>
            <td style={{ textAlign: 'right', fontFamily: 'ui-monospace, monospace' }}>{fmtNum(t)}</td>
          </tr>
          <tr>
            <td><Text strong>自由度 (df)</Text></td>
            <td style={{ textAlign: 'right', fontFamily: 'ui-monospace, monospace' }}>{fmtNum(df)}</td>
          </tr>
          <tr>
            <td><Text strong>p 值</Text></td>
            <td className={p !== null && p < 0.05 ? 'sig' : undefined} style={{ textAlign: 'right' }}>{fmtP(p)}</td>
          </tr>
        </tbody>
      </table>
      <p className="table-one-footnote">
        {testVar ?? '-'} 按 {groupVar ?? '-'} 分两组比较
        {typeof payload.method === 'string' ? ` · ${payload.method}` : ''}
        。Welch 检验不假定方差齐性；正态性未自动检验，小样本请结合专业判断。
      </p>
    </div>
  );
}

/**
 * 相关分析 payload → 相关系数、置信区间与检验统计量表。
 *
 * 后端 `case 'correlation'`（skill-runner.ts:1164）返回 r / t_statistic / df /
 * ci_lower / ci_upper，但 UI 此前只显示 p 与 n，且提示「暂无可用图表」——
 * 真机验收记为「相关报告不完整」。Spearman 时系数按 ρ 标注。
 */
function renderCorrelation(payload: Record<string, unknown>) {
  const num = (key: string): number | null => {
    const value = payload[key];
    return typeof value === 'number' && Number.isFinite(value) ? value : null;
  };
  const methodRaw = typeof payload.method === 'string' ? payload.method : '';
  const isSpearman = methodRaw.toLowerCase().includes('spearman');
  const coefficientLabel = isSpearman ? 'Spearman ρ' : 'Pearson r';
  const xName = typeof payload.x === 'string' ? payload.x : '-';
  const yName = typeof payload.y === 'string' ? payload.y : '-';
  const r = num('r');
  const p = num('p_value');
  const t = num('t_statistic');
  const df = num('df');
  const ciLower = num('ci_lower');
  const ciUpper = num('ci_upper');
  const n = num('n');
  const alpha = num('alpha');
  const ciLabel = alpha !== null ? `${fmtNum((1 - alpha) * 100, 0)}% 置信区间` : '95% 置信区间';
  // 决定系数只对 Pearson 有「可解释方差比例」的含义；Spearman 的 ρ² 不是 R²。
  const rSquared = !isSpearman && r !== null ? r * r : null;

  return (
    <div className="three-line-table-scroll">
      <table className="three-line-table three-line-table--regression" aria-label="相关分析结果表">
        <thead>
          <tr>
            <th className="tlt-corner">统计量</th>
            <th style={{ textAlign: 'right' }}>数值</th>
          </tr>
        </thead>
        <tbody>
          <tr>
            <td><Text strong>{coefficientLabel}</Text></td>
            <td style={{ textAlign: 'right', fontFamily: 'ui-monospace, monospace', fontWeight: 600 }}>{fmtNum(r)}</td>
          </tr>
          <tr>
            <td><Text strong>{ciLabel}</Text></td>
            <td style={{ textAlign: 'right', fontFamily: 'ui-monospace, monospace' }}>
              {ciLower !== null && ciUpper !== null ? `[${fmtNum(ciLower)}, ${fmtNum(ciUpper)}]` : '-'}
            </td>
          </tr>
          {rSquared !== null ? (
            <tr>
              <td><Text strong>决定系数 R²</Text></td>
              <td style={{ textAlign: 'right', fontFamily: 'ui-monospace, monospace' }}>{fmtNum(rSquared)}</td>
            </tr>
          ) : null}
          <tr>
            <td><Text strong>t 统计量</Text></td>
            <td style={{ textAlign: 'right', fontFamily: 'ui-monospace, monospace' }}>{fmtNum(t)}</td>
          </tr>
          <tr>
            <td><Text strong>自由度 (df)</Text></td>
            <td style={{ textAlign: 'right', fontFamily: 'ui-monospace, monospace' }}>{fmtNum(df)}</td>
          </tr>
          <tr>
            <td><Text strong>p 值</Text></td>
            <td className={p !== null && p < 0.05 ? 'sig' : undefined} style={{ textAlign: 'right' }}>{fmtP(p)}</td>
          </tr>
          <tr>
            <td><Text strong>有效观测 n</Text></td>
            <td style={{ textAlign: 'right', fontFamily: 'ui-monospace, monospace' }}>{n ?? '-'}</td>
          </tr>
        </tbody>
      </table>
      <p className="table-one-footnote">
        变量 {xName} 与 {yName}
        {methodRaw ? ` · ${methodRaw}` : ''}
        。相关不等于因果；{isSpearman ? 'Spearman 基于秩，对单调非线性关系稳健' : 'Pearson 只刻画线性关联，离群值敏感'}
        ，置信区间由 Fisher z 变换得出。
      </p>
    </div>
  );
}

/**
 * 功效 / 样本量 payload → 设计阶段结论表。
 *
 * power 是无数据集分析（design phase），后端返回 required_n / achieved_power /
 * effect_size / alpha / method / converged。真机验收时这些数字在 UI 上完全拿不到
 * （「数据快照不可用」且导出禁用），研究者看不到每组样本量。
 */
function renderPower(payload: Record<string, unknown>) {
  const num = (key: string): number | null => {
    const value = payload[key];
    return typeof value === 'number' && Number.isFinite(value) ? value : null;
  };
  const requiredN = num('required_n');
  const achievedPower = num('achieved_power');
  const effectSize = num('effect_size');
  const alpha = num('alpha');
  const method = typeof payload.method === 'string' ? payload.method : null;
  const converged = payload.converged;
  const totalN = num('total_n') ?? (requiredN !== null ? requiredN * 2 : null);

  const rows: Array<{ label: string; value: string; strong?: boolean }> = [
    { label: '每组样本量', value: requiredN !== null ? String(Math.ceil(requiredN)) : '-', strong: true },
    { label: '两组合计', value: totalN !== null ? String(Math.ceil(totalN)) : '-', strong: true },
    { label: '实际功效', value: fmtNum(achievedPower) },
    { label: '效应量', value: fmtNum(effectSize) },
    { label: '显著性水平 α', value: fmtNum(alpha) },
  ];

  return (
    <div className="three-line-table-scroll">
      <table className="three-line-table three-line-table--regression" aria-label="功效与样本量结果表">
        <thead>
          <tr>
            <th className="tlt-corner">设计参数</th>
            <th style={{ textAlign: 'right' }}>数值</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((row) => (
            <tr key={row.label}>
              <td><Text strong>{row.label}</Text></td>
              <td style={{ textAlign: 'right', fontFamily: 'ui-monospace, monospace', fontWeight: row.strong ? 600 : 400 }}>
                {row.value}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      <p className="table-one-footnote">
        {method ? `方法 ${method}· ` : ''}
        样本量为满足目标功效所需的最小整数，已向上取整
        {converged === false ? ' · ⚠ 迭代未收敛，结果仅供参考' : ''}
        。这是设计阶段计算，不使用任何数据集；实际入组请再考虑脱落率与多重比较。
      </p>
    </div>
  );
}

export function ThreeLineTable({ markdown, skillResult, filterKeyword = '' }: ThreeLineTableProps) {
  // Scenario 1: Render from structured SkillResult
  if (skillResult && skillResult.payload) {
    const payload = skillResult.payload as Record<string, unknown>;

    // Sub-scenario 1.0: Engine TableOneResult ({ groups: [...] })
    if (Array.isArray(payload.groups) && payload.groups.length > 0) {
      const first = payload.groups[0] as Record<string, unknown>;
      // Distinguish from t-test groups (label/mean only) by presence of continuous/categorical summaries.
      if (
        first &&
        typeof first === 'object' &&
        (Array.isArray(first.continuous) || Array.isArray(first.categorical))
      ) {
        const rendered = renderGroupsTableOne(payload as {
          strata?: string | null;
          continuous?: string[];
          categorical?: string[];
          groups: TableOneGroup[];
          standardized_differences?: TableOneStandardizedDifferences;
          categorical_tests?: TableOneCategoricalTest[];
          continuous_tests?: TableOneContinuousTest[];
        }, filterKeyword);
        if (rendered) return rendered;
      }
    }

    // Sub-scenario 1.0b: One-way ANOVA (flat SS/df/MS/F payload, no coefficients).
    // 判据用 analysis.algorithm_id 而不是 payload.method：后者是引擎的展示名
    // （实测 'One-way ANOVA'，随文案可变），不适合当契约。tableone 的
    // continuous_tests 里也会出现 ANOVA，但那嵌套在 groups 里、已被上一分支
    // 接走，且 algorithm_id 是 'tableone'，不会误入此处。
    if (skillResult.analysis?.algorithm_id === 'anova' && typeof payload.f_statistic === 'number') {
      return renderAnova(payload);
    }

    // Sub-scenario 1.0c: 双样本 t 检验（groups 只有 label/n/mean，非 Table One）。
    // 判据同样落在 algorithm_id 上，并要求 t_statistic 在场，避免历史/异形载荷
    // 落进来渲染出一张全是「-」的表。
    if (skillResult.analysis?.algorithm_id === 'ttest' && typeof payload.t_statistic === 'number') {
      return renderTtest(payload);
    }

    // Sub-scenario 1.0d: 相关分析（r/ci/t/df 扁平载荷）。
    if (skillResult.analysis?.algorithm_id === 'correlation' && typeof payload.r === 'number') {
      return renderCorrelation(payload);
    }

    // Sub-scenario 1.0e: 功效/样本量。power 不是 Output-Level 算法，
    // skillToAlgorithm('power') 返回 null，因此 analysis 可能没有 algorithm_id；
    // 判据只能落在载荷形状上（required_n 是 power 独有字段）。
    if (typeof payload.required_n === 'number' && typeof payload.achieved_power === 'number') {
      return renderPower(payload);
    }

    // Sub-scenario 1.1: Legacy TableOneResult (rows + group_levels)
    if (payload.rows && Array.isArray(payload.rows) && payload.group_levels) {
      const groupLevels = payload.group_levels as string[];
      return (
        <div className="three-line-table-scroll">
          <table className="three-line-table three-line-table--grouped">
            <thead>
              <tr>
                <th rowSpan={2} className="tlt-corner">特征变量</th>
                <th colSpan={1} style={{ textAlign: 'center', borderBottom: '1px solid #2b3b4c' }}>总体样本</th>
                <th colSpan={groupLevels.length} style={{ textAlign: 'center', borderBottom: '1px solid #2b3b4c' }}>
                  按 {String(payload.by || '分组')} 对比
                </th>
                <th rowSpan={2} style={{ textAlign: 'right' }}>p 值</th>
              </tr>
              <tr>
                <th style={{ textAlign: 'center' }}>
                  Overall (N={
                    (payload.rows as Array<{ overall?: { n_total?: number } }>)[0]?.overall?.n_total || '未统计'
                  })
                </th>
                {groupLevels.map((lvl) => (
                  <th key={lvl} style={{ textAlign: 'center' }}>
                    {lvl}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {(payload.rows as Array<Record<string, unknown>>).map((row, idx) => (
                <tr key={idx}>
                  <td style={{ paddingLeft: row.level ? '24px' : '12px' }}>
                    {row.level ? (
                      <span style={{ color: '#5a6e85' }}>{String(row.level)}</span>
                    ) : (
                      <Text strong>{String(row.label || row.variable || '')}</Text>
                    )}
                  </td>
                  <td style={{ textAlign: 'center' }}>
                    {String((row.overall as { display?: string } | undefined)?.display || '-')}
                  </td>
                  {groupLevels.map((lvl) => {
                    const groupCell = (row.groups as Array<{ group: string; cell?: { display?: string } }> | undefined)
                      ?.find((g) => g.group === lvl);
                    return (
                      <td key={lvl} style={{ textAlign: 'center' }}>
                        {groupCell?.cell?.display || '-'}
                      </td>
                    );
                  })}
                  <td
                    className={typeof row.p_value === 'number' && row.p_value < 0.05 ? 'sig' : undefined}
                    style={{ textAlign: 'right' }}
                  >
                    {fmtP(typeof row.p_value === 'number' ? row.p_value : null)}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      );
    }

    // Sub-scenario 1.2: Regression Coefficients (Logistic/Cox/Linear)
    // Backend may emit camelCase (estimate/stdError/pValue/ciLower) or snake_case.
    if (payload.coefficients && Array.isArray(payload.coefficients)) {
      const termHints = termHintsFromAnalysis(
        skillResult.analysis?.algorithm_id,
        skillResult.analysis?.params as Record<string, unknown> | undefined,
      );
      const coeffs = normalizeCoefficients(payload.coefficients, termHints);
      if (coeffs.length === 0) return null;

      const isLogistic = coeffs.some((c) => c.oddsRatio !== null);
      const isCox = !isLogistic && coeffs.some((c) => c.hazardRatio !== null);

      const effectColHeader = isLogistic ? 'Odds Ratio (OR)' : isCox ? 'Hazard Ratio (HR)' : 'Beta 系数';
      const ciHeader = '95% 置信区间 (CI)';

      return (
        <div className="three-line-table-scroll">
          <table className="three-line-table three-line-table--regression">
            <thead>
              <tr>
                <th className="tlt-corner">影响因素 (协变量)</th>
                <th style={{ textAlign: 'right' }}>估计值 (Beta)</th>
                <th style={{ textAlign: 'right' }}>标准误 (SE)</th>
                {(isLogistic || isCox) && <th style={{ textAlign: 'right' }}>{effectColHeader}</th>}
                <th style={{ textAlign: 'center' }}>{ciHeader}</th>
                <th style={{ textAlign: 'right' }}>p 值</th>
              </tr>
            </thead>
            <tbody>
              {coeffs.map((coeff, idx) => {
                const estValue = isLogistic ? coeff.oddsRatio : isCox ? coeff.hazardRatio : null;
                const p = coeff.pValue;
                return (
                  <tr key={idx}>
                    <td>
                      <Text code>{coeff.term}</Text>
                      {coeff.reference && (
                        <span style={{ fontSize: '11px', color: 'var(--ink-300)', marginLeft: '6px' }}>
                          (对照组: {coeff.reference})
                        </span>
                      )}
                    </td>
                    <td style={{ textAlign: 'right' }}>{fmtNum(coeff.beta)}</td>
                    <td style={{ textAlign: 'right' }}>{fmtNum(coeff.standardError)}</td>
                    {estValue !== null && (
                      <td style={{ textAlign: 'right', fontWeight: 600 }}>{fmtNum(estValue)}</td>
                    )}
                    <td style={{ textAlign: 'center', fontFamily: 'monospace' }}>
                      [{fmtNum(coeff.ciLower)}, {fmtNum(coeff.ciUpper)}]
                    </td>
                    <td
                      className={typeof p === 'number' && p < 0.05 ? 'sig' : undefined}
                      style={{ textAlign: 'right' }}
                    >
                      {fmtP(p)}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      );
    }
  }

  // Scenario 2: Render from markdown string
  if (markdown) {
    const parsed = parseMarkdownTable(markdown);
    if (!parsed) {
      return <pre style={{ whiteSpace: 'pre-wrap', fontFamily: 'monospace' }}>{markdown}</pre>;
    }

    const { headers, rows } = parsed;

    return (
      <div className="three-line-table-scroll">
        <table className="three-line-table">
          <thead>
            <tr>
              {headers.map((h, i) => (
                <th key={i} className={i === 0 ? 'tlt-corner' : undefined}>{h}</th>
              ))}
            </tr>
          </thead>
          <tbody>
            {rows.map((row, rIdx) => (
              <tr key={rIdx}>
                {row.map((cell, cIdx) => (
                  <td key={cIdx}>{cell}</td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    );
  }

  return null;
}

export default ThreeLineTable;
