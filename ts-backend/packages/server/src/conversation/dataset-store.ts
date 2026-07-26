// server/conversation/dataset-store.ts — filesystem-backed dataset store.
//
// Mirrors crates/agent-core/src/store/fs_dataset_store.rs:
//   layout: <root>/<sid>/<datasetId>__<sanitizedFileName>
//   parse CSV/TSV (delimiter by extension) into a DatasetSummary with
//   file_name, row_count, columns (name/inferred_type/missing_count) and a
//   64-char lowercase hex sha256 over the EXACT raw upload bytes.
//
// Behavior (Requirement 6):
//  - saveAndParse persists raw bytes, parses, re-checks the 70 MiB ceiling,
//    rejects (no append) on parse error.
//  - readRawById scans session dirs for the <datasetId>__ prefix.

import { createHash, randomUUID } from 'node:crypto';
import {
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  statSync,
  writeFileSync,
} from 'node:fs';
import { homedir } from 'node:os';
import { join } from 'node:path';
import type { ColumnSummary, DatasetStore, DatasetSummary } from '../state.js';
import { parseDelimitedTable } from './delimited-table.js';
import { isSensitiveFieldName, looksLikeDirectIdentifier } from './sensitive-data.js';

const MAX_DATASET_BYTES = 70 * 1024 * 1024; // 70 MiB
const APP_DIR = 'stats-code';

/**
 * 低基数数值列判为 Categorical 的阈值。
 *
 * 临床数据里 0/1 编码的 disease、death 全是分类变量，但纯按「能否转成数字」
 * 推断会一律得到 Numeric，于是 Table One 的自动变量分派把它们当连续变量算
 * 均值±SD——「疾病均值 0.48」是没有意义的输出。
 *
 * 判据必须同时满足下面四条，任一不满足就保持 Numeric：
 *  1. 全部取值都是整数。取值粗的连续量（人时 fu_pt 只有 0.33/0.42/… 八个
 *     取值）会掉进任何纯基数阈值里，但小数点本身就说明它是测量值而非编码；
 *     实测 demo_cohort.csv 的 fu_pt 正是被这条捞回 Numeric 的。
 *  2. 去重后的非缺失取值不超过 MAX_LEVELS 个；
 *  3. 非缺失行数至少 MIN_ROWS 行——否则 2 行的 age(42,37) 会因「只有 2 个
 *     不同值」被误判；小样本下基数本身不携带信息；
 *  4. 取值数严格少于非缺失行数（distinct < nonMissing），即真的出现过重复。
 *     全部互不相同的列（连续测量值、序号）不可能是分类变量。
 *
 * MAX_LEVELS=6 而不是更宽：它覆盖 0/1、Likert 1–5、分期 I–IV 这些真正的
 * 整数编码，同时把随访月数（demo_cohort 的 fu_time 有 8 个整数取值，是连续
 * 时间）挡在外面。整数型连续量与整数型编码在数据上无法完全区分，这里选择
 * 偏向保守——判成 Numeric 只是让用户手动把它挪到分类变量列表，而误判成
 * Categorical 会让一个连续变量在 Table One 里变成一堆 n(%) 行。
 */
const CATEGORICAL_MAX_LEVELS = 6;
const CATEGORICAL_MIN_ROWS = 20;

export type { DatasetStore } from '../state.js';

export interface FsDatasetStoreOptions {
  /** Defaults to the app-data datasets dir; injectable for tests. */
  root?: string;
}

/** Default datasets root under the per-user application data directory. */
export function defaultDatasetRoot(): string {
  if (process.platform === 'win32') {
    const appData = process.env.APPDATA ?? join(homedir(), 'AppData', 'Roaming');
    return join(appData, APP_DIR, 'datasets');
  }
  const xdg = process.env.XDG_CONFIG_HOME;
  const base = xdg && xdg.length > 0 ? xdg : join(homedir(), '.config');
  return join(base, APP_DIR, 'datasets');
}

