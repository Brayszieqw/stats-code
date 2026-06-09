/**
 * Tests for mapSessionMessages.
 *
 * Property 4: order + count preserved (9.1, 9.6)
 * Unit: User Text/Audio/ChoiceAnswer + Agent block aggregation (9.1, 9.6)
 */

import { describe, it, expect } from 'vitest';
import fc from 'fast-check';
import { mapSessionMessages } from './sessionMessages';
import type { Message } from '../api/types';

let idCounter = 0;
const uid = () => `00000000-0000-4000-8000-${String(idCounter++).padStart(12, '0')}`;

function userText(text: string): Message {
  return { User: { id: uid(), created_at: '2026-01-01T00:00:00Z', content: { Text: text } } };
}
function agentText(text: string): Message {
  return { Agent: { id: uid(), created_at: '2026-01-01T00:00:00Z', blocks: [{ Text: text }] } };
}

describe('Property 4: message mapping preserves order and count (Requirements 9.1, 9.6)', () => {
  it('output length equals input length and roles align by index', () => {
    const msgArb = fc.boolean().map((isUser) => (isUser ? userText('u') : agentText('a')));
    fc.assert(
      fc.property(fc.array(msgArb, { maxLength: 30 }), (messages) => {
        const out = mapSessionMessages(messages);
        expect(out).toHaveLength(messages.length);
        messages.forEach((m, i) => {
          const expectedRole = 'User' in m ? 'user' : 'agent';
          expect(out[i]!.role).toBe(expectedRole);
        });
      }),
      { numRuns: 40 },
    );
  });
});

describe('mapSessionMessages — content derivation', () => {
  it('maps User Text content', () => {
    const out = mapSessionMessages([userText('hello')]);
    expect(out[0]).toMatchObject({ role: 'user', content: 'hello' });
  });

  it('maps User AudioTranscript to its text', () => {
    const msg: Message = {
      User: {
        id: uid(),
        created_at: '2026-01-01T00:00:00Z',
        content: { AudioTranscript: { text: '语音内容', confidence: 0.9 } },
      },
    };
    expect(mapSessionMessages([msg])[0]!.content).toBe('语音内容');
  });

  it('maps User ChoiceAnswer to a summary string', () => {
    const msg: Message = {
      User: {
        id: uid(),
        created_at: '2026-01-01T00:00:00Z',
        content: { ChoiceAnswer: { prompt_id: uid(), options: ['a', 'b'], custom_text: '其它' } },
      },
    };
    expect(mapSessionMessages([msg])[0]!.content).toBe('已选择: a, b | 其它');
  });

  it('aggregates Agent blocks: text joined, choicePrompt/skillResult/interpretation extracted', () => {
    const msg: Message = {
      Agent: {
        id: uid(),
        created_at: '2026-01-01T00:00:00Z',
        blocks: [
          { Text: '第一行' },
          { Text: '第二行' },
          { Interpretation: '解读' },
          { SkillResult: { run_id: uid(), result: { schema_version: '1.0', payload: {}, risk_signals: [] } } },
        ],
      },
    };
    const out = mapSessionMessages([msg])[0]!;
    expect(out.role).toBe('agent');
    expect(out.content).toBe('第一行\n第二行');
    expect(out.interpretation).toBe('解读');
    expect(out.skillResult).toBeDefined();
  });
});
