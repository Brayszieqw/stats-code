import { fireEvent, render, screen, waitFor } from '@testing-library/react';
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

  it('offers the five-provider catalog, defaulting to DeepSeek', () => {
    render(<OnboardingCard onSubmit={vi.fn()} />);

    fireEvent.mouseDown(screen.getByRole('combobox', { name: 'LLM 提供商' }));
    for (const label of ['DeepSeek 深度求索', '通义千问(DashScope)', 'Kimi(Moonshot)', '智谱 GLM', '自定义(OpenAI 兼容 / 中转)']) {
      // The default-selected label renders twice (closed-selector display +
      // open dropdown option), so assert presence rather than a single match.
      expect(screen.getAllByText(label).length).toBeGreaterThan(0);
    }
  });

  it('switching provider fills in its default base URL and model', () => {
    render(<OnboardingCard onSubmit={vi.fn()} />);

    fireEvent.mouseDown(screen.getByRole('combobox', { name: 'LLM 提供商' }));
    fireEvent.click(screen.getByText('智谱 GLM'));

    expect(screen.getByLabelText('API Base URL')).toHaveValue('https://open.bigmodel.cn/api/paas/v4');
    expect(screen.getByRole('combobox', { name: 'LLM model' })).toHaveValue('glm-4.5');
  });

  it('custom provider starts with empty base URL/model and requires both before enabling submit', () => {
    const onSubmit = vi.fn(async () => {});
    render(<OnboardingCard onSubmit={onSubmit} />);

    fireEvent.mouseDown(screen.getByRole('combobox', { name: 'LLM 提供商' }));
    fireEvent.click(screen.getByText('自定义(OpenAI 兼容 / 中转)'));

    expect(screen.getByLabelText('API Base URL')).toHaveValue('');
    expect(screen.getByRole('combobox', { name: 'LLM model' })).toHaveValue('');
    expect(screen.getByTestId('onboarding-card-submit')).toBeDisabled();

    fireEvent.change(screen.getByLabelText('API Key'), { target: { value: 'sk-relay' } });
    fireEvent.change(screen.getByLabelText('API Base URL'), { target: { value: 'https://relay.example.com/v1' } });
    fireEvent.change(screen.getByRole('combobox', { name: 'LLM model' }), { target: { value: 'gpt-4o' } });

    expect(screen.getByTestId('onboarding-card-submit')).not.toBeDisabled();
    fireEvent.click(screen.getByTestId('onboarding-card-submit'));
    return waitFor(() =>
      expect(onSubmit).toHaveBeenCalledWith('custom', 'sk-relay', 'https://relay.example.com/v1', 'gpt-4o'),
    );
  });

  it('allows freely typing a model id not in the preset list (AutoComplete, not a fixed enum)', () => {
    const onSubmit = vi.fn(async () => {});
    render(<OnboardingCard onSubmit={onSubmit} />);

    fireEvent.change(screen.getByLabelText('API Key'), { target: { value: 'sk-test' } });
    fireEvent.change(screen.getByRole('combobox', { name: 'LLM model' }), { target: { value: 'deepseek-chat-custom-tag' } });

    expect(screen.getByTestId('onboarding-card-submit')).not.toBeDisabled();
    fireEvent.click(screen.getByTestId('onboarding-card-submit'));
    return waitFor(() =>
      expect(onSubmit).toHaveBeenCalledWith('deepseek', 'sk-test', 'https://api.deepseek.com/v1', 'deepseek-chat-custom-tag'),
    );
  });
});
