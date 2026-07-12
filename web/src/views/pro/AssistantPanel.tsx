/**
 * AssistantPanel — 专业模式常驻 AI 助手面板（中部下方）。
 *
 * 空态（无消息）：渲染与简易模式一致的 WelcomeHero 居中组合输入框；
 * 对话态：MessageList + ErrorBanner + 共享 ChatInputBar。
 * 流式途中输入、发送、语音保持可用（打断式追问，R8.3/R8.5），仅只读态禁用。
 *
 * Validates: Requirements 8.1, 8.2, 8.3, 8.4, 8.5
 */

import { MessageList } from '../../components/MessageList';
import { ErrorBanner } from '../../components/ErrorBanner';
import { ChatInputBar } from '../../components/ChatInputBar';
import { WelcomeHero } from '../simple/WelcomeHero';
import type { ChatMessage, UseSseChatReturn } from '../../hooks/useSseChat';
import type { ChoiceAnswer, DatasetSummary } from '../../api/types';

export interface AssistantPanelProps {
  sessionId: string;
  chat: UseSseChatReturn;
  isArchived: boolean;
  onSend: (text: string) => void;
  onChoiceSubmit: (a: ChoiceAnswer) => void;
  onRetry: () => void;
  onVoiceTranscript: (t: string) => void;
  datasets?: DatasetSummary[];
  selectedDatasetId?: string | null;
  modelLabel?: string | null;
  onOpenDatasetPicker?: () => void;
  onOpenSettings?: () => void;
  onOpenVoiceInput?: () => void;
  /** Optional merged message stream (for configured runs not yet reloaded). */
  messages?: ChatMessage[];
  onOpenResult?: (view: 'report' | 'chart' | 'code') => void;
}

export function AssistantPanel({
  sessionId,
  chat,
  isArchived,
  onSend,
  onChoiceSubmit,
  onRetry,
  onVoiceTranscript,
  datasets = [],
  selectedDatasetId = null,
  modelLabel,
  onOpenDatasetPicker,
  onOpenSettings,
  onOpenVoiceInput,
  messages: messageOverride,
  onOpenResult,
}: AssistantPanelProps) {
  const { error, isStreaming } = chat;
  const messages = messageOverride ?? chat.messages;

  // 空态：与简易模式一致的居中欢迎组合输入框。
  if (messages.length === 0) {
    return (
      <div className="assistant-panel__empty" aria-label="欢迎区">
        <div className="stats-welcome__inner">
          <WelcomeHero
            onSend={onSend}
            disabled={isArchived}
            datasets={datasets}
            selectedDatasetId={selectedDatasetId}
            modelLabel={modelLabel}
            onOpenDatasetPicker={onOpenDatasetPicker}
            onOpenSettings={onOpenSettings}
            onOpenVoiceInput={onOpenVoiceInput}
          />
        </div>
      </div>
    );
  }

  return (
    <div className="assistant-panel" aria-label="助手面板">
      <div className="assistant-panel__messages" aria-label="消息列表">
        <MessageList
          messages={messages}
          onChoiceSubmit={onChoiceSubmit}
          disabled={isArchived}
          resultPresentation="reference"
          onOpenResult={onOpenResult}
        />
        <ErrorBanner error={error} onRetry={onRetry} />
      </div>

      <div className="assistant-panel__composer">
        <ChatInputBar
          sessionId={sessionId}
          isStreaming={isStreaming}
          isArchived={isArchived}
          onSend={onSend}
          onVoiceTranscript={onVoiceTranscript}
          placeholder="继续追问，Enter 发送，Shift+Enter 换行"
          inputAriaLabel="助手消息输入框"
          maxRows={5}
          datasets={datasets}
          selectedDatasetId={selectedDatasetId}
          modelLabel={modelLabel}
          onOpenDatasetPicker={onOpenDatasetPicker}
          onOpenSettings={onOpenSettings}
        />
      </div>
    </div>
  );
}

export default AssistantPanel;
