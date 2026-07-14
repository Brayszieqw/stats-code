import { createHash } from 'node:crypto';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve, sep } from 'node:path';

import { executeReplay, type ReplayOutcome } from './replay.js';
import { crc32, validateEntryName } from './zip_writer.js';

const LFH_SIG = 0x04034b50;
const CDFH_SIG = 0x02014b50;
const EOCD_SIG = 0x06054b50;
const MAX_EOCD_SEARCH = 0xffff + 22;
const MAX_ENTRY_COUNT = 10_000;
const MAX_EXTRACTED_BYTES = 256 * 1024 * 1024;

export class SnapshotArchiveError extends Error {
  constructor(
    public readonly kind: 'invalid_archive' | 'archive_too_large' | 'archive_sha256_mismatch' | 'io',
    message: string,
  ) {
    super(message);
    this.name = 'SnapshotArchiveError';
  }
}

export interface ReplaySnapshotArchiveOptions {
  snapshotPath: string;
  expectedSha256?: string;
  installedReferenceSoftware?: ReadonlyArray<readonly [string, string]>;
}

export interface ReplaySnapshotArchiveResult {
  outcome: ReplayOutcome;
  archiveSha256: string;
  archiveAnchored: boolean;
}

interface ParsedEntry {
  name: string;
  bytes: Uint8Array;
}

const SHA256_HEX = /^[0-9a-f]{64}$/;

function archiveInvalid(message: string): never {
  throw new SnapshotArchiveError('invalid_archive', message);
}

function u16(view: DataView, offset: number): number {
  if (offset < 0 || offset + 2 > view.byteLength) return archiveInvalid('truncated ZIP structure');
  return view.getUint16(offset, true);
}

function u32(view: DataView, offset: number): number {
  if (offset < 0 || offset + 4 > view.byteLength) return archiveInvalid('truncated ZIP structure');
  return view.getUint32(offset, true);
}

function decodeName(bytes: Uint8Array): string {
  try {
    return new TextDecoder('utf-8', { fatal: true }).decode(bytes);
  } catch {
    return archiveInvalid('ZIP entry name is not valid UTF-8');
  }
}

function findEocd(view: DataView): number {
  const start = Math.max(0, view.byteLength - MAX_EOCD_SEARCH);
  for (let offset = view.byteLength - 22; offset >= start; offset -= 1) {
    if (u32(view, offset) !== EOCD_SIG) continue;
    const commentLength = u16(view, offset + 20);
    if (offset + 22 + commentLength === view.byteLength) return offset;
  }
  return archiveInvalid('ZIP end-of-central-directory record not found');
}

