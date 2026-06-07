// tests/integration/sse-contract-replay.test.ts — SSE contract replay (task 3.9).
//
// Replays a recorded Rust SSE stream fixture and asserts frame-for-frame
// equivalence against the TS SSE emitter, normalizing non-deterministic fields
// (UUID prompt ids). The fixture encodes the exact wire frames produced by
// crates/agent-server/src/handlers/message.rs::agent_event_to_sse.
//
// _Requirements: 1.5_

import { describe, it, expect } from 'vitest';
import {
  buildRouter,
  MemSessionStore,
  serializeSseFrame,
  type AgentEvent,
  type AppState,
  type MessageHandler,
} from '@stats-code/server';

function makeState(overrides: Partial<AppState> = {}): AppState {
  return { sessionStore: new MemSessionStore(), ...overrides };
}

// Recorded Rust SSE stream (frame-for-frame). Each frame is
// `event: <name>\ndata: <json>\n\n`, matching axum's Sse encoder for the
// AgentEvent → Event mapping in the Rust message handler.
const RUST_SSE_FIXTURE =
  'event: text_delta\ndata: {"text":"你好"}\n\n' +
  'event: skill_call\ndata: {"skill_id":"ttest","args":{"y":"age","group":"arm"}}\n\n' +
  'event: skill_result\ndata: {"schema_version":"1","value":1.23}\n\n' +
  'event: interpretation\ndata: {"text":"p < 0.05"}\n\n' +
  'event: done\ndata: {}\n\n';

// The AgentEvent sequence that produced the fixture.
const FIXTURE_EVENTS: AgentEvent[] = [
  { type: 'text_delta', text: '你好' },
  { type: 'skill_call', skill_id: 'ttest', args: { y: 'age', group: 'arm' } },
  { type: 'skill_result', result: { schema_version: '1', value: 1.23 } },
  { type: 'interpretation', text: 'p < 0.05' },
  { type: 'done' },
];

describe('SSE frame serializer matches the Rust emitter', () => {
  it('produces the recorded fixture frame-for-frame', () => {
    const replayed = FIXTURE_EVENTS.map(serializeSseFrame).join('');
    expect(replayed).toBe(RUST_SSE_FIXTURE);
  });

  it('each frame carries a distinct event name and JSON data line', () => {
    for (const ev of FIXTURE_EVENTS) {
      const frame = serializeSseFrame(ev);
      expect(frame.startsWith(`event: ${ev.type}\n`)).toBe(true);
      expect(frame.endsWith('\n\n')).toBe(true);
      const dataLine = frame.split('\n')[1]!;
      expect(dataLine.startsWith('data: ')).toBe(true);
      // data payload is valid JSON.
      expect(() => JSON.parse(dataLine.slice('data: '.length))).not.toThrow();
    }
  });
});

describe('end-to-end SSE relay reproduces the recorded stream', () => {
  it('streaming the fixture events over the route yields the fixture bytes', async () => {
    const handler: MessageHandler = {
      // eslint-disable-next-line @typescript-eslint/require-await
      async *handleMessage() {
        for (const ev of FIXTURE_EVENTS) {
          yield ev;
        }
      },
    };
    const app = buildRouter({ state: makeState({ messageHandler: handler }) });
    const created = (await app.inject({ method: 'POST', url: '/api/sessions' })).json();
    const res = await app.inject({
      method: 'POST',
      url: `/api/sessions/${created.id}/messages`,
      payload: { text: 'run a t-test' },
    });
    expect(res.statusCode).toBe(200);
    expect(res.headers['content-type']).toContain('text/event-stream');
    expect(res.body).toBe(RUST_SSE_FIXTURE);
    await app.close();
  });
});

describe('non-deterministic field normalization', () => {
  it('normalizes prompt UUIDs before diffing choice_prompt frames', () => {
    const uuidRe = /[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/gi;
    const normalize = (s: string) => s.replace(uuidRe, '<uuid>');

    const recorded =
      'event: choice_prompt\ndata: {"prompt_id":"11111111-1111-4111-8111-111111111111","question":"Which test?"}\n\n';
    const liveFrame = serializeSseFrame({
      type: 'choice_prompt',
      prompt: { prompt_id: '22222222-2222-4222-8222-222222222222', question: 'Which test?' },
    });
    expect(normalize(liveFrame)).toBe(normalize(recorded));
  });
});
