import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { DatasetAudit, DatasetSummary, ResearchProtocol, RunRequest } from '../api/types';
import { AnalysisPreflightModal } from './AnalysisPreflightModal';

const dataset: DatasetSummary = {
  dataset_id: 'ds-1',
  file_name: 'cohort.csv',
  size_bytes: 1024,
  encoding: 'Utf8',
  row_count: 24,
  columns: [
    { name: 'outcome', inferred_type: 'Numeric', missing_count: 0 },
    { name: 'age', inferred_type: 'Numeric', missing_count: 5 },
  ],
  uploaded_at: '2026-01-01T00:00:00Z',
};

const request: RunRequest = {
  skill_id: 'model_linear',
  dataset_id: dataset.dataset_id,
  args: { outcome: 'outcome', predictors: ['age'] },
};

const protocol: ResearchProtocol = {
  status: 'Approved',
  research_question: '年龄与结局是否相关？',
  study_design: 'cross_sectional',
  population: '成人观察性队列',
  eligibility_criteria: '有基线记录',
  exposure: 'age',
  comparator: '每增加 1 岁',
  outcome: 'outcome',
  time_zero: '基线',
  follow_up: '不适用',
  analysis_unit: '参与者',
  estimand: '年龄每增加 1 岁对应的平均结局差',
  confounders: '',
  missing_data_strategy: '完整案例',
  primary_analysis: '多元线性回归',
  sensitivity_analysis: '',
  version: 1,
  content_sha256: 'a'.repeat(64),
  state_sha256: 'e'.repeat(64),
  approval_id: '11111111-1111-4111-8111-111111111111',
  approved_at: '2026-01-01T00:00:00Z',
  updated_at: '2026-01-01T00:00:00Z',
};

const audit: DatasetAudit = {
  schema_version: '1.0',
  audit_rules_version: '1.1.0',
  audit_id: '22222222-2222-4222-8222-222222222222',
  dataset_id: dataset.dataset_id,
  dataset_sha256: 'b'.repeat(64),
  protocol_version: 1,
  skill_id: request.skill_id,
  run_spec_sha256: 'c'.repeat(64),
  roles: {},
  status: 'passed',
  findings: [],
  audit_sha256: 'd'.repeat(64),
  created_at: '2026-01-01T00:00:01Z',
};

