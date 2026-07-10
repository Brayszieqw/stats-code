import { Typography } from 'antd';
import type { SkillResult } from '../api/types';
import { fmtNum, fmtP, normalizeCoefficients } from '../lib/coeffFields';

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

export function ThreeLineTable({ markdown, skillResult }: ThreeLineTableProps) {
  // Scenario 1: Render from structured SkillResult
  if (skillResult && skillResult.payload) {
    const payload = skillResult.payload as any;

    // Sub-scenario 1.1: TableOneResult
    if (payload.rows && Array.isArray(payload.rows) && payload.group_levels) {
      const groupLevels = payload.group_levels as string[];
      return (
        <div style={{ width: '100%', overflowX: 'auto' }}>
          <table className="three-line-table">
            <thead>
              <tr>
                <th rowSpan={2}>特征变量</th>
                <th colSpan={1} style={{ textAlign: 'center', borderBottom: '1px solid #2b3b4c' }}>总体样本</th>
                <th colSpan={groupLevels.length} style={{ textAlign: 'center', borderBottom: '1px solid #2b3b4c' }}>
                  按 {payload.by || '分组'} 对比
                </th>
                <th rowSpan={2} style={{ textAlign: 'right' }}>p 值</th>
              </tr>
              <tr>
                <th style={{ textAlign: 'center' }}>Overall (N={payload.rows[0]?.overall?.n_total || '未统计'})</th>
                {groupLevels.map((lvl) => (
                  <th key={lvl} style={{ textAlign: 'center' }}>
                    {lvl}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {payload.rows.map((row: any, idx: number) => (
                <tr key={idx}>
                  <td style={{ paddingLeft: row.level ? '24px' : '12px' }}>
                    {row.level ? (
                      <span style={{ color: '#5a6e85' }}>{row.level}</span>
                    ) : (
                      <Text strong>{row.label || row.variable}</Text>
                    )}
                  </td>
                  <td style={{ textAlign: 'center' }}>{row.overall?.display || '-'}</td>
                  {groupLevels.map((lvl) => {
                    const groupCell = row.groups?.find((g: any) => g.group === lvl);
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
      const coeffs = normalizeCoefficients(payload.coefficients);
      if (coeffs.length === 0) return null;

      const isLogistic = coeffs.some((c) => c.oddsRatio !== null);
      const isCox = !isLogistic && coeffs.some((c) => c.hazardRatio !== null);

      const effectColHeader = isLogistic ? 'Odds Ratio (OR)' : isCox ? 'Hazard Ratio (HR)' : 'Beta 系数';
      const ciHeader = '95% 置信区间 (CI)';

      return (
        <div style={{ width: '100%', overflowX: 'auto' }}>
          <table className="three-line-table">
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
      <div style={{ width: '100%', overflowX: 'auto' }}>
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
