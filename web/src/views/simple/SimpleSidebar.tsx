/**
 * SimpleSidebar — left navigation for the simple mode: new-conversation entry,
 * placeholder search/plugins/automation entries, a project group, and the
 * history-session list (consumes useSessionList). History errors render a
 * non-blocking placeholder.
 *
 * Validates: Requirements 2.1, 2.2, 9.6, 9.7, 11.1
 */

import { Button, Space, Typography, Empty, message, theme as antdTheme } from 'antd';
import {
  PlusOutlined,
  SearchOutlined,
  ApiOutlined,
  ThunderboltOutlined,
  HistoryOutlined,
  FolderOutlined,
} from '@ant-design/icons';
import type { UseSessionListReturn } from '../../hooks/useSessionList';

const { Text, Title } = Typography;

export interface SimpleSidebarProps {
  sessionList: UseSessionListReturn;
  /** Start a brand-new conversation. */
  onNewSession: () => void;
  /** Load a history session by id. */
  onSelectSession: (sessionId: string) => void;
  /** Currently active session id (for highlight). */
  activeSessionId?: string;
}

const comingSoon = () => message.info('即将推出');

export function SimpleSidebar({
  sessionList,
  onNewSession,
  onSelectSession,
  activeSessionId,
}: SimpleSidebarProps) {
  const { token } = antdTheme.useToken();
  const { sessions, error } = sessionList;

  return (
    <Space direction="vertical" size={16} style={{ width: '100%' }}>
      <Title level={4} style={{ margin: 0 }}>
        <ThunderboltOutlined style={{ color: token.colorPrimary }} /> Stats 智能分析
      </Title>

      <Button type="primary" block icon={<PlusOutlined />} onClick={onNewSession} aria-label="新对话">
        新对话
      </Button>

      <Space direction="vertical" size={4} style={{ width: '100%' }}>
        <Button type="text" block icon={<SearchOutlined />} onClick={comingSoon} style={{ textAlign: 'left' }} aria-label="搜索">
          搜索
        </Button>
        <Button type="text" block icon={<ApiOutlined />} onClick={comingSoon} style={{ textAlign: 'left' }} aria-label="插件">
          插件
        </Button>
        <Button type="text" block icon={<ThunderboltOutlined />} onClick={comingSoon} style={{ textAlign: 'left' }} aria-label="自动化">
          自动化
        </Button>
      </Space>

      <div>
        <Text strong style={{ fontSize: 13 }}>
          <FolderOutlined /> 项目
        </Text>
        <div style={{ marginTop: 6 }}>
          <Text type="secondary" style={{ fontSize: 12 }}>默认项目</Text>
        </div>
      </div>

      <div>
        <Text strong style={{ fontSize: 13 }}>
          <HistoryOutlined /> 历史会话
        </Text>
        {error ? (
          <div style={{ marginTop: 8 }} role="note">
            <Text type="secondary" style={{ fontSize: 12 }}>
              历史会话加载失败，不影响当前对话
            </Text>
          </div>
        ) : sessions.length === 0 ? (
          <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="暂无历史" style={{ marginTop: 12 }} />
        ) : (
          <Space direction="vertical" size={4} style={{ width: '100%', marginTop: 8 }}>
            {sessions.map((s) => {
              const active = s.id === activeSessionId;
              return (
                <div
                  key={s.id}
                  role="button"
                  tabIndex={0}
                  aria-label={`历史会话: ${s.title}`}
                  onClick={() => onSelectSession(s.id)}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter' || e.key === ' ') {
                      e.preventDefault();
                      onSelectSession(s.id);
                    }
                  }}
                  style={{
                    padding: '6px 10px',
                    borderRadius: 6,
                    cursor: 'pointer',
                    background: active ? token.colorFillSecondary : token.colorFillTertiary,
                    border: `1px solid ${active ? token.colorPrimaryBorder : 'transparent'}`,
                  }}
                >
                  <Text style={{ fontSize: 12 }} ellipsis>
                    {s.title}
                  </Text>
                  <div>
                    <Text type="secondary" style={{ fontSize: 11 }}>
                      {s.message_count} 条消息
                    </Text>
                  </div>
                </div>
              );
            })}
          </Space>
        )}
      </div>
    </Space>
  );
}

export default SimpleSidebar;
