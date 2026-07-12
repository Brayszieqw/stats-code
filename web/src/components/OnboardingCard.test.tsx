import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { OnboardingCard } from './OnboardingCard';

describe('OnboardingCard', () => {
  it('offers an explicit local-engine path without requiring an API key', () => {
    const onSubmit = vi.fn(async () => {});
    const onSkip = vi.fn();

    render(<OnboardingCard onSubmit={onSubmit} onSkip={onSkip} />);

    expect(screen.getByText(/本机统计引擎无需 LLM/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: '暂不配置，进入专业模式' }));
    expect(onSkip).toHaveBeenCalledTimes(1);
    expect(onSubmit).not.toHaveBeenCalled();
  });
});
