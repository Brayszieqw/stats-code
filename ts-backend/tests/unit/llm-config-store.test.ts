// tests/unit/llm-config-store.test.ts — file-backed LlmConfigStore (task 3.6).
//
// Write→read round-trip; missing file → null; corrupt file → timestamped
// backup + null; atomic write creates parent directories; v2 per-provider
// cache format (active provider + providers map); legacy flat-file backward
// compat and upgrade-on-write; listCached/readProvider.
//
// _Requirements: 3.1, 3.2, 3.3, 3.4_

import { describe, it, expect, afterEach, vi } from 'vitest';
import { mkdtempSync, rmSync, existsSync, writeFileSync, readFileSync, readdirSync, mkdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createFileLlmConfigStore, type LlmConfig } from '@stats-code/server';

// Records every writeFileSync call's options so the permission-mode test below
// can assert on the argument the store actually requests. Windows does not
// enforce/report POSIX bits via fs.statSync().mode (empirically verified:
// statSync().mode & 0o777 reads back 0o666 regardless of the mode a prior
// writeFileSync call requested on this platform), so a statSync-based
// assertion would be flaky/always-wrong here; intercepting the call itself is
// the portable way to verify the source's intent.
const { writeFileSyncCalls } = vi.hoisted(() => ({
  writeFileSyncCalls: [] as Array<{ path: unknown; options: unknown }>,
}));

vi.mock('node:fs', async (importOriginal) => {
  const actual = await importOriginal<typeof import('node:fs')>();
  return {
    ...actual,
    writeFileSync: (path: unknown, data: unknown, options?: unknown) => {
      writeFileSyncCalls.push({ path, options });
      return (actual.writeFileSync as (...a: unknown[]) => unknown)(path, data, options);
    },
  };
});

const tmpDirs: string[] = [];
afterEach(() => {
  for (const d of tmpDirs.splice(0)) rmSync(d, { recursive: true, force: true });
  writeFileSyncCalls.length = 0;
});
function freshTmp(): string {
  const d = mkdtempSync(join(tmpdir(), 'sc-cfg-'));
  tmpDirs.push(d);
  return d;
}

