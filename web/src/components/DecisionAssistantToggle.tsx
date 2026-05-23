/**
 * DecisionAssistantToggle — 辅助决策模式开关
 *
 * 允许用户启用/禁用辅助决策模式。开启后，Agent 会在每轮回复中主动建议
 * 下一步、推荐统计方法并解读结果。
 *
 * 切换时通过 patchSettings API 同步状态到后端 Session。
 *
 * Validates: Requirements 5.1, 5.4
 */

import { useState } from 'react';
import { Switch, Space, Typography } from 'antd';
import { patchSettings } from '../api/client';

const { Text } = Typography;

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

export interface DecisionAssistantToggleProps {
  /** 当前开关状态 */
  value: boolean;
  /** 状态变更回调 */
  onChange: (enabled: boolean) => void;
  /** 当前会话 ID，用于同步设置到后端 */
  sessionId?: string;
  /** 是否禁用（如会话未创建时） */
  disabled?: boolean;
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function DecisionAssistantToggle({
  value,
  onChange,
  sessionId,
  disabled = false,
}: DecisionAssistantToggleProps) {
  const [loading, setLoading] = useState(false);

  const handleChange = async (checked: boolean) => {
    if (!sessionId) {
      // 无 session 时仅更新本地状态
      onChange(checked);
      return;
    }

    setLoading(true);
    try {
      await patchSettings(sessionId, { decision_assistant: checked });
      onChange(checked);
    } catch {
      // 同步失败时不更新状态，保持上一次的值
    } finally {
      setLoading(false);
    }
  };

  return (
    <Space size={8} align="center">
      <Switch
        checked={value}
        onChange={handleChange}
        loading={loading}
        disabled={disabled}
        size="small"
      />
      <Text type={value ? undefined : 'secondary'} style={{ fontSize: 13 }}>
        辅助决策
      </Text>
    </Space>
  );
}

export default DecisionAssistantToggle;
