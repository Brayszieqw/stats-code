/**
 * OnboardingCard — full-screen masking dialog shown when the backend reports
 * `LlmStatus.configured === false` (Requirement 11.1). It collects the LLM
 * provider + API Key and hands them to the parent via `onSubmit`. The actual
 * `POST /api/llm-config` wiring lives in task 11.3 / `ChatPage`; this
 * component is presentation + local form state only.
 *
 * Behavior contract (this task — 11.2):
 *   - Renders an overlay covering the viewport (full-screen mask) so the
 *     surrounding chat UI is visually + functionally blocked. The overlay
 *     itself eats pointer events; parents can additionally apply
 *     `pointer-events: none` to the main container per Requirement 11.1.
 *   - The dialog box on top of the overlay has `role="dialog"` and
 *     `aria-modal="true"` so assistive tech treats it as a modal.
 *   - Provider select offers the LLM provider catalog (`../api/llm-catalog`).
 *   - API Key field is `<input type="password">` so the value is masked.
 *   - 「测试并保存」 button is disabled while `submitting` is true or while
 *     the API Key (trimmed) is empty (Requirement 11.2 + 11.5 — pressing
 *     the button without a key would otherwise yield a guaranteed 422).
 *   - When `errorMessage` is non-empty, it is rendered in red inside the
 *     card (Requirement 11.5: card stays visible after a failed probe).
 *
 * Wiring is left to the parent:
 *   - `onSubmit(provider, apiKey)` returns a Promise. The parent decides
 *     when to flip `submitting` back to false and what `errorMessage` to
 *     surface; OnboardingCard does not catch its own rejection.
 *
 * Validates: Requirements 11.1, 11.2
 */

import { useState, type FormEvent } from 'react';
import { Alert, AutoComplete, Button, Input, Select, Space, Typography } from 'antd';
import type { LlmProvider } from '../api/types';
import {
  DEFAULT_BASE_URLS,
  getBaseUrlPlaceholder,
  getDefaultModel,
  getModelOptions,
  getModelPlaceholder,
  PROVIDER_OPTIONS,
} from '../api/llm-catalog';

const { Title, Paragraph } = Typography;

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

export interface OnboardingCardProps {
  /**
   * Invoked when the user clicks 「测试并保存」. Returns a Promise so the
   * parent can flip `submitting` back to false once the round-trip is done.
   */
  onSubmit: (provider: LlmProvider, apiKey: string, baseUrl: string, model: string) => Promise<void>;
  /** Continue with the deterministic local engine and configure LLM later. */
  onSkip?: () => void;
  /** True while the parent is mid-flight on `POST /api/llm-config`. */
  submitting?: boolean;
  /** Error text rendered in red inside the card after a failed probe. */
  errorMessage?: string | null;
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function OnboardingCard({
  onSubmit,
  onSkip,
  submitting = false,
  errorMessage = null,
}: OnboardingCardProps) {
  const [provider, setProvider] = useState<LlmProvider>('deepseek');
  const [apiKey, setApiKey] = useState('');
  const [baseUrl, setBaseUrl] = useState(DEFAULT_BASE_URLS.deepseek);
  const [model, setModel] = useState(getDefaultModel('deepseek'));
  const [isBaseUrlDirty, setIsBaseUrlDirty] = useState(false);

  const handleProviderChange = (next: LlmProvider) => {
    setProvider(next);
    if (!isBaseUrlDirty) {
      setBaseUrl(DEFAULT_BASE_URLS[next]);
    }
    setModel(getDefaultModel(next));
  };

  const trimmedKey = apiKey.trim();
  const trimmedUrl = baseUrl.trim();
  const selectedModel = model.trim();
  const canSubmit = !submitting && trimmedKey.length > 0 && trimmedUrl.length > 0 && selectedModel.length > 0;

  const handleSubmit = (event?: FormEvent) => {
    event?.preventDefault();
    if (!canSubmit) return;
    // Fire and forget — parent decides what to do with the Promise.
    void onSubmit(provider, trimmedKey, trimmedUrl, selectedModel);
  };

  return (
    <div
      // Full-screen backdrop. The overlay itself catches pointer events so
      // the chat UI underneath is functionally inert while the card is up.
      data-testid="onboarding-card-overlay"
      style={{
        position: 'fixed',
        inset: 0,
        zIndex: 1000,
        background: 'rgba(15, 23, 42, 0.45)',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        padding: 16,
      }}
    >
      <form
        role="dialog"
        aria-modal="true"
        aria-labelledby="onboarding-card-title"
        onSubmit={handleSubmit}
        style={{
          width: '100%',
          maxWidth: 420,
          background: '#ffffff',
          borderRadius: 12,
          boxShadow: '0 12px 32px rgba(0, 0, 0, 0.18)',
          padding: 24,
        }}
      >
        <Space direction="vertical" size={16} style={{ width: '100%' }}>
          <div>
            <Title id="onboarding-card-title" level={4} style={{ marginBottom: 4 }}>
              配置 LLM 服务
            </Title>
            <Paragraph type="secondary" style={{ marginBottom: 0, fontSize: 13 }}>
              LLM 只用于自然语言引导与解释；本机统计引擎无需 LLM，也能直接运行确定性分析。
            </Paragraph>
          </div>

          <label style={{ display: 'block' }}>
            <Paragraph style={{ marginBottom: 4, fontSize: 13 }}>提供商</Paragraph>
            <Select<LlmProvider>
              value={provider}
              onChange={handleProviderChange}
              options={PROVIDER_OPTIONS}
              disabled={submitting}
              style={{ width: '100%' }}
              aria-label="LLM 提供商"
            />
          </label>

          <label style={{ display: 'block' }}>
            <Paragraph style={{ marginBottom: 4, fontSize: 13 }}>API Base URL</Paragraph>
            <Input
              value={baseUrl}
              onChange={(e) => {
                setBaseUrl(e.target.value);
                setIsBaseUrlDirty(true);
              }}
              placeholder={getBaseUrlPlaceholder(provider)}
              disabled={submitting}
              aria-label="API Base URL"
            />
          </label>

          <label style={{ display: 'block' }}>
            <Paragraph style={{ marginBottom: 4, fontSize: 13 }}>Model</Paragraph>
            <AutoComplete
              value={model}
              onChange={setModel}
              options={getModelOptions(provider)}
              placeholder={getModelPlaceholder(provider)}
              disabled={submitting}
              style={{ width: '100%' }}
              aria-label="LLM model"
            />
          </label>

          <label style={{ display: 'block' }}>
            <Paragraph style={{ marginBottom: 4, fontSize: 13 }}>API Key</Paragraph>
            <Input.Password
              value={apiKey}
              onChange={(e) => setApiKey(e.target.value)}
              placeholder="sk-..."
              autoComplete="off"
              disabled={submitting}
              aria-label="API Key"
              // type=password is implied by Input.Password (visibility toggle
              // is opt-in via the eye icon, default state is masked).
            />
          </label>

          {errorMessage ? (
            <Alert
              type="error"
              showIcon
              role="alert"
              data-testid="onboarding-card-error"
              message={
                <span style={{ color: '#cf1322' }}>{errorMessage}</span>
              }
            />
          ) : null}

          <Button
            type="primary"
            htmlType="submit"
            block
            loading={submitting}
            disabled={!canSubmit}
            data-testid="onboarding-card-submit"
          >
            测试并保存
          </Button>
          {onSkip ? (
            <Button type="text" block onClick={onSkip} disabled={submitting}>
              暂不配置，进入专业模式
            </Button>
          ) : null}
        </Space>
      </form>
    </div>
  );
}

export default OnboardingCard;
