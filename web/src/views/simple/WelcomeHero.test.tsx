/**
 * Tests for WelcomeHero and SuggestionCards.
 *
 * Validates: Requirements 2.4, 2.5
 */

import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { WelcomeHero } from './WelcomeHero';
import { SuggestionCards } from './SuggestionCards';
import { SUGGESTION_PROMPTS } from './suggestions';

describe('WelcomeHero (Requirements 2.4, 2.5)', () => {
  it('sends on Enter (no shift)', () => {
    const onSend = vi.fn();
    render(<WelcomeHero onSend={onSend} />);
    const input = screen.getByLabelText('消息输入框');
    fireEvent.change(input, { target: { value: '你好' } });
    fireEvent.keyDown(input, { key: 'Enter', shiftKey: false });
    expect(onSend).toHaveBeenCalledWith('你好');
  });

  it('does not send on Shift+Enter', () => {
    const onSend = vi.fn();
    render(<WelcomeHero onSend={onSend} />);
    const input = screen.getByLabelText('消息输入框');
    fireEvent.change(input, { target: { value: '多行' } });
    fireEvent.keyDown(input, { key: 'Enter', shiftKey: true });
    expect(onSend).not.toHaveBeenCalled();
  });

  it('sends via the send button click', () => {
    const onSend = vi.fn();
    render(<WelcomeHero onSend={onSend} />);
    fireEvent.change(screen.getByLabelText('消息输入框'), { target: { value: 'go' } });
    fireEvent.click(screen.getByLabelText('发送'));
    expect(onSend).toHaveBeenCalledWith('go');
  });
});

describe('SuggestionCards (Requirement 2.4)', () => {
  it('clicking a card sends its preset prompt', () => {
    const onSend = vi.fn();
    render(<SuggestionCards onSend={onSend} />);
    const first = SUGGESTION_PROMPTS[0]!;
    fireEvent.click(screen.getByLabelText(`建议: ${first.title}`));
    expect(onSend).toHaveBeenCalledWith(first.prompt);
  });

  it('renders all preset cards', () => {
    render(<SuggestionCards onSend={() => {}} />);
    for (const s of SUGGESTION_PROMPTS) {
      expect(screen.getByLabelText(`建议: ${s.title}`)).toBeInTheDocument();
    }
  });
});
