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

/** Engine tableone payload: { strata, continuous, categorical, groups[] } */
function renderGroupsTableOne(payload: {
  strata?: string | null;
  continuous?: string[];
  categorical?: string[];
  groups: TableOneGroup[];
}) {
  const groups = payload.groups;
  if (!Array.isArray(groups) || groups.length === 0) return null;

  const first = groups[0]!;
  const continuousNames =
    first.continuous?.map((c) => c.variable) ??
    (Array.isArray(payload.continuous) ? payload.continuous : []);
  const categoricalNames =
    first.categorical?.map((c) => c.variable) ??
    (Array.isArray(payload.categorical) ? payload.categorical : []);

  const findContinuous = (group: TableOneGroup, variable: string) =>
    group.continuous?.find((c) => c.variable === variable);
  const findCategorical = (group: TableOneGroup, variable: string) =>
    group.categorical?.find((c) => c.variable === variable);

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

  return (
    <div className="three-line-table-scroll">
      <table className="three-line-table three-line-table--grouped" aria-label="Table One 三线表">
        <thead>
          <tr>
            <th>特征变量</th>
            {groups.map((group) => (
              <th key={group.label} style={{ textAlign: 'center' }}>
                {group.label} (N={group.n})
              </th>
            ))}
          </tr>
          <tr>
            <th colSpan={1 + groups.length} style={{ fontWeight: 500, color: '#5a6e85', borderBottom: '1px solid #2b3b4c' }}>
              {strataLabel}
            </th>
          </tr>
        </thead>
        <tbody>
          {continuousNames.map((variable) => (
            <tr key={`c-${variable}`}>
              <td>
                <Text strong>{variable}</Text>
                <div style={{ fontSize: 11, color: '#8c8c8c' }}>连续 · 均值±SD；中位数 (Q1–Q3)</div>
              </td>
              {groups.map((group) => (
                <td key={`${group.label}-${variable}`} style={{ textAlign: 'center', fontFamily: 'ui-monospace, monospace', fontSize: 13 }}>
                  {formatContinuous(findContinuous(group, variable))}
                </td>
              ))}
            </tr>
          ))}
          {categoricalNames.map((variable) => {
            const levels = levelsByVar.get(variable) ?? [];
            return (
              <Fragment key={`cat-block-${variable}`}>
                <tr>
                  <td colSpan={1 + groups.length}>
                    <Text strong>{variable}</Text>
                    <span style={{ marginLeft: 8, fontSize: 11, color: '#8c8c8c' }}>分类 · n (%)</span>
                  </td>
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
                  </tr>
                ))}
              </Fragment>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}

export function ThreeLineTable({ markdown, skillResult }: ThreeLineTableProps) {
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
        });
        if (rendered) return rendered;
      }
    }

    // Sub-scenario 1.1: Legacy TableOneResult (rows + group_levels)
    if (payload.rows && Array.isArray(payload.rows) && payload.group_levels) {
      const groupLevels = payload.group_levels as string[];
      return (
        <div className="three-line-table-scroll">
          <table className="three-line-table three-line-table--grouped">
            <thead>
              <tr>
                <th rowSpan={2}>特征变量</th>
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
                <th>影响因素 (协变量)</th>
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
                        <span style={{ fontSize: '11px', color: '#8c8c8c', marginLeft: '6px' }}>
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
                <th key={i}>{h}</th>
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
