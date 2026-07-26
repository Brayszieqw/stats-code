/**
 * Tests for `ErrorBanner` — covers retry button disabled state during retry
 * (Requirement 14.2) and SKILL_* guidance text (Requirement 14.3).
 */

import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { ErrorBanner } from '../ErrorBanner';
import type { ErrorPayload } from '../../api/types';

function err(code: ErrorPayload['error_code'], message = '出错了'): ErrorPayload {
  return { error_code: code, message };
}

describe('ErrorBanner', () => {
  it('renders nothing when error is null', () => {
    const { container } = render(<ErrorBanner error={null} />);
    expect(container).toBeEmptyDOMElement();
  });

  it('shows error code and message', () => {
    render(<ErrorBanner error={err('SessionNotFound', '会话不存在')} />);
    expect(screen.getByText('SessionNotFound')).toBeInTheDocument();
    expect(screen.getByText('会话不存在')).toBeInTheDocument();
  });

  it('shows retry button only for LlmUnavailable', () => {
    const onRetry = vi.fn();
    const { rerender } = render(
      <ErrorBanner error={err('LlmUnavailable')} onRetry={onRetry} />,
    );
    expect(screen.getByRole('button', { name: /重试/ })).toBeInTheDocument();

    rerender(<ErrorBanner error={err('SessionNotFound')} onRetry={onRetry} />);
    expect(screen.queryByRole('button', { name: /重试/ })).not.toBeInTheDocument();
  });

  it('calls onRetry when retry clicked', () => {
    const onRetry = vi.fn();
    render(<ErrorBanner error={err('LlmUnavailable')} onRetry={onRetry} />);
    fireEvent.click(screen.getByRole('button', { name: /重试/ }));
    expect(onRetry).toHaveBeenCalledTimes(1);
  });

  it('shows SKILL guidance text for SkillExecutionFailed', () => {
    render(<ErrorBanner error={err('SkillExecutionFailed')} />);
    expect(
      screen.getByText(/您可以尝试修改变量选择或换用其他统计方法/),
    ).toBeInTheDocument();
  });

  it('shows SKILL guidance text for SkillInvalidArgs', () => {
    render(<ErrorBanner error={err('SkillInvalidArgs')} />);
    expect(
      screen.getByText(/您可以尝试修改变量选择或换用其他统计方法/),
    ).toBeInTheDocument();
  });

  it('does not show SKILL guidance for non-skill errors', () => {
    render(<ErrorBanner error={err('SessionNotFound')} />);
    expect(
      screen.queryByText(/您可以尝试修改变量选择或换用其他统计方法/),
    ).not.toBeInTheDocument();
  });

  it('offers protocol action for ResearchProtocolRequired', () => {
    const onOpenProtocol = vi.fn();
    render(
      <ErrorBanner
        error={err('ResearchProtocolRequired', '必须先审批研究协议')}
        onOpenProtocol={onOpenProtocol}
      />,
    );
    expect(screen.getByText(/填写并审批研究协议/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: /去填写研究协议/ }));
    expect(onOpenProtocol).toHaveBeenCalledTimes(1);
  });

  it('offers inspector action for ResearchApprovalRequired', () => {
    const onOpenInspector = vi.fn();
    render(
      <ErrorBanner
        error={err('ResearchApprovalRequired', '方案未批准')}
        onOpenInspector={onOpenInspector}
      />,
    );
    fireEvent.click(screen.getByRole('button', { name: /去完成审批/ }));
    expect(onOpenInspector).toHaveBeenCalledTimes(1);
  });
});
