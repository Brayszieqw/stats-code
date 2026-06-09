/**
 * TopBar — 专业模式顶栏（对齐 MiroFish 参考图 1，macOS 窗口风格）。
 *
 * 左：红/黄/绿交通灯 + 前进后退箭头。中：圆角标题胶囊 + ＋。右：模式切换 +
 * 设置/分屏/头像等图标。复用 AntD token。
 *
 * Validates: Requirements 4.2, 4.4, 10.1
 */

import { Button, Space } from 'antd';
import {
  LeftOutlined,
  RightOutlined,
  PlusOutlined,
  SettingOutlined,
  ClockCircleOutlined,
  UserOutlined,
} from '@ant-design/icons';
import { ModeToggle } from '../../components/ModeToggle';
import type { ViewMode } from '../../hooks/useModePreference';

export interface TopBarProps {
  title?: string;
  model?: string | null;
  mode: ViewMode;
  onModeChange: (m: ViewMode) => void;
  onOpenSettings?: () => void;
}

function TrafficLight({ color }: { color: string }) {
  return <span style={{ width: 12, height: 12, borderRadius: '50%', background: color, display: 'inline-block' }} />;
}

export function TopBar({ title = 'MediStat Pro | Patient Data Analysis', mode, onModeChange, onOpenSettings }: TopBarProps) {
  return (
    <div
      style={{
        display: 'flex',
        alignItems: 'center',
        height: 44,
        padding: '0 14px',
        background: '#ececec',
        borderBottom: '1px solid #dcdcdc',
        gap: 12,
      }}
    >
      {/* 交通灯 */}
      <Space size={8}>
        <TrafficLight color="#ff5f57" />
        <TrafficLight color="#febc2e" />
        <TrafficLight color="#28c840" />
      </Space>

      {/* 前进后退 */}
      <Space size={2} style={{ marginLeft: 6, color: '#888' }}>
        <Button type="text" size="small" icon={<LeftOutlined />} aria-label="后退" style={{ color: '#888' }} />
        <Button type="text" size="small" icon={<RightOutlined />} aria-label="前进" style={{ color: '#888' }} />
      </Space>

      {/* 中部标题胶囊 */}
      <div style={{ flex: 1, display: 'flex', justifyContent: 'center' }}>
        <div
          style={{
            display: 'flex',
            alignItems: 'center',
            gap: 10,
            background: '#fff',
            border: '1px solid #e0e0e0',
            borderRadius: 8,
            padding: '4px 16px',
            minWidth: 360,
            justifyContent: 'center',
            color: '#3a3a3a',
            fontSize: 13,
          }}
        >
          {title}
          <PlusOutlined style={{ fontSize: 11, color: '#999' }} />
        </div>
      </div>

      {/* 右侧图标 */}
      <Space size={10} align="center">
        <ModeToggle mode={mode} onChange={onModeChange} />
        <Button type="text" size="small" icon={<SettingOutlined />} onClick={onOpenSettings} aria-label="设置" style={{ color: '#888' }} />
        <Button type="text" size="small" icon={<ClockCircleOutlined />} aria-label="历史" style={{ color: '#888' }} />
        <span
          style={{
            width: 24,
            height: 24,
            borderRadius: '50%',
            background: '#cdd6e0',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            color: '#5a6e85',
          }}
        >
          <UserOutlined style={{ fontSize: 13 }} />
        </span>
      </Space>
    </div>
  );
}

export default TopBar;
