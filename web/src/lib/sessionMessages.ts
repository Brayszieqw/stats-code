/**
 * mapSessionMessages — pure conversion of backend Session.messages
 * (User/Agent union) into the frontend ChatMessage[] shape.
 *
 * Extracted verbatim from the former ChatPage inline mapping so it can be
 * reused by the session controller and unit-tested in isolation. Order and
 * count are preserved 1:1 with the input (Property 4).
 *
 * Validates: Requirements 9.1, 9.6
 */

import type { Message, ChoicePrompt, SkillResult } from '../api/types';
import type { ChatMessage } from '../hooks/useSseChat';

export function mapSessionMessages(messages: Message[]): ChatMessage[] {
  return messages.map((msg): ChatMessage => {
    if ('User' in msg) {
      const userMsg = msg.User;
      let content = '';
      if ('Text' in userMsg.content) {
        content = userMsg.content.Text;
      } else if ('AudioTranscript' in userMsg.content) {
        content = userMsg.content.AudioTranscript.text;
      } else if ('ChoiceAnswer' in userMsg.content) {
        const answer = userMsg.content.ChoiceAnswer;
        const parts: string[] = [];
        if (answer.options.length > 0) {
          parts.push(`已选择: ${answer.options.join(', ')}`);
        }
        if (answer.custom_text) {
          parts.push(answer.custom_text);
        }
        content = parts.join(' | ') || '继续';
      }
      return {
        id: userMsg.id,
        role: 'user',
        content,
        timestamp: new Date(userMsg.created_at),
      };
    }

    const agentMsg = msg.Agent;
    let content = '';
    let choicePrompt: ChoicePrompt | undefined;
    let skillResult: SkillResult | undefined;
    let interpretation: string | undefined;

    for (const block of agentMsg.blocks) {
      if ('Text' in block) {
        content += (content ? '\n' : '') + block.Text;
      } else if ('ChoicePrompt' in block) {
        choicePrompt = block.ChoicePrompt;
      } else if ('SkillResult' in block) {
        skillResult = block.SkillResult.result;
      } else if ('Interpretation' in block) {
        interpretation = block.Interpretation;
      }
    }

    return {
      id: agentMsg.id,
      role: 'agent',
      content,
      choicePrompt,
      skillResult,
      interpretation,
      timestamp: new Date(agentMsg.created_at),
    };
  });
}
