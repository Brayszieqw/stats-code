/**
 * ChatInputBar — shared conversation input row used by both SimpleModeView and
 * the Pro-mode AssistantPanel.
 *
 * Owns the local draft state and the send/keydown handlers so the two call
 * sites no longer duplicate them. Behaviour contract:
 *   - Enter sends, Shift+Enter inserts a newline; IME composition is respected.
 *   - Streaming does NOT disable text input, the send button, or voice — this
 *     enables interruptive follow-up (R8.3, R8.5). The send button shows a
 *     loading icon while streaming but stays clickable.
 *   - Archived (read-only) sessions disable the textarea, send, and voice
 *     (R9.3); the parent's ModeToggle is unaffected.
 *
 * Validates: Requirements 8.3, 8.5, 9.3
 */

import { useCallback, useState } from 'react';
import { Button, Input, Typography, theme as antdTheme } from 'antd';
import {
  SendOutlined,
  LoadingOutlined,
  DatabaseOutlined,
  SettingOutlined,
} from '@ant-design/icons';
import { VoiceRecorder } from './VoiceRecorder';
import type { DatasetSummary } from '../api/types';

const { Text } = Typography;
const { TextArea } = Input;

export interface ChatInputBarProps {
  sessionId: string;
  isStreaming: boolean;
  isArchived: boolean;
  onSend: (text: string) => void;
  onVoiceTranscript: (text: string) => void;
  placeholder?: string;
  archivedPlaceholder?: string;
  /** Accessible label for the textarea (distinguishes the two call sites). */
  inputAriaLabel?: string;
  /** Optional footnote rendered under the input row. */
  footer?: React.ReactNode;
  minRows?: number;
  maxRows?: number;
  /** Optional compact toolbar (kept in conversation mode, not only welcome). */
  datasets?: DatasetSummary[];
  selectedDatasetId?: string | null;
  modelLabel?: string | null;
  onOpenDatasetPicker?: () => void;
  onOpenSettings?: () => void;
}

export function ChatInputBar({
  sessionId,
  isStreaming,
  isArchived,
  onSend,
  onVoiceTranscript,
  placeholder = '输入统计分析问题，Enter 发送，Shift+Enter 换行',
  archivedPlaceholder = '当前会话已归档，无法发送消息',
  inputAriaLabel = '消息输入框',
  footer,
  minRows = 1,
  maxRows = 6,
  datasets = [],
  selectedDatasetId = null,
  modelLabel,
  onOpenDatasetPicker,
  onOpenSettings,
}: ChatInputBarProps) {
  const { token } = antdTheme.useToken();
  const [inputValue, setInputValue] = useState('');

  const handleSend = useCallback(() => {
    const text = inputValue.trim();
    // Streaming does not block send (interruptive follow-up); only archived does.
    if (!text || isArchived) return;
    onSend(text);
    setInputValue('');
  }, [inputValue, isArchived, onSend]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
      if (e.key === 'Enter' && !e.shiftKey && !e.nativeEvent.isComposing) {
        e.preventDefault();
        handleSend();
      }
    },
    [handleSend],
  );

  const selectedDataset = datasets.find((d) => d.dataset_id === selectedDatasetId) ?? null;
  const datasetLabel = selectedDataset
    ? selectedDataset.file_name
    : datasets.length > 0
      ? `${datasets.length} 个数据集`
      : '选择数据集';
  const showToolbar = Boolean(onOpenDatasetPicker || onOpenSettings);

  return (
    <div
      className="chat-input-shell"
      style={{
        padding: 12,
        background: token.colorBgContainer,
        borderRadius: token.borderRadiusLG,
        boxShadow: token.boxShadowTertiary,
        border: `1px solid ${token.colorBorderSecondary}`,
      }}
    >
      {showToolbar ? (
        <div
          className="chat-input-toolbar"
          style={{
            display: 'flex',
            alignItems: 'center',
            gap: 8,
            marginBottom: 8,
            flexWrap: 'wrap',
          }}
        >
          {onOpenDatasetPicker ? (
            <Button
              type="text"
              size="small"
              icon={<DatabaseOutlined />}
              aria-label="选择数据集"
              onClick={onOpenDatasetPicker}
              disabled={isArchived}
            >
              {datasetLabel}
            </Button>
          ) : null}
          {onOpenSettings ? (
            <Button
              type="text"
              size="small"
              icon={<SettingOutlined />}
              aria-label="模型选择"
              onClick={onOpenSettings}
            >
              {modelLabel?.trim() || '模型设置'}
            </Button>
          ) : null}
        </div>
      ) : null}
      <div style={{ display: 'flex', alignItems: 'flex-end', gap: 8 }}>
        <VoiceRecorder
          sessionId={sessionId}
          onTranscript={onVoiceTranscript}
          // Voice stays usable mid-stream; only archived disables it (R8.5).
          disabled={isArchived}
        />
        <TextArea
          value={inputValue}
          onChange={(e) => setInputValue(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder={isArchived ? archivedPlaceholder : placeholder}
          autoSize={{ minRows, maxRows }}
          disabled={isArchived}
          style={{ flex: 1, resize: 'none' }}
          aria-label={inputAriaLabel}
        />
        <Button
          type="primary"
          icon={isStreaming ? <LoadingOutlined spin /> : <SendOutlined />}
          onClick={handleSend}
          disabled={!inputValue.trim() || isArchived}
          size="large"
          aria-label="发送"
        >
          发送
        </Button>
      </div>
      {footer ? (
        <div style={{ marginTop: 6, textAlign: 'right' }}>
          <Text type="secondary" style={{ fontSize: 11 }}>
            {footer}
          </Text>
        </div>
      ) : null}
    </div>
  );
}

export default ChatInputBar;
