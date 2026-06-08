// tests/property/sse-frame-shape.property.test.ts — Property 2.
//
// For every AgentEvent variant, serializeSseFrame yields a frame whose `event:`
// name matches the variant and whose `data:` line is always parseable JSON
// with the documented (Rust-mirrored) shape.
//
// Validates: Requirements 9.1, 9.4, 13.4

import { describe, it, expect } from 'vitest';
import fc from 'fast-check';
import { serializeSseFrame, type AgentEvent } from '@stats-code/server';

/** Parse an SSE frame into { name, data } asserting the canonical structure. */
function parseFrame(frame: string): { name: string; data: unknown } {
  // Frame shape: "event: <name>\ndata: <json>\n\n"
  expect(frame.endsWith('\n\n')).toBe(true);
  const lines = frame.slice(0, -2).split('\n');
  expect(lines).toHaveLength(2);
  expect(lines[0]!.startsWith('event: ')).toBe(true);
  expect(lines[1]!.startsWith('data: ')).toBe(true);
  const name = lines[0]!.slice('event: '.length);
  const data = JSON.parse(lines[1]!.slice('data: '.length));
  return { name, data };
}

const jsonObj = () => fc.dictionary(fc.string(), fc.jsonValue());

const agentEventArb: fc.Arbitrary<AgentEvent> = fc.oneof(
  fc.record({ type: fc.constant('text_delta' as const), text: fc.string() }),
  fc.record({ type: fc.constant('interpretation' as const), text: fc.string() }),
  fc.record({ type: fc.constant('skill_call' as const), skill_id: fc.string(), args: jsonObj() }),
  fc.record({ type: fc.constant('skill_result' as const), result: jsonObj() }),
  fc.record({ type: fc.constant('choice_prompt' as const), prompt: jsonObj() }),
  fc.record({ type: fc.constant('error' as const), payload: jsonObj() }),
  fc.record({ type: fc.constant('done' as const) }),
);

describe('Property 2: SSE frame shape stability (Requirements 9.1, 9.4, 13.4)', () => {
  it('every AgentEvent serializes to a stable event name + parseable JSON data', () => {
    fc.assert(
      fc.property(agentEventArb, (event) => {
        const frame = serializeSseFrame(event);
        const { name, data } = parseFrame(frame);
        expect(name).toBe(event.type);
        // The data line must equal the canonical JSON projection of the
        // payload (round-tripping through JSON is the contract; -0/key-order
        // normalization is expected and not a serializer concern).
        const canonical = (v: unknown) => JSON.parse(JSON.stringify(v));
        switch (event.type) {
          case 'text_delta':
          case 'interpretation':
            expect(data).toEqual({ text: event.text });
            break;
          case 'skill_call':
            expect(data).toEqual(canonical({ skill_id: event.skill_id, args: event.args }));
            break;
          case 'skill_result':
            expect(data).toEqual(canonical(event.result));
            break;
          case 'choice_prompt':
            expect(data).toEqual(canonical(event.prompt));
            break;
          case 'error':
            expect(data).toEqual(canonical(event.payload));
            break;
          case 'done':
            expect(data).toEqual({});
            break;
        }
      }),
      { numRuns: 200 },
    );
  });
});
