/**
 * TopBar — 专业模式顶栏：项目/会话标题 + ModeToggle + 设置入口。
 * 复用 AntD token 与 index.css 主题变量。
 *
 * Validates: Requirements 4.2, 4.4, 10.1
 */

import { Button, Space, Typography, Tag, theme as antdTheme } from 'antd';
import { ThunderboltOutlined, SettingOutlined } from '@ant-design/icons';
import { ModeToggle } from '../../components/ModeToggle';
import type { ViewMode } from '../../hooks/useModePreference';

const { Text } = Typography;

export interface TopBarProps {
  title?: string;
  model?: string | null;
  mode: ViewMode;
  onModeChange: (m: ViewMode) => void;
  onOpenSettings?: () => void;
}

export function TopBar({ title = 'Stats 智能科研分析', model, mode, onModeChange, onOpenSettings }: TopBarProps) {
  const { token } = antdTheme.useToken();

  return (
    <div
      style={{
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'space-between',
        height: 56,
        padding: '0 20px',
        background: token.colorBgContainer,
        borderBottom: `1px solid ${token.colorBorderSecondary}`,
      }}
    >
      <Space size={8} align="center">
        <ThunderboltOutlined style={{ color: token.colorPrimary, fontSize: 18 }} />
        <Text strong style={{ fontSize: 15 }}>
          {title}
        </Text>
      </Space>

      <Space size={16} align="center">
        {model ? <Tag color="blue">{model}</Tag> : null}
        <ModeToggle mode={mode} onChange={onModeChange} />
        <Button
          type="text"
          icon={<SettingOutlined />}
          onClick={onOpenSettings}
          title="设置"
          aria-label="设置"
        />
      </Space>
    </div>
  );
}

export default TopBar;
