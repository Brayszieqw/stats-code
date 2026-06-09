// tests/unit/session-list.test.ts — MemSessionStore.list() derivations.
//
// Validates title / message_count / dataset_count derivation and the empty-store
// case (Requirements 11.3, 11.4).

import { describe, it, expect } from 'vitest';
import { MemSessionStore, type DatasetSummary } from '@stats-code/server';

function userText(text: string) {
  return {
    User: {
      id: '11111111-1111-4111-8111-111111111111',
      created_at: new Date().toISOString(),
      content: { Text: text },
    },
  };
}

function fakeDataset(): DatasetSummary {
  return {
    dataset_id: '22222222-2222-4222-8222-222222222222',
    file_name: 'd.csv',
    size_bytes: 10,
    encoding: 'Utf8',
    row_count: 1,
    columns: [],
    uploaded_at: new Date().toISOString(),
    sha256: null,
  };
}

describe('MemSessionStore.list()', () => {
  it('returns [] for an empty store', async () => {
    const store = new MemSessionStore();
    await expect(store.list()).resolves.toEqual([]);
  });

  it('defaults the title to "新对话" when there is no user text message', async () => {
    const store = new MemSessionStore();
    await store.create();
    const [summary] = await store.list();
    expect(summary!.title).toBe('新对话');
    expect(summary!.message_count).toBe(0);
    expect(summary!.dataset_count).toBe(0);
  });

  it('derives title from the first user text message (truncated to 20 chars)', async () => {
    const store = new MemSessionStore();
    const s = await store.create();
    const long = '一二三四五六七八九十一二三四五六七八九十一二三四五';
    s.messages.push(userText(long));
    s.messages.push(userText('第二条不该被采用'));
    const [summary] = await store.list();
    expect(summary!.title).toBe([...long].slice(0, 20).join(''));
    expect(summary!.message_count).toBe(2);
  });

  it('counts datasets in dataset_count', async () => {
    const store = new MemSessionStore();
    const s = await store.create();
    s.datasets.push(fakeDataset());
    s.datasets.push(fakeDataset());
    const [summary] = await store.list();
    expect(summary!.dataset_count).toBe(2);
  });

  it('does not leak sensitive fields in the summary shape', async () => {
    const store = new MemSessionStore();
    await store.create();
    const [summary] = await store.list();
    expect(Object.keys(summary!).sort()).toEqual(
      ['created_at', 'dataset_count', 'id', 'last_active_at', 'message_count', 'status', 'title'].sort(),
    );
  });
});
