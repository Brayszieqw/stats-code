import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { mkdtempSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { createFileSessionStore } from '@stats-code/server';

describe('file session store empty-shell reuse', () => {
  let dir: string;
  let filePath: string;

  beforeEach(() => {
    dir = mkdtempSync(join(tmpdir(), 'stats-sess-'));
    filePath = join(dir, 'sessions.json');
  });

  afterEach(() => {
    rmSync(dir, { recursive: true, force: true });
  });

  it('reuses the newest empty shell and purges extra empties', async () => {
    const store = createFileSessionStore({ filePath });
    const a = await store.create();
    // Force a second empty by temporarily marking a as non-empty then clearing is hard;
    // instead create, append a message, create again, then create empty after delete path.
    await store.appendMessages(a.id, [
      {
        User: {
          id: '11111111-1111-4111-8111-111111111111',
          created_at: new Date().toISOString(),
          content: { Text: 'hello' },
        },
      },
    ]);
    const empty1 = await store.create();
    expect(empty1.id).not.toBe(a.id);
    const empty2 = await store.create();
    // Second create on empty should reuse empty1
    expect(empty2.id).toBe(empty1.id);
    const list = await store.list();
    const empties = list.filter((s) => s.message_count === 0 && s.dataset_count === 0);
    expect(empties).toHaveLength(1);
    expect(list.some((s) => s.id === a.id)).toBe(true);
  });
});
