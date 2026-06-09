/**
 * SimpleSidebar — 简易模式左侧导航。
 *
 * 布局对齐参考图 2（顶部入口列表 + 分组历史 + 底部信息卡），视觉与文案为
 * Stats Code 原创，不照搬 Codex 元素。搜索/插件/自动化/模板为占位
 * message.info；历史项点击调用 onSelectSession。历史加载错误显示占位，不阻塞。
 *
 * Validates: Requirements 2.1, 2.2, 9.6, 9.7, 11.1
 */

import { Typography, Empty, message } from 'antd';
import {
  EditOutlined,
  SearchOutlined,
  ApiOutlined,
  ThunderboltOutlined,
  AppstoreOutlined,
  FolderOpenOutlined,
  HistoryOutlined,
} from '@ant-design/icons';
import type { UseSessionListReturn } from '../../hooks/useSessionList';

const { Text } = Typography;

const PRIMARY = '#38618c';

export interface SimpleSidebarProps {
  sessionList: UseSessionListReturn;
  onNewSession: () => void;
  onSelectSession: (sessionId: string) => void;
  activeSessionId?: string;
}

const comingSoon = () => message.info('即将推出');

const NAV_ITEMS = [
  { key: 'new', label: '新对话', icon: <EditOutlined /> },
  { key: 'search', label: '搜索', icon: <SearchOutlined /> },
  { key: 'plugins', label: '插件', icon: <ApiOutlined /> },
  { key: 'automation', label: '自动化', icon: <ThunderboltOutlined /> },
  { key: 'templates', label: '分析模板', icon: <AppstoreOutlined /> },
];

function NavRow({
  icon,
  label,
  onClick,
  accent = false,
}: {
  icon: React.ReactNode;
  label: string;
  onClick: () => void;
  accent?: boolean;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      onClick={onClick}
      style={{
        display: 'flex',
        alignItems: 'center',
        gap: 10,
        width: '100%',
        padding: '8px 10px',
        border: 'none',
        background: 'transparent',
        borderRadius: 8,
        cursor: 'pointer',
        color: accent ? PRIMARY : '#3a4654',
        fontSize: 14,
        fontWeight: accent ? 600 : 400,
        textAlign: 'left',
      }}
      onMouseEnter={(e) => (e.currentTarget.style.background = 'rgba(56,97,140,0.07)')}
      onMouseLeave={(e) => (e.currentTarget.style.background = 'transparent')}
    >
      <span style={{ fontSize: 15, color: accent ? PRIMARY : '#6a7a8c' }}>{icon}</span>
      {label}
    </button>
  );
}

export function SimpleSidebar({
  sessionList,
  onNewSession,
  onSelectSession,
  activeSessionId,
}: SimpleSidebarProps) {
  const { sessions, error } = sessionList;

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%' }}>
      {/* 品牌 */}
      <div style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '14px 14px 8px' }}>
        <ThunderboltOutlined style={{ color: PRIMARY, fontSize: 18 }} />
        <Text strong style={{ fontSize: 15, color: '#2b3a4a' }}>
          Stats 智能分析
        </Text>
      </div>

      {/* 顶部入口 */}
      <div style={{ padding: '4px 8px' }}>
        {NAV_ITEMS.map((it) => (
          <NavRow
            key={it.key}
            icon={it.icon}
            label={it.label}
            accent={it.key === 'new'}
            onClick={it.key === 'new' ? onNewSession : comingSoon}
          />
        ))}
      </div>

      {/* 历史区 */}
      <div style={{ flex: 1, overflowY: 'auto', padding: '8px' }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 6, padding: '6px 10px', color: '#9aa7b4', fontSize: 12 }}>
          <FolderOpenOutlined /> 默认项目
        </div>
        <div style={{ display: 'flex', alignItems: 'center', gap: 6, padding: '6px 10px', color: '#9aa7b4', fontSize: 12 }}>
          <HistoryOutlined /> 历史会话
        </div>

        {error ? (
          <div style={{ padding: '8px 10px' }} role="note">
            <Text type="secondary" style={{ fontSize: 12 }}>
              历史会话加载失败，不影响当前对话
            </Text>
          </div>
        ) : sessions.length === 0 ? (
          <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="暂无历史" style={{ marginTop: 24 }} />
        ) : (
          sessions.map((s) => {
            const active = s.id === activeSessionId;
            return (
              <button
                key={s.id}
                type="button"
                aria-label={`历史会话: ${s.title}`}
                onClick={() => onSelectSession(s.id)}
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  justifyContent: 'space-between',
                  width: '100%',
                  padding: '8px 10px',
                  border: 'none',
                  background: active ? 'rgba(56,97,140,0.12)' : 'transparent',
                  borderRadius: 8,
                  cursor: 'pointer',
                  color: '#3a4654',
                  fontSize: 13,
                  textAlign: 'left',
                }}
                onMouseEnter={(e) => {
                  if (!active) e.currentTarget.style.background = 'rgba(56,97,140,0.07)';
                }}
                onMouseLeave={(e) => {
                  if (!active) e.currentTarget.style.background = 'transparent';
                }}
              >
                <span style={{ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', maxWidth: 150 }}>
                  {s.title}
                </span>
                <Text type="secondary" style={{ fontSize: 11 }}>
                  {s.message_count}
                </Text>
              </button>
            );
          })
        )}
      </div>
    </div>
  );
}

export default SimpleSidebar;
