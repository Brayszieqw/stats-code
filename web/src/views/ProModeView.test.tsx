/**
 * Tests for ProModeView.
 *
 * Validates: Requirements 4.1, 4.3
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor, within } from '@testing-library/react';

const { useCoverageMatrixSpy, runSkillSpy } = vi.hoisted(() => ({
  useCoverageMatrixSpy: vi.fn(),
  runSkillSpy: vi.fn(),
}));
vi.mock('../lib/coverageMatrixContext', () => ({
  useCoverageMatrix: useCoverageMatrixSpy,
}));
vi.mock('../api/client', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../api/client')>()),
  runSkill: runSkillSpy,
}));
vi.mock('../components/AnalysisConfigurator', () => ({
  AnalysisConfigurator: ({
    summary,
    onSubmit,
    disabled,
  }: {
    summary: DatasetSummary;
    onSubmit: (request: { skill_id: string; dataset_id: string; args: Record<string, unknown> }, prompt: string) => Promise<void>;
    disabled?: boolean;
  }) => (
    <button
      type="button"
      aria-label={`提交分析:${summary.dataset_id}`}
      disabled={disabled}
      onClick={() => {
        void onSubmit(
          { skill_id: 'model_linear', dataset_id: summary.dataset_id, args: {} },
          `分析 ${summary.file_name}`,
        );
      }}
    >
      开始统计计算
    </button>
  ),
}));

import { ProModeView, mergeWorkspaceMessages } from './ProModeView';
import type { SessionController } from '../hooks/useSessionController';
import type { UseSseChatReturn, ChatMessage } from '../hooks/useSseChat';
import type { UseSessionListReturn } from '../hooks/useSessionList';
import type { AnalysisPlanApproval, DatasetAudit, DatasetSummary, ResearchProtocol, SkillResult } from '../api/types';

beforeEach(() => {
  runSkillSpy.mockReset();
  useCoverageMatrixSpy.mockReturnValue({
    matrix: { schema_version: 1, release_version: '1.0.0', algorithms: [] },
    loading: false,
    error: undefined,
  });
});

const approvedProtocol: ResearchProtocol = {
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

function makeController(overrides: Partial<SessionController> = {}): SessionController {
  return {
    sessionId: 's1',
    loading: false,
    error: null,
    isArchived: false,
    datasets: [],
    decisionAssistant: true,
    researchProtocol: approvedProtocol,
    datasetAudits: [],
    analysisPlanApprovals: [],
    integrityWarnings: [],
    setDecisionAssistant: vi.fn(),
    addDataset: vi.fn(),
    saveResearchProtocol: vi.fn(async () => approvedProtocol),
    compileResearchProtocol: vi.fn(async () => { throw new Error('not used'); }),
    auditDataset: vi.fn(async (datasetId, input): Promise<DatasetAudit> => ({
      schema_version: '1.0',
      audit_rules_version: '1.1.0',
      audit_id: '22222222-2222-4222-8222-222222222222',
      dataset_id: datasetId,
      dataset_sha256: 'b'.repeat(64),
      protocol_version: input.expected_protocol_version,
      skill_id: input.skill_id,
      run_spec_sha256: 'c'.repeat(64),
      roles: input.audit_roles ?? {},
      status: 'passed',
      findings: [],
      audit_sha256: 'd'.repeat(64),
      created_at: '2026-01-01T00:00:01Z',
    })),
    approveAnalysisPlan: vi.fn(async (input): Promise<AnalysisPlanApproval> => ({
      schema_version: '1.0',
      plan_id: '33333333-3333-4333-8333-333333333333',
      approval_id: '44444444-4444-4444-8444-444444444444',
      status: 'Approved',
      protocol_version: input.expected_protocol_version,
      protocol_sha256: approvedProtocol.content_sha256,
      protocol_approval_id: approvedProtocol.approval_id!,
      dataset_id: input.dataset_id,
      dataset_sha256: 'b'.repeat(64),
      skill_id: input.skill_id,
      args: input.args,
      run_spec_sha256: 'c'.repeat(64),
      audit_id: '22222222-2222-4222-8222-222222222222',
      audit_sha256: 'd'.repeat(64),
      audit_roles: input.audit_roles ?? {},
      approved_at: '2026-01-01T00:00:02Z',
    })),
    initialMessages: [],
    startNewSession: vi.fn(async () => {}),
    loadSession: vi.fn(async () => {}),
    ...overrides,
  };
}

function makeChat(messages: ChatMessage[] = []): UseSseChatReturn {
  return {
    messages,
    setMessages: vi.fn(),
    sendMessage: vi.fn(),
    status: 'idle',
    error: null,
    isStreaming: false,
  };
}

function makeList(): UseSessionListReturn {
  return { sessions: [], loading: false, error: null, refresh: vi.fn(async () => {}) };
}

const dataset: DatasetSummary = {
  dataset_id: 'ds-1',
  file_name: 'cohort.csv',
  size_bytes: 1024,
  encoding: 'Utf8',
  row_count: 100,
  columns: [{ name: 'age', inferred_type: 'Numeric', missing_count: 0 }],
  uploaded_at: '2026-01-01T00:00:00Z',
  sha256: null,
};

const otherDataset: DatasetSummary = {
  ...dataset,
  dataset_id: 'ds-2',
  file_name: 'other.csv',
  row_count: 12,
};

function analysisMessage({
  id,
  runId,
  boundDataset = dataset,
  beta,
  content,
  interpretation,
  timestamp,
}: {
  id: string;
  runId: string;
  boundDataset?: DatasetSummary;
  beta: number;
  content: string;
  interpretation?: string;
  timestamp: string;
}): ChatMessage {
  return {
    id,
    role: 'agent',
    content,
    interpretation,
    timestamp: new Date(timestamp),
    skillResult: {
      schema_version: '1.0',
      payload: {
        coefficients: [{
          term: 'age',
          beta,
          ci_lower: beta - 0.1,
          ci_upper: beta + 0.1,
          p_value: 0.01,
        }],
      },
      risk_signals: [],
      analysis: {
        algorithm_id: 'model_linear',
        dataset_id: boundDataset.dataset_id,
        dataset_sha256: null,
        columns: boundDataset.columns,
        params: {},
        run_id: runId,
        run_status: 'completed',
      },
    },
  };
}

function renderView(options: {
  controller?: Partial<SessionController>;
  onOpenSettings?: () => void;
  messages?: ChatMessage[];
  llmConfigured?: boolean;
} = {}) {
  return {
    onOpenSettings: options.onOpenSettings,
    ...render(
    <ProModeView
      controller={makeController(options.controller)}
      chat={makeChat(options.messages)}
      sessionList={makeList()}
      mode="pro"
      onModeChange={vi.fn()}
      onSend={vi.fn()}
      onChoiceSubmit={vi.fn()}
      onRetry={vi.fn()}
      onVoiceTranscript={vi.fn()}
      model="deepseek-chat"
      llmConfigured={options.llmConfigured ?? true}
      onOpenSettings={options.onOpenSettings}
    />,
    ),
  };
}

describe('ProModeView (Requirements 4.1, 4.3)', () => {
  it('surfaces non-empty session integrity warnings as a visible alert', () => {
    renderView({
      controller: {
        integrityWarnings: [{
          event: 'file_session_integrity_warning',
          action: 'discarded',
          record_type: 'analysis_plan_approval',
          session_id: '11111111-1111-4111-8111-111111111111',
          reason: 'protocol_not_approved',
        }],
      },
    });

    expect(screen.getByRole('alert')).toHaveAttribute('data-testid', 'session-integrity-warning');
    expect(screen.getByText('会话完整性检查阻止了不可信审批数据')).toBeInTheDocument();
  });

  it('orders direct-run artifacts by time so newer chat results stay current', () => {
    const oldDirect: ChatMessage = {
      id: 'direct-old',
      role: 'agent',
      content: '',
      timestamp: new Date('2026-01-01T00:00:01Z'),
      skillResult: {
        schema_version: '1.0',
        payload: {},
        risk_signals: [],
        analysis: {
          algorithm_id: 'model_linear',
          dataset_id: 'ds-1',
          dataset_sha256: 'a'.repeat(64),
          columns: [],
          params: {},
          run_id: 'direct-old',
          run_status: 'completed',
        },
      },
    };
    const newerChat: ChatMessage = {
      ...oldDirect,
      id: 'chat-new',
      timestamp: new Date('2026-01-01T00:00:02Z'),
      skillResult: {
        ...oldDirect.skillResult!,
        analysis: { ...oldDirect.skillResult!.analysis!, run_id: 'chat-new' },
      },
    };

    expect(mergeWorkspaceMessages([newerChat], [oldDirect], 'direct-old').map((message) => message.id))
      .toEqual(['direct-old', 'chat-new']);
  });

  it(
    'keeps conversation primary and opens a unified analysis workspace on demand',
    async () => {
      renderView();
      expect(await screen.findByText('专业统计分析')).toBeInTheDocument();
      expect(screen.getByLabelText('界面模式切换')).toBeInTheDocument();
      expect(screen.getByLabelText('研究对话')).toBeInTheDocument();
      expect(screen.queryByLabelText('分析检查器')).not.toBeInTheDocument();

      fireEvent.click(await screen.findByLabelText('打开分析检查器'));
      const workspaces = await screen.findAllByLabelText('分析检查器');
      const workspace = workspaces.find((element) => element.tagName === 'ASIDE');
      expect(workspace).toBeDefined();
      if (!workspace) return;
      expect(within(workspace).getByLabelText('工作区视图')).toBeInTheDocument();
      expect(within(workspace).getByText(/暂无分析结果/)).toBeInTheDocument();
      // AssistantPanel empty state shows the welcome composer.
      expect(screen.getAllByLabelText('消息输入框').length).toBeGreaterThan(0);
      expect(screen.queryByLabelText('调整报告与助手区域比例')).not.toBeInTheDocument();
    },
    15_000,
  );


  it('wires the professional welcome controls to real actions', () => {
    const onOpenSettings = vi.fn();
    renderView({ controller: { datasets: [dataset] }, onOpenSettings });

    expect(screen.getByLabelText('选择数据集')).not.toBeDisabled();
    expect(screen.getByLabelText('模型选择')).not.toBeDisabled();
    expect(screen.getByLabelText('语音输入')).not.toBeDisabled();

    fireEvent.click(screen.getByLabelText('模型选择'));
    expect(onOpenSettings).toHaveBeenCalledTimes(1);

    fireEvent.click(screen.getByLabelText('语音输入'));
    expect(screen.getByText(/录音完成后/)).toBeInTheDocument();
  });

  it('keeps direct statistics available when LLM interpretation is not configured', async () => {
    renderView({ controller: { datasets: [dataset] }, llmConfigured: false });

    expect(screen.getByText('AI 解读未配置 · 统计引擎可用')).toBeInTheDocument();
    fireEvent.click(screen.getByLabelText('打开分析检查器'));
    expect(await screen.findByLabelText('提交分析:ds-1')).not.toBeDisabled();
  });

  it('opens reproducible code inside the unified workspace', async () => {
    renderView();
    fireEvent.click(screen.getByLabelText('打开分析检查器'));
    const workspace = (await screen.findAllByLabelText('分析检查器')).find(
      (element) => element.tagName === 'ASIDE',
    );
    expect(workspace).toBeDefined();
    if (!workspace) return;
    fireEvent.click(within(workspace).getByText('代码', { exact: true }));
    expect(within(workspace).getByLabelText('可复现代码')).toBeInTheDocument();
  });

  it('lets the workspace close without removing the conversation', async () => {
    renderView();

    fireEvent.click(screen.getByLabelText('打开分析检查器'));
    expect((await screen.findAllByLabelText('分析检查器')).some((element) => element.tagName === 'ASIDE')).toBe(true);
    fireEvent.click(screen.getByLabelText('收起分析检查器'));
    expect(screen.getByLabelText('打开分析检查器')).toBeInTheDocument();
    expect(screen.getByLabelText('研究对话')).toBeInTheDocument();
  });

  it('keeps a result bound to the dataset that produced it', async () => {
    const result: SkillResult = {
      schema_version: '1.0',
      payload: { coefficients: [{ term: 'age', beta: 0.2, ci_lower: 0.1, ci_upper: 0.3, p_value: 0.01 }] },
      risk_signals: [],
      analysis: {
        algorithm_id: 'model_linear',
        dataset_id: 'ds-1',
        dataset_sha256: null,
        columns: dataset.columns,
        params: {},
        run_id: 'run-1',
        run_status: 'completed',
      },
    };
    const message: ChatMessage = {
      id: 'a1',
      role: 'agent',
      content: '分析完成',
      skillResult: result,
      timestamp: new Date(),
    };
    renderView({ controller: { datasets: [dataset, otherDataset] }, messages: [message] });

    fireEvent.click(screen.getByLabelText('数据集: other.csv'));
    const workspace = (await screen.findAllByLabelText('分析检查器')).find(
      (element) => element.tagName === 'ASIDE',
    );
    expect(workspace).toBeDefined();
    if (!workspace) return;
    fireEvent.click(within(workspace).getByText('代码', { exact: true }));
    const code = within(workspace).getByLabelText('可复现代码');
    expect(within(code).getByText('cohort.csv')).toBeInTheDocument();
    expect(within(code).queryByText('other.csv')).not.toBeInTheDocument();
  });

  it('opens the selected historical report with its own result and message', async () => {
    const olderQuestion: ChatMessage = {
      id: 'older-question',
      role: 'user',
      content: '旧研究问题',
      timestamp: new Date('2026-01-01T00:00:00Z'),
    };
    const older = analysisMessage({
      id: 'older',
      runId: 'old11111-run',
      beta: 0.2,
      content: '旧运行关键结论',
      interpretation: '旧运行方法提示',
      timestamp: '2026-01-01T00:00:01Z',
    });
    const newerQuestion: ChatMessage = {
      id: 'newer-question',
      role: 'user',
      content: '新研究问题',
      timestamp: new Date('2026-01-01T00:00:01.500Z'),
    };
    const newer = analysisMessage({
      id: 'newer',
      runId: 'new22222-run',
      boundDataset: otherDataset,
      beta: 0.8,
      content: '新运行关键结论',
      interpretation: '新运行方法提示',
      timestamp: '2026-01-01T00:00:02Z',
    });
    renderView({
      controller: { datasets: [dataset, otherDataset] },
      messages: [olderQuestion, older, newerQuestion, newer],
    });

    fireEvent.click((await screen.findAllByLabelText('查看报告'))[0]!);
    const workspace = (await screen.findAllByLabelText('分析检查器')).find(
      (element) => element.tagName === 'ASIDE',
    );
    expect(workspace).toBeDefined();
    if (!workspace) return;

    expect(within(workspace).getByText(/old11111 · cohort\.csv/)).toBeInTheDocument();
    expect(within(workspace).getByText('旧研究问题')).toBeInTheDocument();
    expect(within(workspace).queryByText('新研究问题')).not.toBeInTheDocument();
    expect(within(workspace).getByText('正在查看历史运行 old11111（线性回归）')).toBeInTheDocument();
    expect(within(workspace).getByText('旧运行关键结论')).toBeInTheDocument();
    expect(within(workspace).getByText('旧运行方法提示')).toBeInTheDocument();
    expect(within(workspace).queryByText('新运行关键结论')).not.toBeInTheDocument();
    expect(within(workspace).queryByText('新运行方法提示')).not.toBeInTheDocument();
    expect(within(workspace).queryByLabelText('调整变量或再次分析')).not.toBeInTheDocument();
    expect(within(workspace).queryByText('分析设置')).not.toBeInTheDocument();

    fireEvent.click(within(workspace).getByRole('button', { name: '回到最新结果' }));
    await waitFor(() => expect(within(workspace).getByText(/new22222 · other\.csv/)).toBeInTheDocument());
    expect(within(workspace).queryByText(/正在查看历史运行/)).not.toBeInTheDocument();
    expect(within(workspace).getByText('新运行关键结论')).toBeInTheDocument();
  });

  it('keeps the latest report title bound to its own question while the next answer is pending', async () => {
    const firstQuestion: ChatMessage = {
      id: 'question-1',
      role: 'user',
      content: '第一个研究问题',
      timestamp: new Date('2026-01-01T00:00:00Z'),
    };
    const firstResult = analysisMessage({
      id: 'result-1',
      runId: 'run11111',
      beta: 0.2,
      content: '第一个分析结果',
      timestamp: '2026-01-01T00:00:01Z',
    });
    const pendingQuestion: ChatMessage = {
      id: 'question-2',
      role: 'user',
      content: '第二个研究问题',
      timestamp: new Date('2026-01-01T00:00:02Z'),
    };
    renderView({
      controller: { datasets: [dataset] },
      messages: [firstQuestion, firstResult, pendingQuestion],
    });

    expect(screen.getByTitle('第二个研究问题')).toBeInTheDocument();
    fireEvent.click(screen.getByLabelText('打开分析检查器'));
    const workspace = (await screen.findAllByLabelText('分析检查器')).find(
      (element) => element.tagName === 'ASIDE',
    );
    expect(workspace).toBeDefined();
    if (!workspace) return;

    expect(within(workspace).getByText('第一个研究问题')).toBeInTheDocument();
    expect(within(workspace).queryByText('第二个研究问题')).not.toBeInTheDocument();
    expect(within(workspace).getByText('第一个分析结果')).toBeInTheDocument();
  });

  it('returns to follow mode when the latest result bubble is opened', async () => {
    const older = analysisMessage({
      id: 'older',
      runId: 'old11111-run',
      beta: 0.2,
      content: '旧运行关键结论',
      timestamp: '2026-01-01T00:00:01Z',
    });
    const newer = analysisMessage({
      id: 'newer',
      runId: 'new22222-run',
      beta: 0.8,
      content: '新运行关键结论',
      timestamp: '2026-01-01T00:00:02Z',
    });
    renderView({ controller: { datasets: [dataset] }, messages: [older, newer] });

    const reportButtons = await screen.findAllByLabelText('查看报告');
    fireEvent.click(reportButtons[0]!);
    expect(await screen.findByText(/正在查看历史运行 old11111/)).toBeInTheDocument();

    fireEvent.click(reportButtons[1]!);
    await waitFor(() => expect(screen.queryByText(/正在查看历史运行/)).not.toBeInTheDocument());
    const workspace = (await screen.findAllByLabelText('分析检查器')).find(
      (element) => element.tagName === 'ASIDE',
    );
    expect(workspace).toBeDefined();
    if (!workspace) return;
    expect(within(workspace).getByText(/new22222 · cohort\.csv/)).toBeInTheDocument();
  });

  it('opens an older artifact even when legacy metadata has no run id', async () => {
    const older: ChatMessage = {
      id: 'legacy-older',
      role: 'agent',
      content: '旧版工件结论',
      interpretation: '旧版方法提示',
      timestamp: new Date('2026-01-01T00:00:01Z'),
      skillResult: {
        schema_version: '1.0',
        payload: { required_n: 40 },
        risk_signals: [],
      },
    };
    const newer: ChatMessage = {
      id: 'legacy-newer',
      role: 'agent',
      content: '新版工件结论',
      timestamp: new Date('2026-01-01T00:00:02Z'),
      skillResult: {
        schema_version: '1.0',
        payload: { required_n: 80 },
        risk_signals: [],
      },
    };
    renderView({ messages: [older, newer] });

    expect(screen.getByText('结果已生成 · 分析报告')).toBeInTheDocument();
    const workflow = screen.getByLabelText('研究工作流');
    expect(within(workflow).getByText('4. 确定性执行').closest('.research-workflow__step')).toHaveClass('is-done');
    expect(within(workflow).getByText('5. 诊断与导出').closest('.research-workflow__step')).toHaveClass('is-active');

    fireEvent.click((await screen.findAllByLabelText('查看报告'))[0]!);
    const workspace = (await screen.findAllByLabelText('分析检查器')).find(
      (element) => element.tagName === 'ASIDE',
    );
    expect(workspace).toBeDefined();
    if (!workspace) return;

    expect(within(workspace).getByText('正在查看历史工件（统计分析）')).toBeInTheDocument();
    expect(within(workspace).getByText('旧版工件结论')).toBeInTheDocument();
    expect(within(workspace).getByText('旧版方法提示')).toBeInTheDocument();
    expect(within(workspace).queryByText('新版工件结论')).not.toBeInTheDocument();
    expect(within(workspace).getByText('结果已生成')).toBeInTheDocument();
    expect(within(workspace).queryByText('等待分析结果')).not.toBeInTheDocument();
  });

  it('binds historical code to the old dataset and clears the pin on dataset selection', async () => {
    const older = analysisMessage({
      id: 'older',
      runId: 'old11111-run',
      beta: 0.2,
      content: '旧运行关键结论',
      timestamp: '2026-01-01T00:00:01Z',
    });
    const newer = analysisMessage({
      id: 'newer',
      runId: 'new22222-run',
      boundDataset: otherDataset,
      beta: 0.8,
      content: '新运行关键结论',
      timestamp: '2026-01-01T00:00:02Z',
    });
    renderView({ controller: { datasets: [dataset, otherDataset] }, messages: [older, newer] });

    fireEvent.click(screen.getByLabelText('数据集: other.csv'));
    fireEvent.click((await screen.findAllByLabelText('查看代码'))[0]!);
    const workspace = (await screen.findAllByLabelText('分析检查器')).find(
      (element) => element.tagName === 'ASIDE',
    );
    expect(workspace).toBeDefined();
    if (!workspace) return;
    const code = within(workspace).getByLabelText('可复现代码');
    expect(within(code).getByText('cohort.csv')).toBeInTheDocument();
    expect(within(code).queryByText('other.csv')).not.toBeInTheDocument();
    expect(within(workspace).getByText(/正在查看历史运行 old11111/)).toBeInTheDocument();

    fireEvent.click(within(workspace).getByRole('radio', { name: /数据/ }));
    expect(await within(workspace).findByText('数据集已装载：cohort.csv')).toBeInTheDocument();
    expect(within(workspace).queryByText('数据集已装载：other.csv')).not.toBeInTheDocument();

    fireEvent.click(screen.getByLabelText('数据集: other.csv'));
    await waitFor(() => expect(within(workspace).queryByText(/正在查看历史运行/)).not.toBeInTheDocument());
  });

  it('leaves historical mode when a newer result arrives or the session changes', async () => {
    const older = analysisMessage({
      id: 'older',
      runId: 'old11111-run',
      beta: 0.2,
      content: '旧运行关键结论',
      timestamp: '2026-01-01T00:00:01Z',
    });
    const newer = analysisMessage({
      id: 'newer',
      runId: 'new22222-run',
      beta: 0.8,
      content: '新运行关键结论',
      timestamp: '2026-01-01T00:00:02Z',
    });
    const newest = analysisMessage({
      id: 'newest',
      runId: 'top33333-run',
      beta: 1.1,
      content: '最新运行关键结论',
      timestamp: '2026-01-01T00:00:03Z',
    });
    const controller = makeController({ datasets: [dataset] });
    const sessionList = makeList();
    const commonProps = {
      sessionList,
      mode: 'pro' as const,
      onModeChange: vi.fn(),
      onSend: vi.fn(),
      onChoiceSubmit: vi.fn(),
      onRetry: vi.fn(),
      onVoiceTranscript: vi.fn(),
      model: 'deepseek-chat',
    };
    const view = render(
      <ProModeView controller={controller} chat={makeChat([older, newer])} {...commonProps} />,
    );

    fireEvent.click((await screen.findAllByLabelText('查看报告'))[0]!);
    expect(await screen.findByText(/正在查看历史运行 old11111/)).toBeInTheDocument();

    view.rerender(
      <ProModeView controller={controller} chat={makeChat([older, newer, newest])} {...commonProps} />,
    );
    await waitFor(() => expect(screen.queryByText(/正在查看历史运行/)).not.toBeInTheDocument());
    expect(screen.getByText(/top33333 · cohort\.csv/)).toBeInTheDocument();

    fireEvent.click((await screen.findAllByLabelText('查看报告'))[0]!);
    expect(await screen.findByText(/正在查看历史运行 old11111/)).toBeInTheDocument();
    view.rerender(
      <ProModeView
        controller={makeController({ sessionId: 's2', datasets: [] })}
        chat={makeChat()}
        {...commonProps}
      />,
    );
    await screen.findByLabelText('打开分析检查器');
    expect(screen.queryByText(/正在查看历史运行/)).not.toBeInTheDocument();
  });

  it('labels power results as design-phase calculations without a data snapshot', async () => {
    const powerMessage: ChatMessage = {
      id: 'power-result',
      role: 'agent',
      content: '功效分析完成',
      timestamp: new Date('2026-01-01T00:00:01Z'),
      skillResult: {
        schema_version: '1.0',
        payload: {
          required_n: 64,
          achieved_power: 0.801,
          effect_size: 0.5,
          alpha: 0.05,
        },
        risk_signals: [],
        analysis: {
          algorithm_id: 'power',
          dataset_id: dataset.dataset_id,
          dataset_sha256: null,
          columns: [],
          params: {},
          run_id: 'power123-run',
          run_status: 'completed',
        },
      },
    };
    renderView({ controller: { datasets: [dataset] }, messages: [powerMessage] });

    expect(screen.getByText(/结果已生成 · 功效分析/)).toBeInTheDocument();
    expect(screen.queryByText(/结果已生成 · cohort\.csv/)).not.toBeInTheDocument();
    fireEvent.click(screen.getByLabelText('打开分析检查器'));
    expect(await screen.findByText('power123 · 设计阶段计算 · 未使用数据集')).toBeInTheDocument();
  });

  it('keeps analysis settings available after the selected dataset already has a result', async () => {
    const result: SkillResult = {
      schema_version: '1.0',
      payload: { coefficients: [] },
      risk_signals: [],
      analysis: {
        algorithm_id: 'model_linear',
        dataset_id: 'ds-1',
        dataset_sha256: 'a'.repeat(64),
        columns: dataset.columns,
        params: {},
        run_id: 'run-configured',
        run_status: 'completed',
      },
    };
    renderView({
      controller: { datasets: [dataset] },
      messages: [{
        id: 'configured-result',
        role: 'agent',
        content: '分析完成',
        skillResult: result,
        timestamp: new Date(),
      }],
    });

    fireEvent.click(screen.getByLabelText('打开分析检查器'));
    const workspace = (await screen.findAllByLabelText('分析检查器')).find(
      (element) => element.tagName === 'ASIDE',
    );
    expect(workspace).toBeDefined();
    if (!workspace) return;

    // Explicit reconfigure control (preferred over collapse header hit-target).
    const reconfigure = within(workspace).getByLabelText('调整变量或再次分析');
    expect(reconfigure).toBeInTheDocument();
    fireEvent.click(reconfigure);
    expect(within(workspace).getByText('开始统计计算')).toBeInTheDocument();
  });

  it('requires explicit confirmation before calling the backend', async () => {
    runSkillSpy.mockResolvedValue({ schema_version: '1.0', payload: {}, risk_signals: [] });
    renderView({ controller: { datasets: [dataset] } });

    fireEvent.click(screen.getByLabelText('打开分析检查器'));
    fireEvent.click(await screen.findByLabelText('提交分析:ds-1'));
    expect(runSkillSpy).not.toHaveBeenCalled();
    expect(screen.getByRole('dialog', { name: '分析方案审批' })).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: /取\s*消/ }));
    expect(runSkillSpy).not.toHaveBeenCalled();

    fireEvent.click(await screen.findByLabelText('提交分析:ds-1'));
    const approveButton = screen.getByRole('button', { name: '批准方案并运行' });
    await waitFor(() => expect(approveButton).not.toBeDisabled());
    fireEvent.click(approveButton);
    await waitFor(() => expect(runSkillSpy).toHaveBeenCalledTimes(1));
    expect(runSkillSpy.mock.calls[0]?.[1]).toMatchObject({
      plan_id: '33333333-3333-4333-8333-333333333333',
    });
    expect(runSkillSpy.mock.calls[0]?.[1].args).not.toHaveProperty('workflow_approval');
  });

  it('makes protocol approval the first gate of the professional workflow', async () => {
    renderView({ controller: { datasets: [dataset], researchProtocol: null } });

    expect(screen.getByLabelText('研究工作流')).toHaveTextContent('研究协议');
    expect(screen.getByRole('button', { name: '研究协议：未建立' })).toBeInTheDocument();

    fireEvent.click(screen.getByLabelText('打开分析检查器'));
    fireEvent.click(await screen.findByLabelText('提交分析:ds-1'));

    expect(await screen.findByText('研究协议尚未审批')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '批准方案并运行' })).toBeDisabled();
    expect(runSkillSpy).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole('button', { name: '完善并审批协议' }));
    expect(await screen.findByText('研究协议卡')).toBeInTheDocument();
  });

  it('aborts and ignores a confirmed run that completes after switching sessions', async () => {
    let resolveRun!: (result: SkillResult) => void;
    runSkillSpy.mockImplementation(() => new Promise<SkillResult>((resolve) => { resolveRun = resolve; }));
    const view = renderView({ controller: { datasets: [dataset] } });

    fireEvent.click(screen.getByLabelText('打开分析检查器'));
    fireEvent.click(await screen.findByLabelText('提交分析:ds-1'));
    expect(runSkillSpy).not.toHaveBeenCalled();
    const approveButton = screen.getByRole('button', { name: '批准方案并运行' });
    await waitFor(() => expect(approveButton).not.toBeDisabled());
    fireEvent.click(approveButton);
    await waitFor(() => expect(runSkillSpy).toHaveBeenCalledTimes(1));
    const signal = runSkillSpy.mock.calls[0]?.[2] as AbortSignal;

    view.rerender(
      <ProModeView
        controller={makeController({ sessionId: 's2', datasets: [] })}
        chat={makeChat()}
        sessionList={makeList()}
        mode="pro"
        onModeChange={vi.fn()}
        onSend={vi.fn()}
        onChoiceSubmit={vi.fn()}
        onRetry={vi.fn()}
        onVoiceTranscript={vi.fn()}
        model="deepseek-chat"
      />,
    );
    await screen.findByLabelText('打开分析检查器');
    expect(signal.aborted).toBe(true);

    resolveRun({
      schema_version: '1.0',
      payload: {},
      risk_signals: [],
      analysis: {
        algorithm_id: 'model_linear',
        dataset_id: 'ds-1',
        dataset_sha256: 'a'.repeat(64),
        columns: [],
        params: {},
        run_id: 'run-stale',
        run_status: 'completed',
      },
    });

    await Promise.resolve();
    expect(screen.queryByText(/run-stale/)).not.toBeInTheDocument();
    expect(screen.getByLabelText('打开分析检查器')).toBeInTheDocument();
  });
});
