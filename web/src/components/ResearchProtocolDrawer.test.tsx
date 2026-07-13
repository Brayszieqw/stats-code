import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { ProtocolCompileResult, ResearchProtocolInput } from '../api/types';
import { ResearchProtocolDrawer } from './ResearchProtocolDrawer';

describe('ResearchProtocolDrawer', () => {
  it('loads the demo protocol and submits an approved 15-field card', async () => {
    const onSave = vi.fn(async (_input: ResearchProtocolInput) => {});
    render(
      <ResearchProtocolDrawer
        open
        protocol={null}
        onClose={vi.fn()}
        onSave={onSave}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: '加载演示协议' }));
    fireEvent.click(screen.getByRole('button', { name: '审批协议' }));

    await waitFor(() => expect(onSave).toHaveBeenCalledTimes(1));
    expect(onSave.mock.calls[0]?.[0]).toMatchObject({
      status: 'Approved',
      study_design: 'cross_sectional',
      outcome: 'disease（二分类疾病结局）',
      time_zero: '基线调查时点',
    });
    expect(Object.keys(onSave.mock.calls[0]?.[0] ?? {})).toHaveLength(16);
  });

  it('compiles a brief into an editable draft but never saves or approves automatically', async () => {
    const compiled: ProtocolCompileResult = {
      schema_version: '1.0',
      compiler_version: '1.0.0',
      proposal: {
        research_question: '编译后的研究问题',
        study_design: 'cohort',
        population: '成人队列',
        eligibility_criteria: '',
        exposure: '吸烟',
        comparator: '未吸烟',
        outcome: '一年疾病结局',
        time_zero: '基线',
        follow_up: '一年',
        analysis_unit: '参与者',
        estimand: '调整后风险比',
        confounders: '年龄、性别',
        missing_data_strategy: '报告缺失率',
        primary_analysis: '多变量回归',
        sensitivity_analysis: '',
      },
      missing_required_fields: [],
      warnings: ['请人工核对变量定义'],
      brief_sha256: 'a'.repeat(64),
      approval_required: true,
    };
    const onCompile = vi.fn(async () => compiled);
    const onSave = vi.fn(async (_input: ResearchProtocolInput) => {});
    render(
      <ResearchProtocolDrawer
        open
        protocol={null}
        onClose={vi.fn()}
        onCompile={onCompile}
        onSave={onSave}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'AI 编译草稿' }));
    fireEvent.change(screen.getByLabelText('研究摘要'), {
      target: { value: '研究成人队列中吸烟与一年疾病结局的关联，并生成协议草稿。' },
    });
    fireEvent.click(screen.getByRole('button', { name: '编译为草稿' }));

    await screen.findByText('AI 草稿已回填，尚未保存或审批。');
    expect(onCompile).toHaveBeenCalledWith('研究成人队列中吸烟与一年疾病结局的关联，并生成协议草稿。');
    expect(onSave).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole('button', { name: '保存草稿' }));
    await waitFor(() => expect(onSave).toHaveBeenCalledTimes(1));
    expect(onSave.mock.calls[0]?.[0]).toMatchObject({
      status: 'Draft',
      research_question: '编译后的研究问题',
      outcome: '一年疾病结局',
    });
  });
});
