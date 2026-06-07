// snapshot/zip_writer.ts — deterministic ZIP writer (Phase 6, task 13.4).
// Transcribed from crates/stats-code/src/snapshot/zip_writer.rs.
//
// Determinism contract: Stored (no compression), fixed mtime 1980-01-01T00:00:00Z
// (DOS date 0x0021, time 0x0000), entries sorted lexicographically by raw name
// bytes, no clock/env/randomness. Atomic write: write to <dest>.tmp + fsync +
// rename; any error removes the partial .tmp and leaves no artifact at dest.

import { writeFileSync, openSync, fsyncSync, closeSync, renameSync, rmSync, writeSync } from 'node:fs';

export interface ZipEntry {
  /** Archive path inside the zip; forward-slash separators only. */
  name: string;
  /** File payload bytes. */
  bytes: Uint8Array;
}

export class ZipWriteError extends Error {
  constructor(
    public readonly kind: 'io' | 'invalid_entry',
    message: string,
  ) {
    super(message);
    this.name = 'ZipWriteError';
  }
}

const DOS_DATE_1980_01_01 = 0x0021;
const DOS_TIME_MIDNIGHT = 0x0000;
const VERSION_NEEDED = 20;
const VERSION_MADE_BY = 20;
const METHOD_STORED = 0;
const GENERAL_PURPOSE_FLAG = 0;
const LFH_SIG = 0x04034b50;
const CDFH_SIG = 0x02014b50;
const EOCD_SIG = 0x06054b50;

function validateEntryName(name: string): void {
  const invalid = (reason: string): never => {
    throw new ZipWriteError('invalid_entry', `invalid entry name ${JSON.stringify(name)}: ${reason}`);
  };
  if (name.length === 0) invalid('entry name is empty');
  if (name.startsWith('/')) invalid("entry name must not start with '/'");
  if (name.includes('\\')) invalid('entry name must use forward-slash separators only');
  if (name.includes('\u0000')) invalid('entry name must not contain a NUL byte');
  for (const component of name.split('/')) {
    if (component === '..') invalid("entry name must not contain a '..' path component");
  }
  if (Buffer.byteLength(name, 'utf8') > 0xffff) invalid('entry name longer than 65535 bytes');
}

// CRC32 (IEEE reflected 0xedb88320), built once.
const CRC32_TABLE = (() => {
  const table = new Uint32Array(256);
  for (let i = 0; i < 256; i += 1) {
    let c = i;
    for (let j = 0; j < 8; j += 1) {
      c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    }
    table[i] = c >>> 0;
  }
  return table;
})();

export function crc32(bytes: Uint8Array): number {
  let crc = 0xffffffff;
  for (let i = 0; i < bytes.length; i += 1) {
    const idx = (crc ^ bytes[i]!) & 0xff;
    crc = (crc >>> 8) ^ CRC32_TABLE[idx]!;
  }
  return (crc ^ 0xffffffff) >>> 0;
}

