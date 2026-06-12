// server/mem-store.ts — in-memory SessionStore (default for dev/tests).
// Mirrors agent_core::store::MemSessionStore semantics.

import { randomUUID } from 'node:crypto';
import { StoreError, type Session, type SessionStore, type SessionSettings, type DatasetSummary, type SessionSummary, type Message } from './state.js';

const TITLE_MAX_CHARS = 20;
const DEFAULT_TITLE = '新对话';

/** Derive a history title from the first User text message, else the default. */
function deriveTitle(session: Session): string {
  for (const msg of session.messages) {
    if ('User' in msg) {
      const content = msg.User.content;
      if ('Text' in content) {
        const text = content.Text.trim();
        if (text.length > 0) {
          return [...text].slice(0, TITLE_MAX_CHARS).join('');
        }
      }
    }
  }
  return DEFAULT_TITLE;
}

export class MemSessionStore implements SessionStore {
  private readonly sessions = new Map<string, Session>();

  create(): Promise<Session> {
    const now = new Date().toISOString();
    const session: Session = {
      id: randomUUID(),
      status: 'Active',
      created_at: now,
      last_active_at: now,
      settings: { decision_assistant: true },
      messages: [],
      datasets: [],
      skill_runs: [],
      uploaded_bytes: 0,
    };
    this.sessions.set(session.id, session);
    return Promise.resolve(session);
  }

  get(id: string): Promise<Session> {
    const s = this.sessions.get(id);
    if (!s) {
      return Promise.reject(new StoreError('not_found', 'session not found'));
    }
    return Promise.resolve(s);
  }

  updateSettings(id: string, settings: SessionSettings): Promise<void> {
    const s = this.sessions.get(id);
    if (!s) {
      return Promise.reject(new StoreError('not_found', 'session not found'));
    }
    s.settings = settings;
    s.last_active_at = new Date().toISOString();
    return Promise.resolve();
  }

  appendMessages(id: string, messages: Message[]): Promise<void> {
    const s = this.sessions.get(id);
    if (!s) {
      return Promise.reject(new StoreError('not_found', 'session not found'));
    }
    s.messages.push(...messages);
    s.last_active_at = new Date().toISOString();
    return Promise.resolve();
  }

  appendDataset(id: string, dataset: DatasetSummary): Promise<void> {
    const s = this.sessions.get(id);
    if (!s) {
      return Promise.reject(new StoreError('not_found', 'session not found'));
    }
    s.datasets.push(dataset);
    s.last_active_at = new Date().toISOString();
    return Promise.resolve();
  }

  deleteSession(id: string): Promise<void> {
    if (!this.sessions.delete(id)) {
      return Promise.reject(new StoreError('not_found', 'session not found'));
    }
    return Promise.resolve();
  }

  list(): Promise<SessionSummary[]> {
    const summaries: SessionSummary[] = [...this.sessions.values()].map((s) => ({
      id: s.id,
      status: s.status,
      created_at: s.created_at,
      last_active_at: s.last_active_at,
      message_count: s.messages.length,
      title: deriveTitle(s),
      dataset_count: s.datasets.length,
    }));
    // Sort by last_active_at descending (most recent first, Requirement 11.2).
    summaries.sort((a, b) => b.last_active_at.localeCompare(a.last_active_at));
    return Promise.resolve(summaries);
  }
}
