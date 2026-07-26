/**
 * ErrorBanner — 错误展示组件
 *
 * 在对话流中渲染错误信息卡片，包含错误码、可读消息与建议下一步动作。
 *
 * 特殊行为：
 * - LLM_UNAVAILABLE: 显示"重试"按钮
 * - ResearchProtocolRequired: 引导打开研究协议
 * - ResearchApprovalRequired / Stale / AuditBlocked: 引导检查器审批路径
 * - SKILL_*: 引导修改输入或换方法
 *
 * Validates: Requirements 14.1, 14.2, 14.3
 */

import { useEffect, useState } from 'react';
import { Alert, Button, Space, Typography } from 'antd';
import {
  ReloadOutlined,
  CloseOutlined,
  ExclamationCircleOutlined,
  FileProtectOutlined,
  ExperimentOutlined,
} from '@ant-design/icons';
import type { ErrorPayload, ErrorCode } from '../api/types';

const { Text, Paragraph } = Typography;

export interface ErrorBannerProps {
  error: ErrorPayload | null;
  onRetry?: () => void;
  onDismiss?: () => void;
  /** Open research protocol drawer / form */
  onOpenProtocol?: () => void;
  /** Open analysis inspector / switch to Pro for plan approval */
  onOpenInspector?: () => void;
}

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
    case 'ResearchProtocolRequired':
      return '请先填写并审批研究协议（研究问题、设计、结局等），再继续分析或描述性之外的统计任务。';
    case 'ResearchApprovalRequired':
      return '协议审批后，还需在「检查器 → 分析设置」配置方案，点击「批准方案并运行」。聊天不会自动绕过该门禁。';
    case 'ResearchApprovalStale':
      return '已批准方案与当前协议/数据/参数不一致，请在检查器中重新审计并批准。';
    case 'ResearchAuditBlocked':
      return '数据审计发现阻断项，请根据审计结果修正数据或变量角色后再试。';
    case 'ResearchVersionConflict':
      return '协议版本冲突，请重新打开协议并基于最新版本保存/审批。';
    default:
      return '请按提示处理，或切换到专业模式使用可视化配置。';
  }
}

function isSkillError(errorCode: ErrorCode): boolean {
  return errorCode === 'SkillExecutionFailed' || errorCode === 'SkillInvalidArgs';
}

function needsProtocolAction(errorCode: ErrorCode): boolean {
  return errorCode === 'ResearchProtocolRequired' || errorCode === 'ResearchVersionConflict';
}

function needsInspectorAction(errorCode: ErrorCode): boolean {
  return (
    errorCode === 'ResearchApprovalRequired'
    || errorCode === 'ResearchApprovalStale'
    || errorCode === 'ResearchAuditBlocked'
  );
}

export function ErrorBanner({
  error,
  onRetry,
  onDismiss,
  onOpenProtocol,
  onOpenInspector,
}: ErrorBannerProps) {
  const [retrying, setRetrying] = useState(false);

  // Reset retry lock when the error payload changes or clears.
  useEffect(() => {
    setRetrying(false);
  }, [error?.error_code, error?.message]);

  if (!error) return null;

  const isLlmUnavailable = error.error_code === 'LlmUnavailable';
  const isSkill = isSkillError(error.error_code);
  const suggestedAction = getSuggestedAction(error.error_code);

  const handleRetry = () => {
    if (!onRetry || retrying) return;
    setRetrying(true);
    onRetry();
    // Keep disabled until parent clears/replaces `error` (see effect above).
  };

  const actions: React.ReactNode[] = [];

  if (needsProtocolAction(error.error_code) && onOpenProtocol) {
    actions.push(
      <Button
        key="protocol"
        type="primary"
        size="small"
        icon={<FileProtectOutlined />}
        onClick={onOpenProtocol}
      >
        去填写研究协议
      </Button>,
    );
  }

  if (needsInspectorAction(error.error_code) && onOpenInspector) {
    actions.push(
      <Button
        key="inspector"
        type="primary"
        size="small"
        icon={<ExperimentOutlined />}
        onClick={onOpenInspector}
      >
        去完成审批
      </Button>,
    );
  }

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
          <div>
            <Text code style={{ fontSize: 11, marginRight: 8 }}>
              {error.error_code}
            </Text>
            <Text>{error.message}</Text>
          </div>

          <Text type="secondary" style={{ fontSize: 12 }}>
            {suggestedAction}
          </Text>

          {isSkill && (
            <Paragraph
              type="warning"
              style={{ fontSize: 12, marginBottom: 0, marginTop: 4 }}
            >
              您可以尝试修改变量选择或换用其他统计方法，系统将为您提供可选方案。
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
