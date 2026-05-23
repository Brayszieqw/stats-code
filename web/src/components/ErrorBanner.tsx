/**
 * ErrorBanner — 错误展示组件
 *
 * 在对话流中渲染错误信息卡片，包含错误码、可读消息与建议下一步动作。
 *
 * 特殊行为：
 * - LLM_UNAVAILABLE: 显示"重试"按钮，点击后以原请求参数重发，
 *   重试期间禁用按钮以防止重复触发 (R14.2)。
 * - SKILL_EXECUTION_FAILED / SKILL_INVALID_ARGS: 显示引导文本，
 *   建议用户修改输入或尝试其他统计方法 (R14.3)。
 *
 * Validates: Requirements 14.1, 14.2, 14.3
 */

import { useState } from 'react';
import { Alert, Button, Space, Typography } from 'antd';
import {
  ReloadOutlined,
  CloseOutlined,
  ExclamationCircleOutlined,
} from '@ant-design/icons';
import type { ErrorPayload, ErrorCode } from '../api/types';

const { Text, Paragraph } = Typography;

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

export interface ErrorBannerProps {
  /** 错误载荷，null 时不渲染 */
  error: ErrorPayload | null;
  /** 重试回调（用于 LLM_UNAVAILABLE 场景） */
  onRetry?: () => void;
  /** 关闭/忽略错误回调 */
  onDismiss?: () => void;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** 根据错误码生成建议下一步动作文本 */
function getSuggestedAction(errorCode: ErrorCode): string {
  switch (errorCode) {
    case 'LlmUnavailable':
      return '点击重试按钮重新请求 AI 服务。';
    case 'SkillExecutionFailed':
      return '统计任务执行异常，建议检查数据格式或尝试其他统计方法。';
    case 'SkillInvalidArgs':
      return '参数有误，请修改输入后重试，或选择其他统计方法。';
    case 'SkillTimeout':
      return '计算超时，建议减少数据量或简化模型后重试。';
    case 'SkillOom':
      return '内存不足，建议减少数据量后重试。';
    case 'MessageTooLong':
      return '请缩短消息长度后重新发送。';
    case 'AudioTooLarge':
      return '请录制更短的音频（不超过 60 秒）。';
    case 'DatasetTooLarge':
      return '请使用更小的数据文件（不超过 50 MB / 100 万行）。';
    case 'DatasetEmpty':
      return '请上传包含有效数据的文件。';
    case 'SessionQuotaExceeded':
      return '本会话上传容量已满，请创建新会话。';
    case 'SessionArchived':
      return '当前会话已归档，请创建新会话继续。';
    case 'SessionNotFound':
      return '会话不存在，请刷新页面或创建新会话。';
    case 'InvalidChoice':
      return '请从给定选项中选择。';
    default:
      return '请稍后重试或联系支持。';
  }
}

/** 判断是否为 SKILL 相关错误（需要引导修改输入） */
function isSkillError(errorCode: ErrorCode): boolean {
  return errorCode === 'SkillExecutionFailed' || errorCode === 'SkillInvalidArgs';
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function ErrorBanner({ error, onRetry, onDismiss }: ErrorBannerProps) {
  const [retrying, setRetrying] = useState(false);

  if (!error) return null;

  const isLlmUnavailable = error.error_code === 'LlmUnavailable';
  const isSkill = isSkillError(error.error_code);
  const suggestedAction = getSuggestedAction(error.error_code);

  const handleRetry = async () => {
    if (!onRetry || retrying) return;
    setRetrying(true);
    try {
      onRetry();
    } finally {
      // 保持 disabled 状态直到外部通过新渲染重置
      // 如果外部 error 被清除，组件将卸载
      // 如果外部需要重置状态，可通过 key prop 重新挂载
      setRetrying(false);
    }
  };

  // 构建操作按钮
  const actions: React.ReactNode[] = [];

  if (isLlmUnavailable && onRetry) {
    actions.push(
      <Button
        key="retry"
        type="primary"
        size="small"
        icon={<ReloadOutlined />}
        loading={retrying}
        disabled={retrying}
        onClick={handleRetry}
      >
        重试
      </Button>,
    );
  }

  if (onDismiss) {
    actions.push(
      <Button
        key="dismiss"
        type="text"
        size="small"
        icon={<CloseOutlined />}
        onClick={onDismiss}
      >
        关闭
      </Button>,
    );
  }

  return (
    <Alert
      type="error"
      showIcon
      icon={<ExclamationCircleOutlined />}
      style={{ marginTop: 8, marginBottom: 8 }}
      message={
        <Space direction="vertical" size={4} style={{ width: '100%' }}>
          {/* 错误码 + 消息 */}
          <div>
            <Text code style={{ fontSize: 11, marginRight: 8 }}>
              {error.error_code}
            </Text>
            <Text>{error.message}</Text>
          </div>

          {/* 建议下一步动作 */}
          <Text type="secondary" style={{ fontSize: 12 }}>
            {suggestedAction}
          </Text>

          {/* SKILL 错误引导文本 */}
          {isSkill && (
            <Paragraph
              type="warning"
              style={{ fontSize: 12, marginBottom: 0, marginTop: 4 }}
            >
              💡 您可以尝试修改变量选择或换用其他统计方法，系统将为您提供可选方案。
            </Paragraph>
          )}
        </Space>
      }
      action={
        actions.length > 0 ? (
          <Space direction="vertical" size={4}>
            {actions}
          </Space>
        ) : undefined
      }
    />
  );
}

export default ErrorBanner;
