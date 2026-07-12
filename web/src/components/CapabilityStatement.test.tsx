import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { CapabilityStatement } from './CapabilityStatement';

describe('CapabilityStatement', () => {
  it('states the local engine trust boundary and unsupported advanced families', () => {
    render(<CapabilityStatement />);

    expect(screen.getByText('本机确定性引擎 · 数值非 LLM 生成 · 可审计')).toBeInTheDocument();
    expect(screen.getByText(/Table One/)).toBeInTheDocument();
    expect(screen.getByText(/线性、Logistic 与 Cox 回归/)).toBeInTheDocument();
    expect(screen.getByText(/当前不支持：PSM、TMLE、竞争风险、时空模型与 CDISC/)).toBeInTheDocument();
  });
});