describe('AnalysisPreflightModal', () => {
  it('shows the exact run summary, risk warnings and trust statement', async () => {
    render(
      <AnalysisPreflightModal
        open
        dataset={dataset}
        request={request}
        promptText="分析 outcome 与 age 是否存在因果关系"
        protocol={protocol}
        audit={audit}
        onConfirm={vi.fn()}
        onCancel={vi.fn()}
        onEditProtocol={vi.fn()}
      />,
    );

    expect(await screen.findByRole('dialog', { name: '分析方案审批' })).toBeInTheDocument();
    expect(screen.getByLabelText('已审批研究协议')).toHaveTextContent('年龄与结局是否相关');
    expect(await screen.findByText('多元线性回归')).toBeInTheDocument();
    expect(screen.getByText('cohort.csv')).toBeInTheDocument();
    expect(screen.getByText('n = 24')).toBeInTheDocument();
    expect(screen.getByText(/age · 5 例 · 20\.8%/)).toBeInTheDocument();
    expect(screen.getByText(/观察性统计关联不能直接证明因果关系/)).toBeInTheDocument();
    expect(screen.getByText('本机确定性引擎 · 数值非 LLM 生成 · 可审计')).toBeInTheDocument();
  });

  it('keeps confirm and cancel as separate explicit actions', async () => {
    const onConfirm = vi.fn();
    const onCancel = vi.fn();
    render(
      <AnalysisPreflightModal
        open
        dataset={dataset}
        request={request}
        promptText="分析 outcome 与 age"
        protocol={protocol}
        audit={audit}
        onConfirm={onConfirm}
        onCancel={onCancel}
        onEditProtocol={vi.fn()}
      />,
    );

    expect(await screen.findByRole('dialog', { name: '分析方案审批' })).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: /取\s*消/ }));
    expect(onCancel).toHaveBeenCalledTimes(1);
    expect(onConfirm).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole('button', { name: '批准方案并运行' }));
    expect(onConfirm).toHaveBeenCalledTimes(1);
  });

  it('blocks execution until a research protocol is approved', async () => {
    const onConfirm = vi.fn();
    const onEditProtocol = vi.fn();
    render(
      <AnalysisPreflightModal
        open
        dataset={dataset}
        request={request}
        promptText="分析 outcome 与 age"
        protocol={null}
        audit={null}
        onConfirm={onConfirm}
        onCancel={vi.fn()}
        onEditProtocol={onEditProtocol}
      />,
    );

    expect(await screen.findByText('研究协议尚未审批')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '批准方案并运行' })).toBeDisabled();
    fireEvent.click(screen.getByRole('button', { name: '完善并审批协议' }));
    expect(onEditProtocol).toHaveBeenCalledTimes(1);
    expect(onConfirm).not.toHaveBeenCalled();
  });

  it('shows a high-severity warning when the run outcome differs from the approved protocol', async () => {
    render(
      <AnalysisPreflightModal
        open
        dataset={dataset}
        request={{ ...request, args: { outcome: 'age', predictors: ['outcome'] } }}
        promptText="以 age 为结局建立线性回归"
        protocol={protocol}
        audit={audit}
        onConfirm={vi.fn()}
        onCancel={vi.fn()}
        onEditProtocol={vi.fn()}
      />,
    );

    expect(await screen.findByText(/本次结局变量 age 未在已审批协议结局/)).toBeInTheDocument();
    expect(screen.queryByText('未发现预设的样本量、缺失或设计错配风险。')).not.toBeInTheDocument();
  });

  it('shows server blockers and disables approval', async () => {
    render(
      <AnalysisPreflightModal
        open
        dataset={dataset}
        request={request}
        promptText="分析 outcome 与 age"
        protocol={protocol}
        audit={{
          ...audit,
          status: 'blocked',
          findings: [{
            code: 'DUPLICATE_PRIMARY_KEY',
            severity: 'blocker',
            columns: ['participant_id'],
            affected_rows: 2,
            sample_row_numbers: [2, 8],
            message: '主键重复，无法确认独立观测单位。',
          }],
        }}
        onConfirm={vi.fn()}
        onCancel={vi.fn()}
        onEditProtocol={vi.fn()}
      />,
    );

    expect(await screen.findByText(/发现 1 个阻断项/)).toBeInTheDocument();
    expect(screen.getByText('DUPLICATE_PRIMARY_KEY')).toBeInTheDocument();
    expect(screen.getByText(/示例数据行：2、8/)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '批准方案并运行' })).toBeDisabled();
  });

  // PRIMARY_KEY_UNBOUND 是唯一能由用户当场自解的阻断项：服务端只按列名猜主键，
  // 猜不到就阻断，但 /audit 接受显式 audit_roles.primary_key。没有这个入口，
  // 列名不合约定的数据集在界面上只能改名重传。
  const unboundAudit: DatasetAudit = {
    ...audit,
    status: 'blocked',
    findings: [{
      code: 'PRIMARY_KEY_UNBOUND',
      severity: 'blocker',
      columns: [],
      affected_rows: 0,
      sample_row_numbers: [],
      message: '未能识别主键；请指定主键列。',
    }],
  };

  it('lets the user bind a primary key when the server could not infer one', async () => {
    const onPrimaryKeyChange = vi.fn();
    render(
      <AnalysisPreflightModal
        open
        dataset={dataset}
        request={request}
        promptText="分析 outcome 与 age"
        protocol={protocol}
        audit={unboundAudit}
        primaryKey={null}
        onPrimaryKeyChange={onPrimaryKeyChange}
        onConfirm={vi.fn()}
        onCancel={vi.fn()}
        onEditProtocol={vi.fn()}
      />,
    );

    expect(await screen.findByText('指定主键列')).toBeInTheDocument();
    // 阻断仍然生效：绑定后要重新审计，审批按钮不能在此刻放行。
    expect(screen.getByRole('button', { name: '批准方案并运行' })).toBeDisabled();

    fireEvent.mouseDown(screen.getByRole('combobox', { name: '指定主键列' }));
    fireEvent.click(await screen.findByText('outcome（无缺失）'));
    expect(onPrimaryKeyChange).toHaveBeenCalledWith(['outcome']);
  });

  it('does not offer primary-key binding for blockers the user cannot self-resolve', async () => {
    render(
      <AnalysisPreflightModal
        open
        dataset={dataset}
        request={request}
        promptText="分析 outcome 与 age"
        protocol={protocol}
        audit={{
          ...unboundAudit,
          findings: [{
            code: 'SENSITIVE_FIELD_PRESENT',
            severity: 'blocker',
            columns: ['email'],
            affected_rows: 24,
            sample_row_numbers: [1],
            message: '数据中存在可能的直接标识符。',
          }],
        }}
        primaryKey={null}
        onPrimaryKeyChange={vi.fn()}
        onConfirm={vi.fn()}
        onCancel={vi.fn()}
        onEditProtocol={vi.fn()}
      />,
    );

    expect(await screen.findByText('SENSITIVE_FIELD_PRESENT')).toBeInTheDocument();
    expect(screen.queryByText('指定主键列')).not.toBeInTheDocument();
  });
});
