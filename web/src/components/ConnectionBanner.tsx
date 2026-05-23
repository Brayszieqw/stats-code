/**
 * ConnectionBanner — top-of-page banner shown after a runtime LLM failure
 * (Requirement 12.1). Renders the failing provider name and a short summary,
 * plus two recovery actions:
 *
 *   - 「重试」 (`onRetry`)     — resends the last user message (R12.3).
 *   - 「修改 Key」(`onEditKey`) — forces Onboarding_Card back on screen
 *                              (R12.4).
 *
 * The component is purely presentational: it has no internal state and no
 * side effects beyond invoking the supplied callbacks. All state lives in
 * `bannerReducer`, which decides when to hide the banner (R12.5). When
 * `view.visible === false` the component renders nothing.
 *
 * ─── Wiring with the chat hook (Requirements 12.1 / 12.5) ───────────────
 *
 * The chat hook (`useSseChat`) is intentionally left untouched in this
 * task — UI wiring decisions about when exactly to dispatch banner events
 * belong outside the streaming primitive itself. A parent component owns
 * the reducer and translates chat-hook signals into banner events:
 *
 *   ```tsx
 *   import { useReducer } from 'react';
 *   import {
 *     bannerReducer, initialBannerState,
 *   } from '../lib/bannerReducer';
 *   import { ConnectionBanner } from '../components/ConnectionBanner';
 *
 *   function ChatShell() {
 *     const { sendMessage, error, status } = useSseChat(sessionId);
 *     const llmStatus = useLlmStatus();
 *     const [bannerState, dispatch] = useReducer(
 *       bannerReducer, initialBannerState,
 *     );
 *
 *     // Error → banner (R12.1)
 *     useEffect(() => {
 *       if (error?.error_code === 'LlmUnavailable'
 *           || error?.error_code === 'LLM_UPSTREAM_ERROR') {
 *         dispatch({
 *           type: 'Error',
 *           provider: llmStatus.provider,
 *           summary: error.message,
 *         });
 *       }
 *     }, [error, llmStatus.provider]);
 *
 *     // Success → banner hide (R12.5)
 *     useEffect(() => {
 *       if (bannerState.banner.visible
 *           && status === 'streaming' && !error) {
 *         dispatch({ type: 'Success' });
 *       }
 *     }, [status, error, bannerState.banner.visible]);
 *
 *     return (
 *       <>
 *         <ConnectionBanner
 *           view={bannerState.banner}
 *           onRetry={() => sendMessage(lastUserMessage)} // R12.3
 *           onEditKey={() => {                          // R12.4
 *             dispatch({ type: 'EditKey' });
 *             llmStatus.requireReconfigure();
 *           }}
 *         />
 *         {/* …chat surface… *\/}
 *       </>
 *     );
 *   }
 *   ```
 *
 * `useConnectionBanner` (`web/src/hooks/useConnectionBanner.ts`) packages
 * exactly this glue and is the recommended consumer; this component stays
 * decoupled from any specific hook.
 *
 * Validates: Requirements 12.1, 12.2, 12.3, 12.4, 12.5
 */

import { Alert, Button, Space, Typography } from 'antd';
import { KeyOutlined, ReloadOutlined } from '@ant-design/icons';
import type { BannerView } from '../lib/bannerReducer';
import type { LlmProvider } from '../api/types';

const { Text } = Typography;

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

export interface ConnectionBannerProps {
  /** Banner view-model produced by `bannerReducer`. */
  view: BannerView;
  /** Resend the last user message (Requirement 12.3). */
  onRetry: () => void;
  /** Force Onboarding_Card to re-display (Requirement 12.4). */
  onEditKey: () => void;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const PROVIDER_LABELS: Record<LlmProvider, string> = {
  deepseek: 'DeepSeek',
  openai: 'OpenAI',
};

function providerLabel(provider: LlmProvider | null | undefined): string {
  if (provider == null) return 'LLM';
  return PROVIDER_LABELS[provider];
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function ConnectionBanner({
  view,
  onRetry,
  onEditKey,
}: ConnectionBannerProps) {
  // Render nothing when the reducer says the banner is hidden (R12.5).
  if (!view.visible) return null;

  const summary = view.summary ?? '与 AI 服务的连接出现异常';
  const label = providerLabel(view.provider);

  return (
    <Alert
      type="error"
      showIcon
      // `role="alert"` makes assistive tech announce the failure
      // immediately, matching R12.1's "明显的提示".
      role="alert"
      data-testid="connection-banner"
      style={{ marginBottom: 12 }}
      message={
        <Space direction="vertical" size={2} style={{ width: '100%' }}>
          <Text strong>{`${label} 服务异常`}</Text>
          <Text type="secondary" style={{ fontSize: 12 }}>
            {summary}
          </Text>
        </Space>
      }
      action={
        <Space size={6}>
          <Button
            size="small"
            type="primary"
            icon={<ReloadOutlined />}
            onClick={onRetry}
          >
            重试
          </Button>
          <Button size="small" icon={<KeyOutlined />} onClick={onEditKey}>
            修改 Key
          </Button>
        </Space>
      }
    />
  );
}

export default ConnectionBanner;
