/**
 * AssistantPanel — 专业模式常驻 AI 助手面板（中部下方）。
 *
 * 复用 MessageList + 共享 ChatInputBar（含 VoiceRecorder）。发送走共享 onSend；
 * 流式途中输入、发送、语音保持可用（打断式追问，R8.3/R8.5），仅只读态禁用。
 *
 * Validates: Requirements 8.1, 8.2, 8.3, 8.4, 8.5
 */

import { MessageList } from '../../components/MessageList';
import { ErrorBanner } from '../../components/ErrorBanner';
import { ChatInputBar } from '../../components/ChatInputBar';
import type { UseSseChatReturn } from '../../hooks/useSseChat';
import type { ChoiceAnswer } from '../../api/types';

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
  const { messages, error, isStreaming } = chat;

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%', minHeight: 0 }}>
      <div style={{ flex: 1, overflowY: 'auto', paddingRight: 8, minHeight: 0 }}>
        <MessageList messages={messages} onChoiceSubmit={onChoiceSubmit} disabled={isArchived} />
        <ErrorBanner error={error} onRetry={onRetry} />
      </div>

      <div style={{ marginTop: 8 }}>
        <ChatInputBar
          sessionId={sessionId}
          isStreaming={isStreaming}
          isArchived={isArchived}
          onSend={onSend}
          onVoiceTranscript={onVoiceTranscript}
          placeholder="继续追问，Enter 发送，Shift+Enter 换行"
          inputAriaLabel="助手消息输入框"
          maxRows={5}
        />
      </div>
    </div>
  );
}

export default AssistantPanel;
