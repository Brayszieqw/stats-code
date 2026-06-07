// server/mem-store.ts — in-memory SessionStore (default for dev/tests).
// Mirrors agent_core::store::MemSessionStore semantics.

import { randomUUID } from 'node:crypto';
import { StoreError, type Session, type SessionStore, type SessionSettings, type DatasetSummary } from './state.js';

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

  appendDataset(id: string, dataset: DatasetSummary): Promise<void> {
    const s = this.sessions.get(id);
    if (!s) {
      return Promise.reject(new StoreError('not_found', 'session not found'));
    }
    s.datasets.push(dataset);
    return Promise.resolve();
  }
}
