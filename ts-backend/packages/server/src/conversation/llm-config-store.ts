// server/conversation/llm-config-store.ts — file-backed LlmConfigStore.
//
// Persists the LlmConfig to a JSON file under the per-user application data
// directory, mirroring the Rust TomlFileStore location semantics
// (%APPDATA%\stats-code\ on Windows, $XDG_CONFIG_HOME/~/.config/stats-code
// elsewhere). JSON is chosen for parity with the rest of the TS persistence and
// the existing LlmConfig shape.
//
// Behavior (Requirement 3):
//  - read(): config only when the file parses with a non-empty api_key
//  - missing file → null (unconfigured; startup proceeds)
//  - corrupt file → rename to llm-config.json.corrupt-<ISO8601> backup, null
//  - write(): atomic (temp + rename), creating parent dirs

import {
  existsSync,
  mkdirSync,
  readFileSync,
  renameSync,
  writeFileSync,
} from 'node:fs';
import { homedir } from 'node:os';
import { dirname, join } from 'node:path';
import type { LlmConfig, LlmConfigStore } from '../state.js';

const APP_DIR = 'stats-code';
const CONFIG_FILE = 'llm-config.json';

/**
 * Resolve the platform application-data config path:
 *   - Windows: %APPDATA%\stats-code\llm-config.json
 *   - else: $XDG_CONFIG_HOME/stats-code/llm-config.json or
 *           ~/.config/stats-code/llm-config.json
 */
export function defaultLlmConfigPath(): string {
  if (process.platform === 'win32') {
    const appData = process.env.APPDATA ?? join(homedir(), 'AppData', 'Roaming');
    return join(appData, APP_DIR, CONFIG_FILE);
  }
  const xdg = process.env.XDG_CONFIG_HOME;
  const base = xdg && xdg.length > 0 ? xdg : join(homedir(), '.config');
  return join(base, APP_DIR, CONFIG_FILE);
}

export interface FileLlmConfigStoreOptions {
  /** Defaults to the per-user app data dir; injectable for tests. */
  filePath?: string;
  /** Clock injection for deterministic backup names in tests. */
  now?: () => Date;
}

function isValidConfig(value: unknown): value is LlmConfig {
  if (typeof value !== 'object' || value === null) return false;
  const v = value as Record<string, unknown>;
  if (v.provider !== 'deepseek' && v.provider !== 'openai') return false;
  if (typeof v.api_key !== 'string' || v.api_key.length === 0) return false;
  if (v.base_url !== undefined && v.base_url !== null && typeof v.base_url !== 'string') {
    return false;
  }
  if (v.model !== undefined && v.model !== null && typeof v.model !== 'string') {
    return false;
  }
  return true;
}

export function createFileLlmConfigStore(opts: FileLlmConfigStoreOptions = {}): LlmConfigStore {
  const filePath = opts.filePath ?? defaultLlmConfigPath();
  const now = opts.now ?? (() => new Date());

  function backupCorrupt(): void {
    const stamp = now().toISOString().replace(/[:.]/g, '-');
    const backupPath = `${filePath}.corrupt-${stamp}`;
    try {
      renameSync(filePath, backupPath);
    } catch {
      // Best-effort: if the rename fails we still report unconfigured.
    }
  }

  return {
    read(): LlmConfig | null {
      if (!existsSync(filePath)) return null;
      let raw: string;
      try {
        raw = readFileSync(filePath, 'utf8');
      } catch {
        return null;
      }
      let parsed: unknown;
      try {
        parsed = JSON.parse(raw);
      } catch {
        // Requirement 3.4: exists but cannot be parsed → back up + null.
        backupCorrupt();
        return null;
      }
      if (!isValidConfig(parsed)) {
        // Parseable JSON but not a usable config (e.g. empty key) → report
        // unconfigured (Requirement 3.2/3.3) without disturbing the file.
        return null;
      }
      return {
        provider: parsed.provider,
        api_key: parsed.api_key,
        base_url: parsed.base_url ?? null,
        model: parsed.model ?? null,
      };
    },

    write(config: LlmConfig): void {
      const dir = dirname(filePath);
      mkdirSync(dir, { recursive: true });
      const tmpPath = `${filePath}.tmp-${process.pid}-${now().getTime()}`;
      const payload = JSON.stringify(
        {
          provider: config.provider,
          api_key: config.api_key,
          base_url: config.base_url ?? null,
          model: config.model ?? null,
        },
        null,
        2,
      );
      writeFileSync(tmpPath, payload, { encoding: 'utf8', mode: 0o600 });
      renameSync(tmpPath, filePath);
    },
  };
}
