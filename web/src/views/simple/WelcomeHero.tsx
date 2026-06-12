/**
 * WelcomeHero — 简易模式欢迎态。
 *
 * 布局对齐参考图 2（居中大标题 + 单个圆角输入框，输入框底部内置工具条），
 * 但视觉与文案为 Stats Code 原创（学术分析主题），不照搬 Codex 元素。
 * Enter 发送、Shift+Enter 换行；输入与发送控件带 aria-label。
 *
 * Validates: Requirements 2.3, 2.4, 2.5, 10.2
 */

import { useCallback, useState } from 'react';
import { Input, Button, Typography } from 'antd';
import {
  PaperClipOutlined,
  ArrowUpOutlined,
  AudioOutlined,
  DatabaseOutlined,
  DownOutlined,
} from '@ant-design/icons';
import type { DatasetSummary } from '../../api/types';

const { Title, Text } = Typography;
const { TextArea } = Input;

const PRIMARY = '#38618c';

export interface WelcomeHeroProps {
  onSend: (text: string) => void;
  disabled?: boolean;
  datasets?: DatasetSummary[];
  selectedDatasetId?: string | null;
  modelLabel?: string | null;
  onOpenDatasetPicker?: () => void;
  onOpenSettings?: () => void;
  onOpenVoiceInput?: () => void;
}

export function WelcomeHero({
  onSend,
  disabled = false,
  datasets = [],
  selectedDatasetId = null,
  modelLabel = 'DeepSeek',
  onOpenDatasetPicker,
  onOpenSettings,
  onOpenVoiceInput,
}: WelcomeHeroProps) {
  const [value, setValue] = useState('');

  const submit = useCallback(() => {
    const text = value.trim();
    if (!text || disabled) return;
    onSend(text);
    setValue('');
  }, [value, disabled, onSend]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
      if (e.key === 'Enter' && !e.shiftKey && !e.nativeEvent.isComposing) {
        e.preventDefault();
        submit();
      }
    },
    [submit],
  );

  const canSend = value.trim().length > 0 && !disabled;
  const selectedDataset = datasets.find((ds) => ds.dataset_id === selectedDatasetId) ?? null;
  const datasetLabel = selectedDataset
    ? selectedDataset.file_name
    : datasets.length > 0
      ? `${datasets.length} 个数据集`
      : '选择数据集';
  const resolvedModelLabel = modelLabel?.trim() || 'DeepSeek';

  return (
    <div style={{ width: '100%', maxWidth: 720, margin: '0 auto' }}>
      <div style={{ textAlign: 'center', marginBottom: 32 }}>
        <Title
          level={2}
          style={{ fontWeight: 600, color: '#2b3a4a', marginBottom: 8, fontSize: 30 }}
        >
          开始你的统计分析
        </Title>
        <Text type="secondary" style={{ fontSize: 14 }}>
          用自然语言描述研究问题，Stats 会引导你完成分析
        </Text>
      </div>

      <div
        style={{
          border: '1px solid #e3e1d8',
          borderRadius: 18,
          background: '#fff',
          boxShadow: '0 4px 20px rgba(56, 97, 140, 0.06)',
          padding: '14px 16px 10px',
        }}
      >
        <TextArea
          value={value}
          onChange={(e) => setValue(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder="例如：分析血压与年龄的关系，并给出回归模型"
          variant="borderless"
          autoSize={{ minRows: 1, maxRows: 8 }}
          disabled={disabled}
          style={{ fontSize: 15, padding: '2px 4px', resize: 'none' }}
          aria-label="消息输入框"
        />

        <div
          style={{
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
            marginTop: 8,
          }}
        >
          {/* 左侧工具 */}
          <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
            <Button
              type="text"
              shape="circle"
              icon={<PaperClipOutlined />}
              aria-label="附加数据集"
              onClick={onOpenDatasetPicker}
              disabled={disabled || !onOpenDatasetPicker}
              style={{ color: '#5a6e85' }}
            />
            <Button
              type="text"
              size="small"
              icon={<DatabaseOutlined />}
              aria-label="选择数据集"
              onClick={onOpenDatasetPicker}
              disabled={disabled || !onOpenDatasetPicker}
              style={{
                color: PRIMARY,
                borderRadius: 16,
                background: 'rgba(56,97,140,0.08)',
                paddingInline: 12,
              }}
            >
              {datasetLabel} <DownOutlined style={{ fontSize: 10 }} />
            </Button>
          </div>

          {/* 右侧工具 */}
          <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
            <Button
              type="text"
              size="small"
              aria-label="模型选择"
              onClick={onOpenSettings}
              disabled={!onOpenSettings}
              style={{ color: '#5a6e85' }}
            >
              {resolvedModelLabel} <DownOutlined style={{ fontSize: 10 }} />
            </Button>
            <Button
              type="text"
              shape="circle"
              icon={<AudioOutlined />}
              aria-label="语音输入"
              onClick={onOpenVoiceInput}
              disabled={disabled || !onOpenVoiceInput}
              style={{ color: '#5a6e85' }}
            />
            <Button
              type="text"
              shape="circle"
              onClick={submit}
              disabled={!canSend}
              aria-label="发送"
              icon={<ArrowUpOutlined />}
              style={{
                background: canSend ? PRIMARY : '#dfe3e8',
                color: '#fff',
                width: 34,
                height: 34,
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'center',
              }}
            />
          </div>
        </div>
      </div>
    </div>
  );
}

export default WelcomeHero;
