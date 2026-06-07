// server/sse.ts — Server-Sent Events frame serialization for the messages
// route (task 3.3). Mirrors the Rust SSE emitter in
// crates/agent-server/src/handlers/message.rs::agent_event_to_sse.
//
// Each AgentEvent maps to a distinct `event:` name and a JSON `data:` payload,
// matching the Rust backend frame-for-frame so the SSE contract replay test
// (task 3.9) diffs equal.

import type { AgentEvent } from './state.js';

/**
 * Serialize one AgentEvent into a single SSE frame:
 *   event: <name>\n
 *   data: <json>\n
 *   \n
 *
 * The `data:` payload's JSON shape mirrors the Rust emitter exactly:
 *  - text_delta / interpretation → { "text": ... }
 *  - skill_call                  → { "skill_id": ..., "args": ... }
 *  - choice_prompt               → the prompt value, JSON-serialized
 *  - skill_result                → the result value, JSON-serialized
 *  - error                       → the error payload, JSON-serialized
 *  - done                        → {}
 */
export function serializeSseFrame(event: AgentEvent): string {
  let name: string;
  let data: string;
  switch (event.type) {
    case 'text_delta':
      name = 'text_delta';
      data = JSON.stringify({ text: event.text });
      break;
    case 'interpretation':
      name = 'interpretation';
      data = JSON.stringify({ text: event.text });
      break;
    case 'choice_prompt':
      name = 'choice_prompt';
      data = JSON.stringify(event.prompt);
      break;
    case 'skill_call':
      name = 'skill_call';
      data = JSON.stringify({ skill_id: event.skill_id, args: event.args });
      break;
    case 'skill_result':
      name = 'skill_result';
      data = JSON.stringify(event.result);
      break;
    case 'error':
      name = 'error';
      data = JSON.stringify(event.payload);
      break;
    case 'done':
      name = 'done';
      data = '{}';
      break;
  }
  return `event: ${name}\ndata: ${data}\n\n`;
}
