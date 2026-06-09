/**
 * SuggestionCards — preset prompt cards; clicking one immediately sends the
 * preset prompt via onSend.
 *
 * Validates: Requirements 2.4, 2.5
 */

import { Typography, theme as antdTheme } from 'antd';
import { SUGGESTION_PROMPTS } from './suggestions';

const { Text } = Typography;

export interface SuggestionCardsProps {
  onSend: (prompt: string) => void;
  disabled?: boolean;
}

export function SuggestionCards({ onSend, disabled = false }: SuggestionCardsProps) {
  const { token } = antdTheme.useToken();

  return (
    <div
      style={{
        display: 'grid',
        gridTemplateColumns: 'repeat(auto-fit, minmax(180px, 1fr))',
        gap: 12,
        maxWidth: 720,
        margin: '24px auto 0',
      }}
    >
      {SUGGESTION_PROMPTS.map((s) => (
        <button
          key={s.title}
          type="button"
          disabled={disabled}
          aria-label={`建议: ${s.title}`}
          onClick={() => onSend(s.prompt)}
          style={{
            padding: 14,
            background: token.colorBgContainer,
            border: `1px solid ${token.colorBorderSecondary}`,
            borderRadius: token.borderRadiusLG,
            textAlign: 'left',
            cursor: disabled ? 'not-allowed' : 'pointer',
          }}
        >
          <div style={{ fontSize: 22 }}>{s.icon}</div>
          <Text strong style={{ display: 'block', marginTop: 4 }}>
            {s.title}
          </Text>
          <Text type="secondary" style={{ fontSize: 12 }}>
            {s.prompt}
          </Text>
        </button>
      ))}
    </div>
  );
}

export default SuggestionCards;