describe('createFileLlmConfigStore (Requirements 3.1, 3.2, 3.3, 3.4)', () => {
  it('round-trips a written config', () => {
    const filePath = join(freshTmp(), 'nested', 'deep', 'llm-config.json');
    const store = createFileLlmConfigStore({ filePath });
    const cfg: LlmConfig = { provider: 'deepseek', api_key: 'sk-1', base_url: null, model: 'deepseek-chat' };
    store.write(cfg);
    expect(existsSync(filePath)).toBe(true);
    expect(store.read()).toEqual(cfg);
  });

  it('atomic write creates parent directories', () => {
    const filePath = join(freshTmp(), 'a', 'b', 'c', 'llm-config.json');
    const store = createFileLlmConfigStore({ filePath });
    store.write({ provider: 'qwen', api_key: 'sk-2', base_url: null, model: null });
    expect(existsSync(filePath)).toBe(true);
  });

  it('write() requests mode 0o600 on the persisted file (Requirement: key security hardening #1)', () => {
    // Asserts on the writeFileSync call argument rather than a post-write
    // statSync, because Windows does not surface POSIX permission bits
    // (statSync().mode reads back 0o666 here regardless of the mode written).
    // The argument is the portable, platform-independent way to verify the
    // store's on-disk intent: restrict the credential file to the owning
    // user (POSIX 0o600), matching the %APPDATA%-per-user isolation model.
    const filePath = join(freshTmp(), 'llm-config.json');
    const store = createFileLlmConfigStore({ filePath });
    store.write({ provider: 'deepseek', api_key: 'sk-perm-check', base_url: null, model: null });

    const tmpWrite = writeFileSyncCalls.find(
      (c) => typeof c.path === 'string' && c.path.includes('llm-config.json.tmp-'),
    );
    expect(tmpWrite).toBeDefined();
    expect(tmpWrite?.options).toMatchObject({ mode: 0o600 });
  });

  it('returns null when the file is missing', () => {
    const filePath = join(freshTmp(), 'llm-config.json');
    const store = createFileLlmConfigStore({ filePath });
    expect(store.read()).toBeNull();
  });

  it('returns null and backs up a corrupt (unparseable) file', () => {
    const dir = freshTmp();
    const filePath = join(dir, 'llm-config.json');
    writeFileSync(filePath, '{ this is not json', 'utf8');
    const fixedNow = () => new Date('2026-01-02T03:04:05.678Z');
    const store = createFileLlmConfigStore({ filePath, now: fixedNow });
    expect(store.read()).toBeNull();
    // Original file moved to a timestamped backup.
    expect(existsSync(filePath)).toBe(false);
    const backups = readdirSync(dir).filter((f) => f.startsWith('llm-config.json.corrupt-'));
    expect(backups).toHaveLength(1);
  });

  it('returns null without backup for parseable-but-unusable config (empty key)', () => {
    const dir = freshTmp();
    const filePath = join(dir, 'llm-config.json');
    writeFileSync(filePath, JSON.stringify({ provider: 'qwen', api_key: '' }), 'utf8');
    const store = createFileLlmConfigStore({ filePath });
    expect(store.read()).toBeNull();
    // The file is left intact (it parsed fine).
    expect(existsSync(filePath)).toBe(true);
    const backups = readdirSync(dir).filter((f) => f.includes('.corrupt-'));
    expect(backups).toHaveLength(0);
  });

  it('reads a pre-existing valid legacy (flat, pre-v2) config file', () => {
    const dir = freshTmp();
    mkdirSync(dir, { recursive: true });
    const filePath = join(dir, 'llm-config.json');
    writeFileSync(
      filePath,
      JSON.stringify({
        provider: 'deepseek',
        api_key: 'sk-existing',
        base_url: 'https://api.deepseek.com/v1',
        model: 'deepseek-chat',
      }),
      'utf8',
    );
    const store = createFileLlmConfigStore({ filePath });
    expect(store.read()).toEqual({
      provider: 'deepseek',
      api_key: 'sk-existing',
      base_url: 'https://api.deepseek.com/v1',
      model: 'deepseek-chat',
    });
  });

  it('a legacy file for the retired openai provider reads as null and is left untouched', () => {
    const dir = freshTmp();
    const filePath = join(dir, 'llm-config.json');
    writeFileSync(
      filePath,
      JSON.stringify({ provider: 'openai', api_key: 'sk-old', base_url: null, model: 'gpt-4' }),
      'utf8',
    );
    const store = createFileLlmConfigStore({ filePath });
    expect(store.read()).toBeNull();
    expect(existsSync(filePath)).toBe(true);
    const backups = readdirSync(dir).filter((f) => f.includes('.corrupt-'));
    expect(backups).toHaveLength(0);
  });

  it('a legacy file for an unrecognized provider also reads as null', () => {
    const dir = freshTmp();
    const filePath = join(dir, 'llm-config.json');
    writeFileSync(
      filePath,
      JSON.stringify({ provider: 'not-a-real-provider', api_key: 'sk-old', base_url: null, model: null }),
      'utf8',
    );
    const store = createFileLlmConfigStore({ filePath });
    expect(store.read()).toBeNull();
  });
});