/** Strip path separators and control characters from a filename. */
function sanitizeFilename(name: string): string {
  return [...name]
    .map((c) => (c === '/' || c === '\\' || c.charCodeAt(0) < 0x20 ? '_' : c))
    .join('');
}

function sha256HexLower(bytes: Uint8Array): string {
  return createHash('sha256').update(bytes).digest('hex');
}

const PREVIEW_ROW_LIMIT = 10;

/** Coerce a cell to number when it is a finite numeric string; otherwise keep string. */
function coerceCell(field: string): string | number {
  if (field.length === 0) return '';
  const n = Number(field);
  return Number.isFinite(n) && field.trim() !== '' ? n : field;
}

function previewCell(column: string, field: string): string | number {
  const value = field.trim();
  return isSensitiveFieldName(column) || looksLikeDirectIdentifier(value)
    ? '[已脱敏]'
    : coerceCell(value);
}

/** Re-sanitize persisted previews created by older releases before returning them. */
export function sanitizePreviewRows(
  rows: Record<string, unknown>[],
): Record<string, string | number>[] {
  return rows.map((row) => Object.fromEntries(
    Object.entries(row).map(([column, value]) => [column, previewCell(column, String(value))]),
  ));
}

/**
 * Extract the first `limit` data rows as plain objects keyed by header name.
 * Safe for SPA preview; does not invent synthetic data.
 */
export function extractPreviewRows(
  bytes: Uint8Array,
  fileName: string,
  limit = PREVIEW_ROW_LIMIT,
): Record<string, string | number>[] {
  const { headers, rows: records } = parseDelimitedTable(bytes, fileName);
  if (records.length === 0) return [];
  const rows: Record<string, string | number>[] = [];
  for (const record of records.slice(0, limit)) {
    const obj: Record<string, string | number> = {};
    for (let i = 0; i < headers.length; i += 1) {
      const key = headers[i]!;
      if (!key) continue;
      Object.defineProperty(obj, key, {
        value: previewCell(key, record[i] ?? ''),
        enumerable: true,
        configurable: true,
        writable: true,
      });
    }
    rows.push(obj);
  }
  return rows;
}

/** Parse CSV/TSV bytes into a DatasetSummary (without dataset_id / sha256). */
function parseTextTable(
  bytes: Uint8Array,
  fileName: string,
  _ext: string,
): {
  row_count: number;
  columns: ColumnSummary[];
  preview_rows: Record<string, string | number>[];
  encoding: DatasetSummary['encoding'];
} {
  const { headers, rows, encoding } = parseDelimitedTable(bytes, fileName);
  const columnCount = headers.length;
  if (columnCount === 0) {
    throw new Error('dataset has no columns');
  }
  const missingCounts = new Array<number>(columnCount).fill(0);
  const numericCounts = new Array<number>(columnCount).fill(0);
  // 每列非缺失取值的去重集合，用于低基数判定。一旦超过阈值就停止累积，
  // 避免高基数列（如主键）在大文件上把整列值都留在内存里。
  const distinctValues = Array.from({ length: columnCount }, () => new Set<string>());
  const distinctOverflow = new Array<boolean>(columnCount).fill(false);
  let rowCount = 0;
  const preview_rows: Record<string, string | number>[] = [];

  for (const record of rows) {
    rowCount += 1;
    if (preview_rows.length < PREVIEW_ROW_LIMIT) {
      const obj: Record<string, string | number> = {};
      for (let idx = 0; idx < columnCount; idx += 1) {
        const name = headers[idx]!;
        if (!name) continue;
        Object.defineProperty(obj, name, {
          value: previewCell(name, record[idx] ?? ''),
          enumerable: true,
          configurable: true,
          writable: true,
        });
      }
      preview_rows.push(obj);
    }
    for (let idx = 0; idx < columnCount; idx += 1) {
      const field = (record[idx] ?? '').trim();
      if (field.length === 0) {
        missingCounts[idx] = (missingCounts[idx] ?? 0) + 1;
        continue;
      }
      if (field !== '' && Number.isFinite(Number(field))) {
        numericCounts[idx] = (numericCounts[idx] ?? 0) + 1;
      }
      if (!distinctOverflow[idx]) {
        const seen = distinctValues[idx]!;
        seen.add(field);
        // 超过阈值就不可能判为低基数，清空集合止损（继续 add 只会白占内存）。
        if (seen.size > CATEGORICAL_MAX_LEVELS) {
          distinctOverflow[idx] = true;
          seen.clear();
        }
      }
    }
  }

  const columns: ColumnSummary[] = headers.map((name, idx) => {
    const nonMissing = rowCount - missingCounts[idx]!;
    const isNumeric = nonMissing > 0 && numericCounts[idx] === nonMissing;
    const distinct = distinctOverflow[idx] ? Number.POSITIVE_INFINITY : distinctValues[idx]!.size;
    // 低基数**整数**列（0/1 编码的 disease/death、分期、Likert）判为 Categorical，
    // 让 Table One 用 n(%) 与卡方检验描述，而不是对它算均值±SD。
    // 含小数的列一律留给 Numeric：小数点说明是测量值，不是分类编码。
    const allIntegers = !distinctOverflow[idx]
      && [...distinctValues[idx]!].every((value) => Number.isInteger(Number(value)));
    const isLowCardinality = isNumeric
      && allIntegers
      && nonMissing >= CATEGORICAL_MIN_ROWS
      && distinct <= CATEGORICAL_MAX_LEVELS
      && distinct < nonMissing;
    const inferredType = isNumeric
      ? (isLowCardinality ? 'Categorical' : 'Numeric')
      : 'String';
    return { name, inferred_type: inferredType, missing_count: missingCounts[idx]! };
  });

  return { row_count: rowCount, columns, preview_rows, encoding };
}

