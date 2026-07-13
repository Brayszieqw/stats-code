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
      obj[key] = previewCell(key, record[i] ?? '');
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
): { row_count: number; columns: ColumnSummary[]; preview_rows: Record<string, string | number>[] } {
  const { headers, rows } = parseDelimitedTable(bytes, fileName);
  const columnCount = headers.length;
  if (columnCount === 0) {
    throw new Error('dataset has no columns');
  }
  const missingCounts = new Array<number>(columnCount).fill(0);
  const numericCounts = new Array<number>(columnCount).fill(0);
  let rowCount = 0;
  const preview_rows: Record<string, string | number>[] = [];

  for (const record of rows) {
    rowCount += 1;
    if (preview_rows.length < PREVIEW_ROW_LIMIT) {
      const obj: Record<string, string | number> = {};
      for (let idx = 0; idx < columnCount; idx += 1) {
        const name = headers[idx]!;
        if (!name) continue;
        obj[name] = previewCell(name, record[idx] ?? '');
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
    }
  }

  const columns: ColumnSummary[] = headers.map((name, idx) => {
    const nonMissing = rowCount - missingCounts[idx]!;
    const inferredType = nonMissing > 0 && numericCounts[idx] === nonMissing ? 'Numeric' : 'String';
    return { name, inferred_type: inferredType, missing_count: missingCounts[idx]! };
  });

  return { row_count: rowCount, columns, preview_rows };
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
        encoding: 'Utf8',
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