function parseStoredZip(archive: Uint8Array): ParsedEntry[] {
  if (archive.byteLength > MAX_EXTRACTED_BYTES) {
    throw new SnapshotArchiveError('archive_too_large', `snapshot archive exceeds ${MAX_EXTRACTED_BYTES} bytes`);
  }
  const view = new DataView(archive.buffer, archive.byteOffset, archive.byteLength);
  const eocd = findEocd(view);
  if (u16(view, eocd + 4) !== 0 || u16(view, eocd + 6) !== 0) archiveInvalid('multi-disk ZIP is not supported');
  const count = u16(view, eocd + 10);
  if (u16(view, eocd + 8) !== count) archiveInvalid('inconsistent ZIP entry counts');
  if (count > MAX_ENTRY_COUNT) archiveInvalid(`ZIP has too many entries: ${count}`);
  const centralSize = u32(view, eocd + 12);
  const centralOffset = u32(view, eocd + 16);
  if (centralOffset + centralSize !== eocd) archiveInvalid('invalid ZIP central-directory bounds');

  const entries: ParsedEntry[] = [];
  const names = new Set<string>();
  let totalBytes = 0;
  let cursor = centralOffset;
  for (let index = 0; index < count; index += 1) {
    if (u32(view, cursor) !== CDFH_SIG) archiveInvalid(`invalid central-directory entry ${index}`);
    const flags = u16(view, cursor + 8);
    const method = u16(view, cursor + 10);
    const expectedCrc = u32(view, cursor + 16);
    const compressedSize = u32(view, cursor + 20);
    const uncompressedSize = u32(view, cursor + 24);
    const nameLength = u16(view, cursor + 28);
    const extraLength = u16(view, cursor + 30);
    const commentLength = u16(view, cursor + 32);
    const diskStart = u16(view, cursor + 34);
    const externalAttributes = u32(view, cursor + 38);
    const localOffset = u32(view, cursor + 42);
    const centralEnd = cursor + 46 + nameLength + extraLength + commentLength;
    if (centralEnd > eocd) archiveInvalid(`truncated central-directory entry ${index}`);
    if (flags !== 0 || method !== 0) archiveInvalid('only unencrypted Stored ZIP entries are supported');
    if (diskStart !== 0) archiveInvalid('multi-disk ZIP entry is not supported');
    if (compressedSize !== uncompressedSize) archiveInvalid('Stored ZIP entry has inconsistent sizes');
    if (((externalAttributes >>> 16) & 0xf000) === 0xa000) archiveInvalid('symbolic-link ZIP entries are forbidden');

    const nameBytes = archive.subarray(cursor + 46, cursor + 46 + nameLength);
    const name = decodeName(nameBytes);
    try {
      validateEntryName(name);
    } catch (error) {
      archiveInvalid((error as Error).message);
    }
    if (names.has(name)) archiveInvalid(`duplicate ZIP entry name ${JSON.stringify(name)}`);
    names.add(name);

    if (u32(view, localOffset) !== LFH_SIG) archiveInvalid(`invalid local header for ${name}`);
    if (u16(view, localOffset + 6) !== flags || u16(view, localOffset + 8) !== method) {
      archiveInvalid(`local/central flags or method mismatch for ${name}`);
    }
    if (u32(view, localOffset + 14) !== expectedCrc
      || u32(view, localOffset + 18) !== compressedSize
      || u32(view, localOffset + 22) !== uncompressedSize) {
      archiveInvalid(`local/central digest or size mismatch for ${name}`);
    }
    const localNameLength = u16(view, localOffset + 26);
    const localExtraLength = u16(view, localOffset + 28);
    const localName = decodeName(archive.subarray(localOffset + 30, localOffset + 30 + localNameLength));
    if (localName !== name) archiveInvalid(`local/central name mismatch for ${name}`);
    const dataStart = localOffset + 30 + localNameLength + localExtraLength;
    const dataEnd = dataStart + uncompressedSize;
    if (dataEnd > centralOffset) archiveInvalid(`entry data exceeds ZIP payload bounds for ${name}`);
    const bytes = archive.slice(dataStart, dataEnd);
    if (crc32(bytes) !== expectedCrc) archiveInvalid(`CRC32 mismatch for ${name}`);
    totalBytes += bytes.byteLength;
    if (totalBytes > MAX_EXTRACTED_BYTES) {
      throw new SnapshotArchiveError('archive_too_large', `extracted snapshot exceeds ${MAX_EXTRACTED_BYTES} bytes`);
    }
    entries.push({ name, bytes });
    cursor = centralEnd;
  }
  if (cursor !== eocd) archiveInvalid('central-directory size does not match entry records');
  return entries;
}

/** Extract a validated Stored-ZIP snapshot into a new directory. */
export function extractSnapshotZipBytes(archive: Uint8Array, destination: string): string[] {
  const entries = parseStoredZip(archive);
  try {
    mkdirSync(destination);
    const root = resolve(destination);
    for (const entry of entries) {
      const target = resolve(root, ...entry.name.split('/'));
      if (!target.startsWith(`${root}${sep}`)) archiveInvalid(`ZIP entry escapes destination: ${entry.name}`);
      mkdirSync(dirname(target), { recursive: true });
      writeFileSync(target, entry.bytes, { flag: 'wx' });
    }
  } catch (error) {
    if (error instanceof SnapshotArchiveError) throw error;
    throw new SnapshotArchiveError('io', `snapshot extraction failed: ${(error as Error).message}`);
  }
  return entries.map((entry) => entry.name);
}

/** Verify an archive-level SHA-256 (when supplied), safely extract, then replay. */
export function replaySnapshotArchive(opts: ReplaySnapshotArchiveOptions): ReplaySnapshotArchiveResult {
  let archive: Uint8Array;
  try {
    archive = readFileSync(opts.snapshotPath);
  } catch (error) {
    throw new SnapshotArchiveError('io', `cannot read snapshot archive: ${(error as Error).message}`);
  }
  const archiveSha256 = createHash('sha256').update(archive).digest('hex');
  if (opts.expectedSha256 !== undefined) {
    const expected = opts.expectedSha256.toLowerCase();
    if (!SHA256_HEX.test(expected)) archiveInvalid('expected archive SHA-256 must be 64 lowercase hex characters');
    if (expected !== archiveSha256) {
      throw new SnapshotArchiveError(
        'archive_sha256_mismatch',
        `archive SHA-256 mismatch: expected ${expected}, actual ${archiveSha256}`,
      );
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'stats-code-replay-'));
  const extractedDir = join(scratch, 'snapshot');
  try {
    extractSnapshotZipBytes(archive, extractedDir);
    const outcome = executeReplay({
      extractedDir,
      installedReferenceSoftware: opts.installedReferenceSoftware ?? [],
    });
    return { outcome, archiveSha256, archiveAnchored: opts.expectedSha256 !== undefined };
  } finally {
    rmSync(scratch, { recursive: true, force: true });
  }
}
