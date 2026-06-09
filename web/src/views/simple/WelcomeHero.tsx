/**
 * WelcomeHero — centered welcome title + a large multi-line input box for the
 * simple-mode empty state. Enter sends, Shift+Enter inserts a newline. The
 * input and send button carry aria-labels.
 *
 * Validates: Requirements 2.3, 2.4, 2.5, 10.2
 */

import { useCallback, useState } from 'react';
import { Button, Input, Typography, theme as antdTheme } from 'antd';
import { SendOutlined } from '@ant-design/icons';

const { Title, Paragraph } = Typography;
const { TextArea } = Input;

export interface WelcomeHeroProps {
  onSend: (text: string) => void;
  disabled?: boolean;
}

export function WelcomeHero({ onSend, disabled = false }: WelcomeHeroProps) {
  const { token } = antdTheme.useToken();
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

  return (
    <div style={{ maxWidth: 720, width: '100%', margin: '0 auto', textAlign: 'center' }}>
      <Title level={2} style={{ marginBottom: 8 }}>
        欢迎使用 Stats 智能分析
      </Title>
      <Paragraph type="secondary" style={{ marginBottom: 24 }}>
        用自然语言描述你的研究问题，由 AI 引导你完成统计分析
      </Paragraph>
      <div
        style={{
          display: 'flex',
          alignItems: 'flex-end',
          gap: 8,
          padding: 12,
          background: token.colorBgContainer,
          borderRadius: token.borderRadiusLG,
          boxShadow: token.boxShadowTertiary,
          border: `1px solid ${token.colorBorderSecondary}`,
        }}
      >
        <TextArea
          value={value}
          onChange={(e) => setValue(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder="输入统计分析问题，Enter 发送，Shift+Enter 换行"
          autoSize={{ minRows: 3, maxRows: 8 }}
          disabled={disabled}
          style={{ flex: 1, resize: 'none', fontSize: 16 }}
          aria-label="消息输入框"
        />
        <Button
          type="primary"
          size="large"
          icon={<SendOutlined />}
          onClick={submit}
          disabled={!value.trim() || disabled}
          aria-label="发送"
        >
          发送
        </Button>
      </div>
    </div>
  );
}

export default WelcomeHero;
