// server/conversation/llm-config-store.ts — file-backed LlmConfigStore.
//
// Persists LlmConfig to a JSON file under the per-user application data
// directory, mirroring the Rust TomlFileStore location semantics
// (%APPDATA%\stats-code\ on Windows, $XDG_CONFIG_HOME/~/.config/stats-code
// elsewhere). JSON is chosen for parity with the rest of the TS persistence.
//
// On-disk format is v2 (opencode auth.json-style per-provider cache):
//   { "version": 2, "active": "qwen",
//     "providers": { "qwen": { "api_key": "...", "base_url": null, "model": "qwen-plus" } } }
// `read()` returns the config for the active provider only; `write()` merges
// the given config into the providers map and activates it. A pre-existing
// legacy flat file (`{ provider, api_key, base_url, model }`) is read
// transparently as long as its provider is still in the current union, and is
// upgraded to v2 (preserving that one entry) on the next write.
//
// Behavior (Requirement 3):
//  - read(): config only when the active provider has a cached non-empty api_key
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
import { LLM_PROVIDER_IDS, type LlmProviderId } from './llm-catalog.js';

const APP_DIR = 'stats-code';
const CONFIG_FILE = 'llm-config.json';
const CONFIG_VERSION = 2;

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

interface ProviderEntry {
  api_key: string;
  base_url: string | null;
  model: string | null;
}

interface V2File {
  version: 2;
  active?: unknown;
  providers?: unknown;
}

type LoadResult = { status: 'missing' } | { status: 'corrupt' } | { status: 'ok'; value: unknown };

function isValidProviderId(value: unknown): value is LlmProviderId {
  return typeof value === 'string' && (LLM_PROVIDER_IDS as readonly string[]).includes(value);
}

function isValidProviderEntry(value: unknown): value is ProviderEntry {
  if (typeof value !== 'object' || value === null) return false;
  const v = value as Record<string, unknown>;
  if (typeof v.api_key !== 'string' || v.api_key.length === 0) return false;
  if (v.base_url !== undefined && v.base_url !== null && typeof v.base_url !== 'string') return false;
  if (v.model !== undefined && v.model !== null && typeof v.model !== 'string') return false;
  return true;
}

function isV2Shape(value: unknown): value is V2File {
  return (
    typeof value === 'object' && value !== null && (value as Record<string, unknown>).version === 2
  );
}

/** Legacy flat-file shape, valid only when its provider is in the current union. */
function isValidLegacyConfig(value: unknown): value is LlmConfig {
  if (typeof value !== 'object' || value === null) return false;
  const v = value as Record<string, unknown>;
  if (!isValidProviderId(v.provider)) return false;
  if (typeof v.api_key !== 'string' || v.api_key.length === 0) return false;
  if (v.base_url !== undefined && v.base_url !== null && typeof v.base_url !== 'string') return false;
  if (v.model !== undefined && v.model !== null && typeof v.model !== 'string') return false;
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

  /** Read + JSON.parse the file, without side effects. */
  function loadFile(): LoadResult {
    if (!existsSync(filePath)) return { status: 'missing' };
    let raw: string;
    try {
      raw = readFileSync(filePath, 'utf8');
    } catch {
      return { status: 'missing' };
    }
    try {
      return { status: 'ok', value: JSON.parse(raw) };
    } catch {
      return { status: 'corrupt' };
    }
  }

  /** Valid provider entries keyed by provider id, from either file shape. */
  function loadProviders(): Record<string, ProviderEntry> {
    const loaded = loadFile();
    if (loaded.status !== 'ok') return {};
    const parsed = loaded.value;
    if (isV2Shape(parsed)) {
      const providers = parsed.providers;
      if (typeof providers !== 'object' || providers === null) return {};
      const out: Record<string, ProviderEntry> = {};
      for (const [k, v] of Object.entries(providers as Record<string, unknown>)) {
        if (isValidProviderId(k) && isValidProviderEntry(v)) out[k] = v;
      }
      return out;
    }
    if (isValidLegacyConfig(parsed)) {
      return {
        [parsed.provider]: {
          api_key: parsed.api_key,
          base_url: parsed.base_url ?? null,
          model: parsed.model ?? null,
        },
      };
    }
    return {};
  }

  return {
    read(): LlmConfig | null {
      const loaded = loadFile();
      if (loaded.status === 'missing') return null;
      if (loaded.status === 'corrupt') {
        // Requirement 3.4: exists but cannot be parsed → back up + null.
        backupCorrupt();
        return null;
      }
      const parsed = loaded.value;
      if (isV2Shape(parsed)) {
        const active = parsed.active;
        if (!isValidProviderId(active)) return null;
        const providers = parsed.providers;
        if (typeof providers !== 'object' || providers === null) return null;
        const entry = (providers as Record<string, unknown>)[active];
        if (!isValidProviderEntry(entry)) return null;
        return {
          provider: active,
          api_key: entry.api_key,
          base_url: entry.base_url ?? null,
          model: entry.model ?? null,
        };
      }
      // Parseable JSON but not a usable config (e.g. empty key, unknown/legacy
      // 'openai' provider) → report unconfigured without disturbing the file.
      if (!isValidLegacyConfig(parsed)) return null;
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
      const providers = loadProviders();
      providers[config.provider] = {
        api_key: config.api_key,
        base_url: config.base_url ?? null,
        model: config.model ?? null,
      };
      const tmpPath = `${filePath}.tmp-${process.pid}-${now().getTime()}`;
      const payload = JSON.stringify(
        { version: CONFIG_VERSION, active: config.provider, providers },
        null,
        2,
      );
      writeFileSync(tmpPath, payload, { encoding: 'utf8', mode: 0o600 });
      renameSync(tmpPath, filePath);
    },

    listCached(): LlmProviderId[] {
      const providers = loadProviders();
      return (Object.keys(providers) as LlmProviderId[]).filter(
        (id) => (providers[id]?.api_key.length ?? 0) > 0,
      );
    },

    readProvider(provider: LlmProviderId): LlmConfig | null {
      const providers = loadProviders();
      const entry = providers[provider];
      if (!entry) return null;
      return { provider, api_key: entry.api_key, base_url: entry.base_url, model: entry.model };
    },
  };
}
