import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { ResearchWorkflowBar } from './ResearchWorkflowBar';

describe('ResearchWorkflowBar', () => {
  it('shows the five research states and opens the protocol card', () => {
    const onOpenProtocol = vi.fn();
    render(
      <ResearchWorkflowBar
        protocol={null}
        datasetReady
        auditStatus="blocked"
        planApproved={false}
        isRunning={false}
        resultReady={false}
        onOpenProtocol={onOpenProtocol}
      />,
    );

    expect(screen.getByLabelText('研究工作流')).toHaveTextContent('研究协议');
    expect(screen.getByLabelText('研究工作流')).toHaveTextContent('数据质控（已阻断）');
    expect(screen.getByLabelText('研究工作流')).toHaveTextContent('方案审批');
    expect(screen.getByLabelText('研究工作流')).toHaveTextContent('确定性执行');
    expect(screen.getByLabelText('研究工作流')).toHaveTextContent('诊断与导出');
    fireEvent.click(screen.getByRole('button', { name: '研究协议：未建立' }));
    expect(onOpenProtocol).toHaveBeenCalledTimes(1);
  });
});
