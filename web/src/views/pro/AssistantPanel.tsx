/**
 * AssistantPanel — 专业模式常驻 AI 助手面板（中部下方）。
 *
 * 复用 MessageList + 输入栏 + VoiceRecorder。发送走共享 onSend；流式途中输入与
 * 发送按钮保持可点击（打断式追问，R8.3），发送按钮展示加载态但不禁用，语音保持
 * 可用（R8.5）。
 *
 * Validates: Requirements 8.1, 8.2, 8.3, 8.4, 8.5
 */

import { useCallback, useState } from 'react';
import { Input, Button, theme as antdTheme } from 'antd';
import { SendOutlined, LoadingOutlined } from '@ant-design/icons';
import { MessageList } from '../../components/MessageList';
import { ErrorBanner } from '../../components/ErrorBanner';
import { VoiceRecorder } from '../../components/VoiceRecorder';
import type { UseSseChatReturn } from '../../hooks/useSseChat';
import type { ChoiceAnswer } from '../../api/types';

const { TextArea } = Input;

export interface AssistantPanelProps {
  sessionId: string;
  chat: UseSseChatReturn;
  isArchived: boolean;
  onSend: (text: string) => void;
  onChoiceSubmit: (a: ChoiceAnswer) => void;
  onRetry: () => void;
  onVoiceTranscript: (t: string) => void;
}

export function AssistantPanel({
  sessionId,
  chat,
  isArchived,
  onSend,
  onChoiceSubmit,
  onRetry,
  onVoiceTranscript,
}: AssistantPanelProps) {
  const { token } = antdTheme.useToken();
  const { messages, error, isStreaming } = chat;
  const [inputValue, setInputValue] = useState('');

  const handleSend = useCallback(
    (overrideText?: string) => {
      const text = (overrideText ?? inputValue).trim();
      // 流式途中允许打断追问：不因 isStreaming 阻断发送。
      if (!text || isArchived) return;
      onSend(text);
      if (overrideText === undefined) {
        setInputValue('');
      }
    },
    [inputValue, isArchived, onSend],
  );

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
      if (e.key === 'Enter' && !e.shiftKey && !e.nativeEvent.isComposing) {
        e.preventDefault();
        handleSend();
      }
    },
    [handleSend],
  );

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%', minHeight: 0 }}>
      <div style={{ flex: 1, overflowY: 'auto', paddingRight: 8, minHeight: 0 }}>
        <MessageList messages={messages} onChoiceSubmit={onChoiceSubmit} disabled={isArchived} />
        <ErrorBanner error={error} onRetry={onRetry} />
      </div>

      <div
        style={{
          marginTop: 8,
          padding: 12,
          background: token.colorBgContainer,
          borderRadius: token.borderRadiusLG,
          border: `1px solid ${token.colorBorderSecondary}`,
        }}
      >
        <div style={{ display: 'flex', alignItems: 'flex-end', gap: 8 }}>
          <VoiceRecorder
            sessionId={sessionId}
            onTranscript={onVoiceTranscript}
            // 语音在流式时保持可用；仅只读态禁用。
            disabled={isArchived}
          />
          <TextArea
            value={inputValue}
            onChange={(e) => setInputValue(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder={isArchived ? '当前会话已归档，无法发送消息' : '继续追问，Enter 发送，Shift+Enter 换行'}
            autoSize={{ minRows: 1, maxRows: 5 }}
            // 流式途中保持可输入；仅只读态禁用。
            disabled={isArchived}
            style={{ flex: 1, resize: 'none' }}
            aria-label="助手消息输入框"
          />
          <Button
            type="primary"
            icon={isStreaming ? <LoadingOutlined spin /> : <SendOutlined />}
            onClick={() => handleSend()}
            // 流式途中不禁用（仅展示加载态），实现打断式追问。
            disabled={!inputValue.trim() || isArchived}
            aria-label="发送"
          >
            发送
          </Button>
        </div>
      </div>
    </div>
  );
}

export default AssistantPanel;