export function createFsDatasetStore(opts: FsDatasetStoreOptions = {}): DatasetStore {
  const root = opts.root ?? defaultDatasetRoot();

  function sessionDir(sid: string): string {
    return join(root, sid);
  }

  return {
    async saveAndParse(sid, fileName, bytes): Promise<DatasetSummary> {
      // Re-check the 70 MiB ceiling (route body-limit is the first gate).
      if (bytes.byteLength > MAX_DATASET_BYTES) {
        throw new Error('dataset exceeds 70 MiB limit');
      }
      if (bytes.byteLength === 0) {
        throw new Error('dataset is empty');
      }
      const ext = (fileName.split('.').pop() ?? '').toLowerCase();
      if (ext !== 'csv' && ext !== 'tsv') {
        throw new Error(`unsupported file extension: ${ext}`);
      }

      // Parse FIRST so an unparseable upload never persists a summary.
      const parsed = parseTextTable(bytes, fileName, ext);

      const datasetId = randomUUID();
      const dir = sessionDir(sid);
      mkdirSync(dir, { recursive: true });
      const path = join(dir, `${datasetId}__${sanitizeFilename(fileName)}`);
      writeFileSync(path, bytes);

      const summary: DatasetSummary = {
        dataset_id: datasetId,
        file_name: fileName,
        size_bytes: bytes.byteLength,
        encoding: parsed.encoding,
        row_count: parsed.row_count,
        columns: parsed.columns,
        uploaded_at: new Date().toISOString(),
        sha256: sha256HexLower(bytes),
        preview_rows: parsed.preview_rows,
      };
      return Promise.resolve(summary);
    },

    readRawById(datasetId): Promise<Uint8Array> {
      const prefix = `${datasetId}__`;
      if (!existsSync(root)) {
        return Promise.reject(new Error(`dataset ${datasetId} not found in store`));
      }
      for (const sessionEntry of readdirSync(root)) {
        const sessionPath = join(root, sessionEntry);
        if (!statSync(sessionPath).isDirectory()) continue;
        for (const fileEntry of readdirSync(sessionPath)) {
          if (fileEntry.startsWith(prefix)) {
            return Promise.resolve(new Uint8Array(readFileSync(join(sessionPath, fileEntry))));
          }
        }
      }
      return Promise.reject(new Error(`dataset ${datasetId} not found in store`));
    },
  };
}
