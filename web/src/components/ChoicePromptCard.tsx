/**
 * ChoicePromptCard — 渲染结构化选择题
 *
 * 单选模式：每个选项以 Button 呈现，点击即提交。
 * 多选模式：Checkbox.Group + 提交按钮。
 * 支持自定义文本输入（allow_custom_text = true）。
 * 推荐选项以 Badge 标识。
 *
 * Validates: Requirements 4.2, 4.3, 4.6, 5.5
 */

import { useState } from 'react';
import {
  Card,
  Button,
  Checkbox,
  Input,
  Tag,
  Space,
  Typography,
  Tooltip,
} from 'antd';
import { CheckOutlined, StarFilled } from '@ant-design/icons';
import type { ChoicePrompt, ChoiceAnswer, ChoiceOption } from '../api/types';

const { Text, Paragraph } = Typography;

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

export interface ChoicePromptCardProps {
  prompt: ChoicePrompt;
  onSubmit: (answer: ChoiceAnswer) => void;
  disabled?: boolean;
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function ChoicePromptCard({ prompt, onSubmit, disabled = false }: ChoicePromptCardProps) {
  const [selectedOptions, setSelectedOptions] = useState<string[]>([]);
  const [customText, setCustomText] = useState('');
  const [submitted, setSubmitted] = useState(false);

  const isDisabled = disabled || submitted;

  // 单选：点击即提交
  const handleSingleSelect = (optionId: string) => {
    if (isDisabled) return;
    setSelectedOptions([optionId]);
    setSubmitted(true);
    onSubmit({
      prompt_id: prompt.prompt_id,
      options: [optionId],
      custom_text: null,
    });
  };

  // 多选：更新选中列表
  const handleMultiSelectChange = (checkedValues: string[]) => {
    if (isDisabled) return;
    setSelectedOptions(checkedValues);
  };

  // 多选提交
  const handleMultiSubmit = () => {
    if (isDisabled) return;
    const text = customText.trim() || null;
    // 至少选一项或提供自定义文本
    if (selectedOptions.length === 0 && !text) return;
    setSubmitted(true);
    onSubmit({
      prompt_id: prompt.prompt_id,
      options: selectedOptions,
      custom_text: text,
    });
  };

  // 仅自定义文本提交（单选模式 + allow_custom_text 时）
  const handleCustomTextSubmit = () => {
    if (isDisabled) return;
    const text = customText.trim();
    if (!text) return;
    setSubmitted(true);
    onSubmit({
      prompt_id: prompt.prompt_id,
      options: [],
      custom_text: text,
    });
  };

  // 多选模式：提交按钮是否可用
  const canSubmitMulti = selectedOptions.length > 0 || customText.trim().length > 0;

  return (
    <Card
      size="small"
      style={{
        marginTop: 8,
        borderColor: '#91caff',
        background: '#f0f5ff',
      }}
    >
      {/* 问题文本 */}
      <Paragraph strong style={{ marginBottom: 12 }}>
        {prompt.question}
      </Paragraph>

      {/* 选项区域 */}
      {prompt.multi_select ? (
        <MultiSelectOptions
          options={prompt.options}
          recommendation={prompt.recommendation}
          selectedOptions={selectedOptions}
          onChange={handleMultiSelectChange}
          disabled={isDisabled}
        />
      ) : (
        <SingleSelectOptions
          options={prompt.options}
          recommendation={prompt.recommendation}
          onSelect={handleSingleSelect}
          disabled={isDisabled}
        />
      )}

      {/* 自定义文本输入 */}
      {prompt.allow_custom_text && (
        <div style={{ marginTop: 12 }}>
          <Input
            value={customText}
            onChange={(e) => setCustomText(e.target.value)}
            placeholder="自定义回答"
            disabled={isDisabled}
            onPressEnter={prompt.multi_select ? handleMultiSubmit : handleCustomTextSubmit}
            suffix={
              !prompt.multi_select ? (
                <Button
                  type="text"
                  size="small"
                  icon={<CheckOutlined />}
                  disabled={isDisabled || !customText.trim()}
                  onClick={handleCustomTextSubmit}
                />
              ) : undefined
            }
          />
        </div>
      )}

      {/* 多选模式：提交按钮 */}
      {prompt.multi_select && (
        <div style={{ marginTop: 12, textAlign: 'right' }}>
          <Button
            type="primary"
            size="small"
            onClick={handleMultiSubmit}
            disabled={isDisabled || !canSubmitMulti}
            icon={<CheckOutlined />}
          >
            提交
          </Button>
        </div>
      )}

      {/* 已提交状态提示 */}
      {submitted && (
        <Text type="secondary" style={{ display: 'block', marginTop: 8, fontSize: 12 }}>
          ✓ 已提交
        </Text>
      )}
    </Card>
  );
}

// ---------------------------------------------------------------------------
// Sub-components
// ---------------------------------------------------------------------------

/** 单选模式：按钮列表 */
function SingleSelectOptions({
  options,
  recommendation,
  onSelect,
  disabled,
}: {
  options: ChoiceOption[];
  recommendation: string | null;
  onSelect: (optionId: string) => void;
  disabled: boolean;
}) {
  return (
    <Space direction="vertical" size={8} style={{ width: '100%' }}>
      {options.map((option) => {
        const isRecommended = recommendation === option.option_id;
        const button = (
          <Button
            key={option.option_id}
            block
            disabled={disabled}
            onClick={() => onSelect(option.option_id)}
            style={{
              textAlign: 'left',
              height: 'auto',
              whiteSpace: 'normal',
              padding: '8px 12px',
            }}
          >
            <Space size={8} align="start">
              <span>{option.text}</span>
              {isRecommended && (
                <Tag color="gold" icon={<StarFilled />} style={{ marginLeft: 4 }}>
                  推荐
                </Tag>
              )}
            </Space>
          </Button>
        );

        if (option.explanation) {
          return (
            <Tooltip key={option.option_id} title={option.explanation} placement="right">
              {button}
            </Tooltip>
          );
        }

        return button;
      })}
    </Space>
  );
}

/** 多选模式：Checkbox 列表 */
function MultiSelectOptions({
  options,
  recommendation,
  selectedOptions,
  onChange,
  disabled,
}: {
  options: ChoiceOption[];
  recommendation: string | null;
  selectedOptions: string[];
  onChange: (values: string[]) => void;
  disabled: boolean;
}) {
  return (
    <Checkbox.Group
      value={selectedOptions}
      onChange={(values) => onChange(values as string[])}
      disabled={disabled}
      style={{ width: '100%' }}
    >
      <Space direction="vertical" size={8} style={{ width: '100%' }}>
        {options.map((option) => {
          const isRecommended = recommendation === option.option_id;

          const checkbox = (
            <Checkbox key={option.option_id} value={option.option_id}>
              <Space size={8}>
                <span>{option.text}</span>
                {isRecommended && (
                  <Tag color="gold" icon={<StarFilled />}>
                    推荐
                  </Tag>
                )}
              </Space>
            </Checkbox>
          );

          if (option.explanation) {
            return (
              <Tooltip key={option.option_id} title={option.explanation} placement="right">
                {checkbox}
              </Tooltip>
            );
          }

          return checkbox;
        })}
      </Space>
    </Checkbox.Group>
  );
}

export default ChoicePromptCard;
