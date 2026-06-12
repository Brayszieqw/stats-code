// File-backed SessionStore for production/dev launcher history persistence.

import { randomUUID } from 'node:crypto';
import {
  existsSync,
  mkdirSync,
  readFileSync,
  renameSync,
  writeFileSync,
} from 'node:fs';
import { homedir } from 'node:os';
import { dirname, join } from 'node:path';
import {
  StoreError,
  type DatasetSummary,
  type Message,
  type Session,
  type SessionSettings,
  type SessionStore,
  type SessionSummary,
} from '../state.js';

const APP_DIR = 'stats-code';
const SESSIONS_FILE = 'sessions.json';
const TITLE_MAX_CHARS = 20;
const DEFAULT_TITLE = '新对话';

export interface FileSessionStoreOptions {
  filePath?: string;
  now?: () => Date;
}

export function defaultSessionStorePath(): string {
  if (process.platform === 'win32') {
    const appData = process.env.APPDATA ?? join(homedir(), 'AppData', 'Roaming');
    return join(appData, APP_DIR, SESSIONS_FILE);
  }
  const xdg = process.env.XDG_CONFIG_HOME;
  const base = xdg && xdg.length > 0 ? xdg : join(homedir(), '.config');
  return join(base, APP_DIR, SESSIONS_FILE);
}

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

function cloneSession(session: Session): Session {
  return JSON.parse(JSON.stringify(session)) as Session;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

function isSession(value: unknown): value is Session {
  if (!isRecord(value)) return false;
  return (
    typeof value.id === 'string' &&
    (value.status === 'Active' || value.status === 'Archived') &&
    typeof value.created_at === 'string' &&
    typeof value.last_active_at === 'string' &&
    isRecord(value.settings) &&
    typeof value.settings.decision_assistant === 'boolean' &&
    Array.isArray(value.messages) &&
    Array.isArray(value.datasets) &&
    Array.isArray(value.skill_runs) &&
    typeof value.uploaded_bytes === 'number'
  );
}

function parseSessions(raw: string): Session[] {
  const parsed = JSON.parse(raw) as unknown;
  const sessions = Array.isArray(parsed)
    ? parsed
    : isRecord(parsed) && Array.isArray(parsed.sessions)
      ? parsed.sessions
      : [];
  return sessions.filter(isSession).map(cloneSession);
}

export function createFileSessionStore(opts: FileSessionStoreOptions = {}): SessionStore {
  const filePath = opts.filePath ?? defaultSessionStorePath();
  const now = opts.now ?? (() => new Date());
  let loaded = false;
  const sessions = new Map<string, Session>();

  function backupCorrupt(): void {
    const stamp = now().toISOString().replace(/[:.]/g, '-');
    try {
      renameSync(filePath, `${filePath}.corrupt-${stamp}`);
    } catch {
      // Best effort only; startup should still continue with an empty store.
    }
  }

  function ensureLoaded(): void {
    if (loaded) return;
    loaded = true;
    if (!existsSync(filePath)) return;
    try {
      for (const session of parseSessions(readFileSync(filePath, 'utf8'))) {
        sessions.set(session.id, session);
      }
    } catch {
      backupCorrupt();
      sessions.clear();
    }
  }

  function save(): void {
    mkdirSync(dirname(filePath), { recursive: true });
    const tmpPath = `${filePath}.tmp-${process.pid}-${now().getTime()}`;
    const payload = JSON.stringify({ version: 1, sessions: [...sessions.values()] }, null, 2);
    writeFileSync(tmpPath, payload, { encoding: 'utf8', mode: 0o600 });
    renameSync(tmpPath, filePath);
  }

  function mustGet(id: string): Session {
    ensureLoaded();
    const session = sessions.get(id);
    if (!session) {
      throw new StoreError('not_found', 'session not found');
    }
    return session;
  }

  return {
    async create(): Promise<Session> {
      ensureLoaded();
      const timestamp = now().toISOString();
      const session: Session = {
        id: randomUUID(),
        status: 'Active',
        created_at: timestamp,
        last_active_at: timestamp,
        settings: { decision_assistant: true },
        messages: [],
        datasets: [],
        skill_runs: [],
        uploaded_bytes: 0,
      };
      sessions.set(session.id, session);
      save();
      return cloneSession(session);
    },

    async get(id: string): Promise<Session> {
      return cloneSession(mustGet(id));
    },

    async updateSettings(id: string, settings: SessionSettings): Promise<void> {
      const session = mustGet(id);
      session.settings = settings;
      session.last_active_at = now().toISOString();
      save();
    },

    async appendMessages(id: string, messages: Message[]): Promise<void> {
      const session = mustGet(id);
      session.messages.push(...(JSON.parse(JSON.stringify(messages)) as Message[]));
      session.last_active_at = now().toISOString();
      save();
    },

    async appendDataset(id: string, dataset: DatasetSummary): Promise<void> {
      const session = mustGet(id);
      session.datasets.push(JSON.parse(JSON.stringify(dataset)) as DatasetSummary);
      session.last_active_at = now().toISOString();
      save();
    },

    async deleteSession(id: string): Promise<void> {
      ensureLoaded();
      if (!sessions.delete(id)) {
        throw new StoreError('not_found', 'session not found');
      }
      save();
    },

    async list(): Promise<SessionSummary[]> {
      ensureLoaded();
      const summaries: SessionSummary[] = [...sessions.values()].map((session) => ({
        id: session.id,
        status: session.status,
        created_at: session.created_at,
        last_active_at: session.last_active_at,
        message_count: session.messages.length,
        title: deriveTitle(session),
        dataset_count: session.datasets.length,
      }));
      summaries.sort((a, b) => b.last_active_at.localeCompare(a.last_active_at));
      return summaries;
    },
  };
}
