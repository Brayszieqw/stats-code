import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { CapabilityStatement } from './CapabilityStatement';

describe('CapabilityStatement', () => {
  it('states the local engine trust boundary and unsupported advanced families', () => {
    render(<CapabilityStatement />);

    expect(screen.getByText('本机确定性引擎 · 数值非 LLM 生成 · 可审计')).toBeInTheDocument();
    expect(screen.getByText(/Table One/)).toBeInTheDocument();
    expect(screen.getByText(/线性、Logistic 与 Cox 回归/)).toBeInTheDocument();
    expect(screen.getByText(/不独立作出诊断、治疗或个体决策/)).toBeInTheDocument();
    expect(screen.getByText(/当前不支持：PSM、TMLE、竞争风险、时空模型与 CDISC/)).toBeInTheDocument();
  });

  it('separates methods with a real entry point from engine-only ones', () => {
    render(<CapabilityStatement />);

    // 「当前支持」必须限定为有分析入口的方法，标题自身要说清这个限定
    expect(screen.getByText(/当前支持（有分析入口，可直接运行）/)).toBeInTheDocument();
    expect(screen.getByText(/引擎已实现，但暂无分析入口/)).toBeInTheDocument();
    expect(screen.getByText(/尚未提供配置入口，当前版本无法运行/)).toBeInTheDocument();
  });

  it('lists engine-only families under 暂无入口 rather than 已支持', () => {
    render(<CapabilityStatement />);

    // 非参数检验没有注册成技能，不能出现在「已支持」里
    const nonparametric = screen.getByText('非参数检验与秩方法');
    expect(nonparametric.closest('li')?.textContent).toContain('暂无入口');
    expect(nonparametric.closest('li')?.textContent).not.toContain('已支持');
  });

  it('claims power analysis as supported because it has an entry point', () => {
    render(<CapabilityStatement />);

    const power = screen.getByText('功效与样本量分析');
    expect(power.closest('li')?.textContent).toContain('已支持');
  });
});
