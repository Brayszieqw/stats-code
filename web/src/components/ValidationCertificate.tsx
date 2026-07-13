import type { ReactElement } from 'react';

import type {
  AlgorithmEntry,
  CoverageState,
  ReferenceSoftware,
} from '../lib/coverageMatrix';

const SOFTWARE_ORDER: readonly ReferenceSoftware[] = ['R', 'SAS', 'Python', 'SPSS'];

const STATE_LABEL: Record<CoverageState, string> = {
  live: 'live parity 测试面',
  recorded: 'recorded 金样 parity',
  sidecar_only: '仅代码 · 未自动 parity',
  none: '未覆盖',
};

export interface ValidationCertificateProps {
  algorithmId: string;
  entry?: AlgorithmEntry;
  releaseVersion: string;
  matrixSchemaVersion: number;
}

/** Compact, coverage-matrix-backed certificate shown beside reproducible code. */
export function ValidationCertificate({
  algorithmId,
  entry,
  releaseVersion,
  matrixSchemaVersion,
}: ValidationCertificateProps): ReactElement {
  return (
    <section
      className="validation-certificate"
      aria-label="验证证书"
      data-testid="validation-certificate"
    >
      <div className="validation-certificate__header">
        <div>
          <strong>验证证书</strong>
          <span>{entry?.display_name ?? algorithmId}</span>
        </div>
        <code>@stats-code/engine {releaseVersion || 'unknown'}</code>
      </div>

      {entry ? (
        <div className="validation-certificate__coverage" aria-label="参考软件验证覆盖">
          {SOFTWARE_ORDER.map((software) => {
            const state = entry.coverage[software];
            const reference = entry.reference[software];
            return (
              <div key={software} data-coverage-state={state}>
                <span>{software}</span>
                <strong>{STATE_LABEL[state]}</strong>
                <small>{reference.callable} · {reference.version}</small>
              </div>
            );
          })}
        </div>
      ) : (
        <p className="validation-certificate__unregistered">
          算法未登记在覆盖矩阵中，不能声明 parity 或金样验证状态。
        </p>
      )}

      <details className="validation-certificate__limits">
        <summary>证书范围与限制 · matrix schema v{matrixSchemaVersion}</summary>
        <p><code>live</code> / <code>recorded</code> 描述自动化测试面的能力，不代表本次运行已调用外部软件实时复算。</p>
        <p><code>sidecar_only</code> 仅保证生成参考代码；<code>none</code> 不提供代码或 parity 声明。</p>
        <p>数值覆盖不替代研究设计、模型假设、临床意义与因果解释审核；本次数据绑定见下方 SHA256。</p>
      </details>
    </section>
  );
}

export default ValidationCertificate;
