/**
 * Tests for `ChoicePromptCard` — covers single-select, multi-select, and
 * custom-text submission paths (Requirements 4.2, 4.3, 4.6).
 */

import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { ChoicePromptCard } from '../ChoicePromptCard';
import type { ChoicePrompt } from '../../api/types';

function makePrompt(partial: Partial<ChoicePrompt> = {}): ChoicePrompt {
  return {
    prompt_id: '00000000-0000-0000-0000-000000000001',
    question: '请选择一个分析方法',
    options: [
      { option_id: 'linear', text: '线性回归', explanation: '连续因变量' },
      { option_id: 'logistic', text: 'Logistic 回归', explanation: '二分类因变量' },
    ],
    multi_select: false,
    allow_custom_text: false,
    recommendation: null,
    ...partial,
  };
}

describe('ChoicePromptCard — single select', () => {
  it('submits immediately on button click', () => {
    const onSubmit = vi.fn();
    render(<ChoicePromptCard prompt={makePrompt()} onSubmit={onSubmit} />);

    fireEvent.click(screen.getByText('线性回归').closest('button')!);

    expect(onSubmit).toHaveBeenCalledTimes(1);
    expect(onSubmit).toHaveBeenCalledWith(
      expect.objectContaining({
        prompt_id: '00000000-0000-0000-0000-000000000001',
        options: ['linear'],
        custom_text: null,
      }),
    );
  });

  it('marks recommended option with badge', () => {
    const prompt = makePrompt({ recommendation: 'logistic' });
    render(<ChoicePromptCard prompt={prompt} onSubmit={vi.fn()} />);
    expect(screen.getByText('推荐')).toBeInTheDocument();
  });
});

describe('ChoicePromptCard — multi select', () => {
  it('does not submit until 提交 is clicked', () => {
    const onSubmit = vi.fn();
    const prompt = makePrompt({ multi_select: true });
    render(<ChoicePromptCard prompt={prompt} onSubmit={onSubmit} />);

    // Click checkbox
    fireEvent.click(screen.getByRole('checkbox', { name: /线性回归/ }));
    expect(onSubmit).not.toHaveBeenCalled();

    // Click submit button
    fireEvent.click(screen.getByRole('button', { name: /提交/ }));
    expect(onSubmit).toHaveBeenCalledWith(
      expect.objectContaining({ options: ['linear'], custom_text: null }),
    );
  });

  it('submit button stays disabled when no option selected', () => {
    const prompt = makePrompt({ multi_select: true });
    render(<ChoicePromptCard prompt={prompt} onSubmit={vi.fn()} />);
    const submit = screen.getByRole('button', { name: /提交/ }) as HTMLButtonElement;
    expect(submit.disabled).toBe(true);
  });
});

describe('ChoicePromptCard — custom text', () => {
  it('submits with custom_text only when no options selected', () => {
    const onSubmit = vi.fn();
    const prompt = makePrompt({
      multi_select: false,
      allow_custom_text: true,
    });
    render(<ChoicePromptCard prompt={prompt} onSubmit={onSubmit} />);

    const input = screen.getByPlaceholderText('自定义回答') as HTMLInputElement;
    fireEvent.change(input, { target: { value: '我想做方差分析' } });
    fireEvent.keyDown(input, { key: 'Enter' });

    expect(onSubmit).toHaveBeenCalledWith(
      expect.objectContaining({
        options: [],
        custom_text: '我想做方差分析',
      }),
    );
  });
});

describe('ChoicePromptCard — disabled state', () => {
  it('disables further interaction after submission', () => {
    const onSubmit = vi.fn();
    render(<ChoicePromptCard prompt={makePrompt()} onSubmit={onSubmit} />);

    fireEvent.click(screen.getByText('线性回归').closest('button')!);
    expect(onSubmit).toHaveBeenCalledTimes(1);

    // Second click should be ignored (component shows ✓ 已提交 and disables)
    fireEvent.click(screen.getByText('线性回归').closest('button')!);
    expect(onSubmit).toHaveBeenCalledTimes(1);
    expect(screen.getByText(/已提交/)).toBeInTheDocument();
  });
});
