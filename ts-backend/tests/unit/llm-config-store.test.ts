// tests/unit/llm-config-store.test.ts — file-backed LlmConfigStore (task 3.6).
//
// Write→read round-trip; missing file → null; corrupt file → timestamped
// backup + null; atomic write creates parent directories.
//
// _Requirements: 3.1, 3.2, 3.3, 3.4_

import { describe, it, expect, afterEach } from 'vitest';
import { mkdtempSync, rmSync, existsSync, writeFileSync, readdirSync, mkdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createFileLlmConfigStore, type LlmConfig } from '@stats-code/server';

const tmpDirs: string[] = [];
afterEach(() => {
  for (const d of tmpDirs.splice(0)) rmSync(d, { recursive: true, force: true });
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
    store.write({ provider: 'openai', api_key: 'sk-2', base_url: null, model: null });
    expect(existsSync(filePath)).toBe(true);
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
    writeFileSync(filePath, JSON.stringify({ provider: 'openai', api_key: '' }), 'utf8');
    const store = createFileLlmConfigStore({ filePath });
    expect(store.read()).toBeNull();
    // The file is left intact (it parsed fine).
    expect(existsSync(filePath)).toBe(true);
    const backups = readdirSync(dir).filter((f) => f.includes('.corrupt-'));
    expect(backups).toHaveLength(0);
  });

  it('reads a pre-existing valid config file', () => {
    const dir = freshTmp();
    mkdirSync(dir, { recursive: true });
    const filePath = join(dir, 'llm-config.json');
    writeFileSync(
      filePath,
      JSON.stringify({ provider: 'openai', api_key: 'sk-existing', base_url: 'https://x/v1', model: 'gpt' }),
      'utf8',
    );
    const store = createFileLlmConfigStore({ filePath });
    expect(store.read()).toEqual({
      provider: 'openai',
      api_key: 'sk-existing',
      base_url: 'https://x/v1',
      model: 'gpt',
    });
  });
});
