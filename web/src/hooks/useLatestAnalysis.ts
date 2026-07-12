/**
 * useLatestAnalysis — derive the most recent analysis result + agent message
 * from a chat message list (Pro mode), extracted from the former WorkflowPage
 * useMemo logic.
 *
 * Validates: Requirements 6.1, 7.1
 */

import { useMemo } from 'react';
import type { SkillResult } from '../api/types';
import type { ChatMessage } from './useSseChat';

export interface LatestAnalysis {
  /** The most recent agent message carrying a skillResult, else null. */
  result: SkillResult | null;
  /** The message that owns `result`; keeps conclusion and interpretation aligned. */
  resultMessage: ChatMessage | null;
  /** The most recent agent message of any kind, else null. */
  agentMessage: ChatMessage | null;
}

export function useLatestAnalysis(messages: ChatMessage[]): LatestAnalysis {
  return useMemo(() => {
    let result: SkillResult | null = null;
    let resultMessage: ChatMessage | null = null;
    let agentMessage: ChatMessage | null = null;

    for (let i = messages.length - 1; i >= 0; i--) {
      const msg = messages[i];
      if (!msg || msg.role !== 'agent') continue;
      if (agentMessage === null) {
        agentMessage = msg;
      }
      if (result === null && msg.skillResult) {
        result = msg.skillResult;
        resultMessage = msg;
      }
      if (agentMessage !== null && result !== null) break;
    }

    return { result, resultMessage, agentMessage };
  }, [messages]);
}
