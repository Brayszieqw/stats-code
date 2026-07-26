// tests/unit/dataset-store.test.ts — FsDatasetStore (task 5.6).
//
// CSV/TSV parse produces correct row count and columns; readRawById returns
// exact bytes; unparseable input rejects without appending.
//
// _Requirements: 6.1, 6.2, 6.7, 6.8_

import { describe, it, expect, afterEach } from 'vitest';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createHash, randomUUID } from 'node:crypto';
import { createFsDatasetStore } from '@stats-code/server';
import { extractPreviewRows } from '../../packages/server/src/conversation/dataset-store.js';

const tmpDirs: string[] = [];
afterEach(() => {
  for (const d of tmpDirs.splice(0)) rmSync(d, { recursive: true, force: true });
});
function freshRoot(): string {
  const d = mkdtempSync(join(tmpdir(), 'sc-ds-'));
  tmpDirs.push(d);
  return d;
}

const enc = (s: string) => new TextEncoder().encode(s);

describe('createFsDatasetStore (Requirements 6.1, 6.2, 6.7, 6.8)', () => {
  it('parses CSV into correct row count and columns with sha256', async () => {
    const store = createFsDatasetStore({ root: freshRoot() });
    const sid = randomUUID();
    const bytes = enc('age,name\n42,alice\n37,bob\n');
    const summary = await store.saveAndParse(sid, 'data.csv', bytes);
    expect(summary.file_name).toBe('data.csv');
    expect(summary.row_count).toBe(2);
    expect(summary.columns.map((c) => c.name)).toEqual(['age', 'name']);
    expect(summary.columns[0].inferred_type).toBe('Numeric');
    expect(summary.columns[1].inferred_type).toBe('String');
    expect(summary.sha256).toBe(createHash('sha256').update(bytes).digest('hex'));
    expect(summary.preview_rows).toEqual([
      { age: 42, name: '[已脱敏]' },
      { age: 37, name: '[已脱敏]' },
    ]);
  });

  it('keeps a small numeric column Numeric even when values repeat', async () => {
    // 2 行的 age(42,37) 去重后只有 2 个取值，但样本太小、基数不携带信息；
    // 低基数判定要求至少 CATEGORICAL_MIN_ROWS 行，因此仍是 Numeric。
    const store = createFsDatasetStore({ root: freshRoot() });
    const bytes = enc('age\n42\n42\n37\n');
    const summary = await store.saveAndParse(randomUUID(), 'small.csv', bytes);
    expect(summary.columns[0].inferred_type).toBe('Numeric');
  });

  it('infers a low-cardinality numeric column as Categorical', async () => {
    // 0/1 编码的临床结局：24 行、2 个取值 → 分类变量。否则 Table One 会对它
    // 算出「疾病均值 0.5 ± 0.51」这种无意义的连续型描述。
    const store = createFsDatasetStore({ root: freshRoot() });
    const rows = Array.from({ length: 24 }, (_, i) => String(i % 2)).join('\n');
    const bytes = enc('disease\n' + rows + '\n');
    const summary = await store.saveAndParse(randomUUID(), 'binary.csv', bytes);
    expect(summary.columns[0].inferred_type).toBe('Categorical');
  });

  it('keeps a low-cardinality decimal column Numeric (person-time, not a code)', async () => {
    // 真机 demo_cohort.csv 的 fu_pt：只有 8 个取值却是人时测量值。
    // 小数点是「测量值而非编码」的判据，纯基数阈值分不开这两者。
    const store = createFsDatasetStore({ root: freshRoot() });
    const levels = ['0.33', '0.42', '0.50', '0.67'];
    const rows = Array.from({ length: 24 }, (_, i) => levels[i % levels.length]).join('\n');
    const bytes = enc('fu_pt\n' + rows + '\n');
    const summary = await store.saveAndParse(randomUUID(), 'pt.csv', bytes);
    expect(summary.columns[0].inferred_type).toBe('Numeric');
  });

  it('keeps a coarse integer measurement Numeric when it exceeds the level cap', async () => {
    // 真机 demo_cohort.csv 的 fu_time：8 个整数取值的随访月数。
    // 阈值定在 6 就是为了把这种「取值粗的连续时间」挡在 Categorical 之外。
    const store = createFsDatasetStore({ root: freshRoot() });
    const rows = Array.from({ length: 32 }, (_, i) => String(4 + (i % 8))).join('\n');
    const bytes = enc('fu_time\n' + rows + '\n');
    const summary = await store.saveAndParse(randomUUID(), 'fu.csv', bytes);
    expect(summary.columns[0].inferred_type).toBe('Numeric');
  });

  it('infers a 4-level integer code as Categorical (disease stage)', async () => {
    // 分期 1–4：整数、基数在阈值内、有重复 → 分类变量。
    const store = createFsDatasetStore({ root: freshRoot() });
    const rows = Array.from({ length: 24 }, (_, i) => String((i % 4) + 1)).join('\n');
    const bytes = enc('stage\n' + rows + '\n');
    const summary = await store.saveAndParse(randomUUID(), 'stage.csv', bytes);
    expect(summary.columns[0].inferred_type).toBe('Categorical');
  });

  it('keeps a high-cardinality numeric column Numeric', async () => {
    // 30 个互不相同的测量值：取值数超过阈值，是连续变量。
    const store = createFsDatasetStore({ root: freshRoot() });
    const rows = Array.from({ length: 30 }, (_, i) => String(20 + i * 0.7)).join('\n');
    const bytes = enc('bmi\n' + rows + '\n');
    const summary = await store.saveAndParse(randomUUID(), 'cont.csv', bytes);
    expect(summary.columns[0].inferred_type).toBe('Numeric');
  });

  it('keeps an all-distinct numeric column Numeric even under the level cap', async () => {
    // 8 行、8 个互不相同的整数：取值数虽在阈值内，但 distinct == nonMissing，
    // 全不重复的列不可能是分类变量（distinct < nonMissing 这条守卫）。
    const store = createFsDatasetStore({ root: freshRoot() });
    const rows = Array.from({ length: 8 }, (_, i) => String(i + 1)).join('\n');
    const bytes = enc('seq\n' + rows + '\n');
    const summary = await store.saveAndParse(randomUUID(), 'seq.csv', bytes);
    expect(summary.columns[0].inferred_type).toBe('Numeric');
  });

  it('redacts identifier-shaped values even when the column name is generic', async () => {
    const store = createFsDatasetStore({ root: freshRoot() });
    const sid = randomUUID();
    const bytes = enc('participant_id,notes\nP001,alice@example.com\nP002,ordinary\n');
    const summary = await store.saveAndParse(sid, 'data.csv', bytes);
    expect(summary.preview_rows).toEqual([
      { participant_id: 'P001', notes: '[已脱敏]' },
      { participant_id: 'P002', notes: 'ordinary' },
    ]);
  });

  it('preserves markup as data without allowing special headers to mutate the preview object', async () => {
    const store = createFsDatasetStore({ root: freshRoot() });
    const sid = randomUUID();
    const bytes = enc('__proto__,label\nkept,"<img src=x onerror=""alert(1)"">"\n');
    const summary = await store.saveAndParse(sid, 'data.csv', bytes);
    const row = summary.preview_rows[0]!;

    expect(Object.getPrototypeOf(row)).toBe(Object.prototype);
    expect(Object.hasOwn(row, '__proto__')).toBe(true);
    expect(row.__proto__).toBe('kept');
    expect(row.label).toBe('<img src=x onerror="alert(1)">');
  });

  it('extractPreviewRows also guards against special headers mutating the preview object', () => {
    const bytes = enc('__proto__,label\nkept,"<img src=x onerror=""alert(1)"">"\n');
    const rows = extractPreviewRows(bytes, 'data.csv');
    const row = rows[0]!;

    expect(Object.getPrototypeOf(row)).toBe(Object.prototype);
    expect(Object.hasOwn(row, '__proto__')).toBe(true);
    expect(row.__proto__).toBe('kept');
    expect(row.label).toBe('<img src=x onerror="alert(1)">');
  });

  it('parses TSV using a tab delimiter', async () => {
    const store = createFsDatasetStore({ root: freshRoot() });
    const sid = randomUUID();
    const bytes = enc('col1\tcol2\nfoo\t1\nbar\t2\n');
    const summary = await store.saveAndParse(sid, 'data.tsv', bytes);
    expect(summary.columns.map((c) => c.name)).toEqual(['col1', 'col2']);
    expect(summary.row_count).toBe(2);
  });

  it('counts missing values per column', async () => {
    const store = createFsDatasetStore({ root: freshRoot() });
    const sid = randomUUID();
    const bytes = enc('a,b\n1,\n2,x\n,y\n');
    const summary = await store.saveAndParse(sid, 'data.csv', bytes);
    const a = summary.columns.find((c) => c.name === 'a')!;
    const b = summary.columns.find((c) => c.name === 'b')!;
    expect(a.missing_count).toBe(1);
    expect(b.missing_count).toBe(1);
  });

  it('readRawById returns the exact bytes', async () => {
    const store = createFsDatasetStore({ root: freshRoot() });
    const sid = randomUUID();
    const bytes = enc('x,y\n1,2\n');
    const summary = await store.saveAndParse(sid, 'd.csv', bytes);
    const raw = await store.readRawById(summary.dataset_id);
    expect(Buffer.from(raw).equals(Buffer.from(bytes))).toBe(true);
  });

  it('rejects an unsupported extension without persisting', async () => {
    const store = createFsDatasetStore({ root: freshRoot() });
    const sid = randomUUID();
    await expect(store.saveAndParse(sid, 'data.bin', enc('not a table'))).rejects.toThrow(
      /unsupported file extension/,
    );
  });

  it('rejects an empty payload', async () => {
    const store = createFsDatasetStore({ root: freshRoot() });
    const sid = randomUUID();
    await expect(store.saveAndParse(sid, 'data.csv', new Uint8Array(0))).rejects.toThrow(/empty/);
  });

  it('readRawById rejects an unknown dataset id', async () => {
    const store = createFsDatasetStore({ root: freshRoot() });
    await expect(store.readRawById(randomUUID())).rejects.toThrow(/not found/);
  });
});
