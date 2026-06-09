/**
 * SimpleModeView — 简易模式视图（参考 MiroFish 极简首页）。
 *
 * 组装 SimpleSidebar（左导航 + 历史会话）+ 右上 ModeToggle：
 *   - 欢迎态（messages.length === 0）：居中 WelcomeHero + SuggestionCards。
 *   - 对话态：上方 MessageList + ErrorBanner，下方输入栏（VoiceRecorder +
 *     TextArea + 发送）。
 * 内联展示 SkillResult / ChoicePrompt 由 MessageList 负责。Archived 时禁用
 * 发送 / 上传 / 选择，但 ModeToggle 保持可用。
 *
 * Validates: Requirements 2.3, 3.1, 3.2, 3.3, 3.4, 3.5, 3.6, 3.7, 9.3, 9.4
 */

import { useCallback, useRef, useState } from 'react';
import {
  Layout,
  Input,
  Button,
  Typography,
  Alert,
  theme as antdTheme,
} from 'antd';
import { SendOutlined, LoadingOutlined } from '@ant-design/icons';
import { SimpleSidebar } from './simple/SimpleSidebar';
import { WelcomeHero } from './simple/WelcomeHero';
import { SuggestionCards } from './simple/SuggestionCards';
import { ModeToggle } from '../components/ModeToggle';
import { MessageList } from '../components/MessageList';
import { ErrorBanner } from '../components/ErrorBanner';
import { VoiceRecorder } from '../components/VoiceRecorder';
import type { SessionController } from '../hooks/useSessionController';
import type { UseSseChatReturn } from '../hooks/useSseChat';
import type { UseSessionListReturn } from '../hooks/useSessionList';
import type { ViewMode } from '../hooks/useModePreference';
import type { ChoiceAnswer } from '../api/types';

const { Sider, Header, Content } = Layout;
const { Text } = Typography;
const { TextArea } = Input;

const SIDER_WIDTH = 280;

export interface SimpleModeViewProps {
  controller: SessionController;
  chat: UseSseChatReturn;
  sessionList: UseSessionListReturn;
  mode: ViewMode;
  onModeChange: (m: ViewMode) => void;
  onSend: (text: string) => void;
  onChoiceSubmit: (a: ChoiceAnswer) => void;
  onRetry: () => void;
  onVoiceTranscript: (t: string) => void;
}

export function SimpleModeView({
  controller,
  chat,
  sessionList,
  mode,
  onModeChange,
  onSend,
  onChoiceSubmit,
  onRetry,
  onVoiceTranscript,
}: SimpleModeViewProps) {
  const { token } = antdTheme.useToken();
  const { messages, error, isStreaming } = chat;
  const { isArchived, sessionId } = controller;

  const isWelcome = messages.length === 0;

  // ─── Conversation-mode input bar ────────────────────────────────────────
  const [inputValue, setInputValue] = useState('');
  const textAreaRef = useRef<{ resizableTextArea?: { textArea: HTMLTextAreaElement } }>(null);

  const handleSend = useCallback(
    (overrideText?: string) => {
      const text = (overrideText ?? inputValue).trim();
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
    <Layout style={{ height: '100vh' }}>
      <Sider
        width={SIDER_WIDTH}
        style={{
          background: token.colorBgContainer,
          borderRight: `1px solid ${token.colorBorderSecondary}`,
          padding: 16,
          overflowY: 'auto',
        }}
        breakpoint="md"
        collapsedWidth={0}
      >
        <SimpleSidebar
          sessionList={sessionList}
          activeSessionId={sessionId}
          onNewSession={() => {
            void controller.startNewSession();
          }}
          onSelectSession={(sid) => {
            void controller.loadSession(sid);
          }}
        />
      </Sider>

      <Layout>
        <Header
          style={{
            background: token.colorBgContainer,
            borderBottom: `1px solid ${token.colorBorderSecondary}`,
            padding: '0 24px',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'flex-end',
            height: 56,
            lineHeight: '56px',
          }}
        >
          <ModeToggle mode={mode} onChange={onModeChange} />
        </Header>

        <Content
          style={{
            display: 'flex',
            flexDirection: 'column',
            padding: 16,
            background: token.colorBgLayout,
            overflow: 'hidden',
          }}
        >
          {isArchived && (
            <Alert
              message="此会话已归档 (只读模式)"
              description="该会话处于只读状态。您无法再发送消息或进行选择。"
              type="warning"
              showIcon
              style={{ marginBottom: 16, maxWidth: 880, width: '100%', margin: '0 auto 16px' }}
            />
          )}

          {isWelcome ? (
            <div
              style={{
                flex: 1,
                overflowY: 'auto',
                display: 'flex',
                flexDirection: 'column',
                justifyContent: 'center',
                alignItems: 'center',
                padding: '24px 16px',
              }}
            >
              <WelcomeHero onSend={(t) => handleSend(t)} disabled={isArchived} />
              <SuggestionCards onSend={(t) => handleSend(t)} disabled={isArchived} />
            </div>
          ) : (
            <>
              <div
                style={{
                  flex: 1,
                  overflowY: 'auto',
                  paddingRight: 8,
                  maxWidth: 880,
                  width: '100%',
                  margin: '0 auto',
                }}
              >
                <MessageList messages={messages} onChoiceSubmit={onChoiceSubmit} disabled={isArchived} />
                <ErrorBanner error={error} onRetry={onRetry} />
              </div>

              <div
                style={{
                  maxWidth: 880,
                  width: '100%',
                  margin: '12px auto 0',
                  padding: 12,
                  background: token.colorBgContainer,
                  borderRadius: token.borderRadiusLG,
                  boxShadow: token.boxShadowTertiary,
                  border: `1px solid ${token.colorBorderSecondary}`,
                }}
              >
                <div style={{ display: 'flex', alignItems: 'flex-end', gap: 8 }}>
                  <VoiceRecorder
                    sessionId={sessionId}
                    onTranscript={onVoiceTranscript}
                    disabled={isStreaming || isArchived}
                  />
                  <TextArea
                    ref={textAreaRef as never}
                    value={inputValue}
                    onChange={(e) => setInputValue(e.target.value)}
                    onKeyDown={handleKeyDown}
                    placeholder={
                      isArchived
                        ? '当前会话已归档，无法发送消息'
                        : '输入统计分析问题，Enter 发送，Shift+Enter 换行'
                    }
                    autoSize={{ minRows: 1, maxRows: 6 }}
                    disabled={isArchived}
                    style={{ flex: 1, resize: 'none' }}
                    aria-label="消息输入框"
                  />
                  <Button
                    type="primary"
                    icon={isStreaming ? <LoadingOutlined spin /> : <SendOutlined />}
                    onClick={() => handleSend()}
                    disabled={!inputValue.trim() || isArchived}
                    size="large"
                    aria-label="发送"
                  >
                    发送
                  </Button>
                </div>
                <div
                  style={{
                    marginTop: 6,
                    fontSize: 11,
                    color: token.colorTextTertiary,
                    textAlign: 'right',
                  }}
                >
                  <Text type="secondary" style={{ fontSize: 11 }}>
                    结果由 AI 生成，请结合专业判断
                  </Text>
                </div>
              </div>
            </>
          )}
        </Content>
      </Layout>
    </Layout>
  );
}

export default SimpleModeView;
