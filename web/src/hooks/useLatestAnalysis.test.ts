/**
 * Tests for useLatestAnalysis.
 *
 * Validates: Requirements 6.1, 7.1
 */

import { describe, it, expect } from 'vitest';
import { renderHook } from '@testing-library/react';
import { useLatestAnalysis } from './useLatestAnalysis';
import type { ChatMessage } from './useSseChat';
import type { SkillResult } from '../api/types';

function agent(id: string, skillResult?: SkillResult): ChatMessage {
  return { id, role: 'agent', content: '', skillResult, timestamp: new Date() };
}
function user(id: string): ChatMessage {
  return { id, role: 'user', content: 'q', timestamp: new Date() };
}
const result = (tag: string): SkillResult => ({ schema_version: '1.0', payload: { tag }, risk_signals: [] });

describe('useLatestAnalysis (Requirements 6.1, 7.1)', () => {
  it('returns null result/agentMessage when there are no agent messages', () => {
    const { result: r } = renderHook(() => useLatestAnalysis([user('u1')]));
    expect(r.current.result).toBeNull();
    expect(r.current.agentMessage).toBeNull();
  });

  it('returns null result but a latest agentMessage when no skillResult present', () => {
    const msgs = [user('u1'), agent('a1')];
    const { result: r } = renderHook(() => useLatestAnalysis(msgs));
    expect(r.current.result).toBeNull();
    expect(r.current.agentMessage?.id).toBe('a1');
  });

  it('returns the most recent skillResult among multiple agent messages', () => {
    const msgs = [agent('a1', result('first')), user('u1'), agent('a2', result('second')), agent('a3')];
    const { result: r } = renderHook(() => useLatestAnalysis(msgs));
    expect((r.current.result?.payload as { tag: string }).tag).toBe('second');
    expect(r.current.agentMessage?.id).toBe('a3');
  });

  it('ignores user messages when deriving the result', () => {
    const msgs = [user('u1'), agent('a1', result('only'))];
    const { result: r } = renderHook(() => useLatestAnalysis(msgs));
    expect((r.current.result?.payload as { tag: string }).tag).toBe('only');
  });
});