describe('v2 per-provider cache format', () => {
  it('reads the active provider entry from a hand-written v2 file', () => {
    const dir = freshTmp();
    const filePath = join(dir, 'llm-config.json');
    writeFileSync(
      filePath,
      JSON.stringify({
        version: 2,
        active: 'kimi',
        providers: {
          kimi: { api_key: 'sk-kimi', base_url: null, model: 'kimi-latest' },
          zhipu: { api_key: 'sk-zhipu', base_url: null, model: 'glm-4.5' },
        },
      }),
      'utf8',
    );
    const store = createFileLlmConfigStore({ filePath });
    expect(store.read()).toEqual({
      provider: 'kimi',
      api_key: 'sk-kimi',
      base_url: null,
      model: 'kimi-latest',
    });
  });

  it('returns null when active points at a provider missing from the providers map', () => {
    const dir = freshTmp();
    const filePath = join(dir, 'llm-config.json');
    writeFileSync(
      filePath,
      JSON.stringify({ version: 2, active: 'kimi', providers: {} }),
      'utf8',
    );
    const store = createFileLlmConfigStore({ filePath });
    expect(store.read()).toBeNull();
  });

  it('listCached() reports every cached provider id, readProvider() fetches any of them', () => {
    const dir = freshTmp();
    const filePath = join(dir, 'llm-config.json');
    writeFileSync(
      filePath,
      JSON.stringify({
        version: 2,
        active: 'kimi',
        providers: {
          kimi: { api_key: 'sk-kimi', base_url: null, model: 'kimi-latest' },
          zhipu: { api_key: 'sk-zhipu', base_url: null, model: 'glm-4.5' },
        },
      }),
      'utf8',
    );
    const store = createFileLlmConfigStore({ filePath });
    expect(store.listCached().sort()).toEqual(['kimi', 'zhipu'].sort());
    expect(store.readProvider('zhipu')).toEqual({
      provider: 'zhipu',
      api_key: 'sk-zhipu',
      base_url: null,
      model: 'glm-4.5',
    });
    expect(store.readProvider('deepseek')).toBeNull();
  });

  it('write() upgrades a legacy flat file to v2 while preserving the pre-existing entry', () => {
    const dir = freshTmp();
    const filePath = join(dir, 'llm-config.json');
    writeFileSync(
      filePath,
      JSON.stringify({
        provider: 'deepseek',
        api_key: 'sk-old-deepseek',
        base_url: null,
        model: 'deepseek-chat',
      }),
      'utf8',
    );
    const store = createFileLlmConfigStore({ filePath });
    store.write({ provider: 'qwen', api_key: 'sk-new-qwen', base_url: null, model: 'qwen-plus' });

    // On-disk shape is now v2.
    const onDisk = JSON.parse(readFileSync(filePath, 'utf8')) as { version: number };
    expect(onDisk.version).toBe(2);

    // The newly written provider becomes active...
    expect(store.read()).toEqual({
      provider: 'qwen',
      api_key: 'sk-new-qwen',
      base_url: null,
      model: 'qwen-plus',
    });
    // ...and the pre-existing legacy entry survives the upgrade.
    expect(store.readProvider('deepseek')).toEqual({
      provider: 'deepseek',
      api_key: 'sk-old-deepseek',
      base_url: null,
      model: 'deepseek-chat',
    });
    expect(store.listCached().sort()).toEqual(['deepseek', 'qwen'].sort());
  });

  it('write() called twice for different providers keeps both cached and switches active', () => {
    const filePath = join(freshTmp(), 'llm-config.json');
    const store = createFileLlmConfigStore({ filePath });
    store.write({ provider: 'zhipu', api_key: 'sk-zhipu', base_url: null, model: 'glm-4.5' });
    store.write({ provider: 'custom', api_key: 'sk-custom', base_url: 'https://relay.example.com/v1', model: 'whatever' });

    expect(store.read()).toEqual({
      provider: 'custom',
      api_key: 'sk-custom',
      base_url: 'https://relay.example.com/v1',
      model: 'whatever',
    });
    expect(store.readProvider('zhipu')).toEqual({
      provider: 'zhipu',
      api_key: 'sk-zhipu',
      base_url: null,
      model: 'glm-4.5',
    });
    expect(store.listCached().sort()).toEqual(['custom', 'zhipu'].sort());
  });
});
