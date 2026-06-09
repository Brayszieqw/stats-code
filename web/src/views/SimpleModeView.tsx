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

import {
  Layout,
  Alert,
  theme as antdTheme,
} from 'antd';
import { SimpleSidebar } from './simple/SimpleSidebar';
import { WelcomeHero } from './simple/WelcomeHero';
import { SuggestionCards } from './simple/SuggestionCards';
import { ModeToggle } from '../components/ModeToggle';
import { MessageList } from '../components/MessageList';
import { ErrorBanner } from '../components/ErrorBanner';
import { ChatInputBar } from '../components/ChatInputBar';
import type { SessionController } from '../hooks/useSessionController';
import type { UseSseChatReturn } from '../hooks/useSseChat';
import type { UseSessionListReturn } from '../hooks/useSessionList';
import type { ViewMode } from '../hooks/useModePreference';
import type { ChoiceAnswer } from '../api/types';

const { Sider, Header, Content } = Layout;

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
              <WelcomeHero onSend={onSend} disabled={isArchived} />
              <SuggestionCards onSend={onSend} disabled={isArchived} />
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

              <div style={{ maxWidth: 880, width: '100%', margin: '12px auto 0' }}>
                <ChatInputBar
                  sessionId={sessionId}
                  isStreaming={isStreaming}
                  isArchived={isArchived}
                  onSend={onSend}
                  onVoiceTranscript={onVoiceTranscript}
                  footer="结果由 AI 生成，请结合专业判断"
                />
              </div>
            </>
          )}
        </Content>
      </Layout>
    </Layout>
  );
}

export default SimpleModeView;
