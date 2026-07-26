/** A single parser shared by upload summaries, audits, and deterministic runs. */
export interface DelimitedTable {
  headers: string[];
  rows: string[][];
  /** Encoding the bytes were actually decoded with (D2). */
  encoding: DetectedEncoding;
}

/** Contract `Encoding` tokens (contract/domain.ts); UTF-16 LE/BE both map to 'Utf16'. */
export type DetectedEncoding = 'Utf8' | 'Gbk' | 'Utf16';

export interface DecodedText {
  text: string;
  encoding: DetectedEncoding;
}

/** Decode UTF-16BE by byte-swapping when the runtime lacks the utf-16be label. */
function decodeUtf16Be(bytes: Uint8Array): string {
  try {
    return new TextDecoder('utf-16be').decode(bytes);
  } catch {
    const even = bytes.length - (bytes.length % 2);
    const swapped = new Uint8Array(even);
    for (let i = 0; i + 1 < even; i += 2) {
      swapped[i] = bytes[i + 1]!;
      swapped[i + 1] = bytes[i]!;
    }
    return new TextDecoder('utf-16le').decode(swapped);
  }
}

/** NUL as a runtime constant — avoids embedding a control character in source. */
const NUL_CHAR = String.fromCharCode(0);

/**
 * Decode dataset bytes with encoding detection (D2) instead of assuming UTF-8:
 *  1. BOM sniff: FF FE = UTF-16LE, FE FF = UTF-16BE, EF BB BF = UTF-8;
 *  2. no BOM: strict UTF-8 (`fatal: true`) first;
 *  3. NUL-riddled "valid UTF-8" is BOM-less UTF-16 (real CSVs never carry NUL);
 *  4. anything else falls back to GBK, the dominant legacy zh-CN encoding.
 * Detection affects parsing/labels only; stored bytes and sha256 stay raw.
 */
export function decodeDatasetText(bytes: Uint8Array): DecodedText {
  if (bytes.length >= 2 && bytes[0] === 0xff && bytes[1] === 0xfe) {
    return { text: new TextDecoder('utf-16le').decode(bytes.subarray(2)), encoding: 'Utf16' };
  }
  if (bytes.length >= 2 && bytes[0] === 0xfe && bytes[1] === 0xff) {
    return { text: decodeUtf16Be(bytes.subarray(2)), encoding: 'Utf16' };
  }
  if (bytes.length >= 3 && bytes[0] === 0xef && bytes[1] === 0xbb && bytes[2] === 0xbf) {
    return { text: new TextDecoder('utf-8').decode(bytes.subarray(3)), encoding: 'Utf8' };
  }
  let utf8: string | null = null;
  try {
    utf8 = new TextDecoder('utf-8', { fatal: true }).decode(bytes);
  } catch {
    utf8 = null;
  }
  if (utf8 !== null && !utf8.includes(NUL_CHAR)) {
    return { text: utf8, encoding: 'Utf8' };
  }
  if (utf8 !== null) {
    // Contains NUL: BOM-less UTF-16. ASCII text puts the NUL in the high
    // byte — odd offsets for LE, even offsets for BE.
    let evenZeros = 0;
    let oddZeros = 0;
    for (let i = 0; i < bytes.length; i += 1) {
      if (bytes[i] === 0) {
        if (i % 2 === 0) evenZeros += 1;
        else oddZeros += 1;
      }
    }
    return evenZeros > oddZeros
      ? { text: decodeUtf16Be(bytes), encoding: 'Utf16' }
      : { text: new TextDecoder('utf-16le').decode(bytes), encoding: 'Utf16' };
  }
  return { text: new TextDecoder('gbk').decode(bytes), encoding: 'Gbk' };
}

function splitDelimitedLine(line: string, delimiter: string): string[] {
  const fields: string[] = [];
  let current = '';
  let quoted = false;
  for (let index = 0; index < line.length; index += 1) {
    const char = line[index]!;
    if (quoted) {
      if (char === '"') {
        if (line[index + 1] === '"') {
          current += '"';
          index += 1;
        } else {
          quoted = false;
        }
      } else {
        current += char;
      }
    } else if (char === '"') {
      quoted = true;
    } else if (char === delimiter) {
      fields.push(current.trim());
      current = '';
    } else {
      current += char;
    }
  }
  if (quoted) throw new Error('unterminated quoted field');
  fields.push(current.trim());
  return fields;
}

export function parseDelimitedTable(bytes: Uint8Array, fileName: string): DelimitedTable {
  const delimiter = fileName.toLowerCase().endsWith('.tsv') ? '\t' : ',';
  const decoded = decodeDatasetText(bytes);
  const text = decoded.text.replace(/^\uFEFF/, '');
  const lines = text
    .split(/\r\n|\n|\r/)
    .filter((line, index, all) => line.length > 0 || index !== all.length - 1);
  if (lines.length === 0) throw new Error('empty dataset');
  const headers = splitDelimitedLine(lines[0]!, delimiter);
  if (headers.length === 0 || headers.every((header) => header.length === 0)) {
    throw new Error('dataset has no columns');
  }
  return {
    headers,
    rows: lines.slice(1).map((line) => splitDelimitedLine(line, delimiter)),
    encoding: decoded.encoding,
  };
}
