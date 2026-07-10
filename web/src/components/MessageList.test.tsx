import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { MessageList } from './MessageList';
import type { ChatMessage } from '../hooks/useSseChat';

vi.mock('./StatsChartRenderer', () => ({
  StatsChartRenderer: () => null,
}));

const baseMessage = {
  timestamp: new Date('2026-01-01T00:00:00Z'),
};

function userMessage(content: string): ChatMessage {
  return {
    ...baseMessage,
    id: `user-${content}`,
    role: 'user',
    content,
  };
}

function emptyAgentMessage(): ChatMessage {
  return {
    ...baseMessage,
    id: 'agent-empty',
    role: 'agent',
    content: '',
  };
}

describe('MessageList', () => {
  it('does not render an empty agent placeholder bubble', () => {
    const { container } = render(<MessageList messages={[userMessage('hello'), emptyAgentMessage()]} />);

    expect(screen.getByText('hello')).toBeInTheDocument();
    expect(container.querySelector('.anticon-robot')).not.toBeInTheDocument();
  });
});
