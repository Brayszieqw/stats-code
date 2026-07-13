/** A single parser shared by upload summaries, audits, and deterministic runs. */
export interface DelimitedTable {
  headers: string[];
  rows: string[][];
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
  const text = new TextDecoder('utf-8').decode(bytes).replace(/^\uFEFF/, '');
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
  };
}