/** Encode the in-memory ZIP bytes for already-validated, already-sorted entries. */
export function encodeArchive(entries: readonly ZipEntry[]): Uint8Array {
  const parts: number[] = [];
  const pushU16 = (v: number) => {
    parts.push(v & 0xff, (v >>> 8) & 0xff);
  };
  const pushU32 = (v: number) => {
    parts.push(v & 0xff, (v >>> 8) & 0xff, (v >>> 16) & 0xff, (v >>> 24) & 0xff);
  };
  const pushBytes = (b: Uint8Array) => {
    for (let i = 0; i < b.length; i += 1) parts.push(b[i]!);
  };

  interface CdEntry {
    nameBytes: Uint8Array;
    crc: number;
    size: number;
    offset: number;
  }
  const cdEntries: CdEntry[] = [];

  for (const entry of entries) {
    const offset = parts.length;
    const crc = crc32(entry.bytes);
    const size = entry.bytes.length;
    const nameBytes = new TextEncoder().encode(entry.name);

    pushU32(LFH_SIG);
    pushU16(VERSION_NEEDED);
    pushU16(GENERAL_PURPOSE_FLAG);
    pushU16(METHOD_STORED);
    pushU16(DOS_TIME_MIDNIGHT);
    pushU16(DOS_DATE_1980_01_01);
    pushU32(crc);
    pushU32(size);
    pushU32(size);
    pushU16(nameBytes.length);
    pushU16(0); // extra field length
    pushBytes(nameBytes);
    pushBytes(entry.bytes);

    cdEntries.push({ nameBytes, crc, size, offset });
  }

  const cdOffset = parts.length;
  for (const cd of cdEntries) {
    pushU32(CDFH_SIG);
    pushU16(VERSION_MADE_BY);
    pushU16(VERSION_NEEDED);
    pushU16(GENERAL_PURPOSE_FLAG);
    pushU16(METHOD_STORED);
    pushU16(DOS_TIME_MIDNIGHT);
    pushU16(DOS_DATE_1980_01_01);
    pushU32(cd.crc);
    pushU32(cd.size);
    pushU32(cd.size);
    pushU16(cd.nameBytes.length);
    pushU16(0); // extra field length
    pushU16(0); // file comment length
    pushU16(0); // disk number start
    pushU16(0); // internal file attributes
    pushU32(0); // external file attributes
    pushU32(cd.offset);
    pushBytes(cd.nameBytes);
  }
  const cdSize = parts.length - cdOffset;

  const entryCount = cdEntries.length;
  pushU32(EOCD_SIG);
  pushU16(0); // disk number
  pushU16(0); // disk where CD starts
  pushU16(entryCount);
  pushU16(entryCount);
  pushU32(cdSize);
  pushU32(cdOffset);
  pushU16(0); // comment length

  return Uint8Array.from(parts);
}

/** Sort entries by raw name bytes and encode the deterministic archive. */
export function buildZipBytes(entries: readonly ZipEntry[]): Uint8Array {
  for (const e of entries) {
    validateEntryName(e.name);
  }
  const sorted = [...entries].sort((a, b) => {
    const an = new TextEncoder().encode(a.name);
    const bn = new TextEncoder().encode(b.name);
    const len = Math.min(an.length, bn.length);
    for (let i = 0; i < len; i += 1) {
      if (an[i]! !== bn[i]!) return an[i]! - bn[i]!;
    }
    return an.length - bn.length;
  });
  return encodeArchive(sorted);
}

/**
 * Write entries to a deterministic zip at `destTmp` with fsync. Does NOT
 * rename to the final path (the caller does the atomic rename). Removes the
 * partial temp file on any error.
 */
export function writeDeterministicZip(entries: readonly ZipEntry[], destTmp: string): void {
  const archive = buildZipBytes(entries); // validation happens here, before any fs touch
  let fd: number | undefined;
  try {
    fd = openSync(destTmp, 'w');
    writeSync(fd, archive);
    fsyncSync(fd);
    closeSync(fd);
    fd = undefined;
  } catch (err) {
    if (fd !== undefined) {
      try {
        closeSync(fd);
      } catch {
        /* ignore */
      }
    }
    try {
      rmSync(destTmp, { force: true });
    } catch {
      /* ignore */
    }
    throw new ZipWriteError('io', `io error writing zip at ${destTmp}: ${(err as Error).message}`);
  }
}

/**
 * Atomic deterministic write: build the zip, write <dest>.tmp + fsync, then
 * rename to dest. Any failure leaves no artifact at dest.
 */
export function writeSnapshotAtomic(entries: readonly ZipEntry[], dest: string): void {
  const tmp = `${dest}.tmp`;
  writeDeterministicZip(entries, tmp);
  try {
    renameSync(tmp, dest);
  } catch (err) {
    try {
      rmSync(tmp, { force: true });
    } catch {
      /* ignore */
    }
    throw new ZipWriteError('io', `io error renaming ${tmp} → ${dest}: ${(err as Error).message}`);
  }
}

export { writeFileSync };
