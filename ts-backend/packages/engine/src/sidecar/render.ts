// sidecar/render.ts — pure template renderer (Phase 6, task 13.3).
// Transcribed from crates/stats-code/src/sidecar/render.rs.
//
// Placeholder grammar (closed set):
//   {{dataset.sha256}}    → the 64-hex SHA256
//   {{release.version}}   → the release version
//   {{column.<i>.name}}   → columns[i].name
//   {{column.<i>.dtype}}  → columns[i].dtype token
//   {{params.<key>}}      → params[key]
// Whitespace inside braces is strict (not trimmed); the first `{{` opens and
// the next `}}` closes. Unknown/malformed placeholders throw RenderError.

export type ColumnDtype = 'numeric' | 'categorical' | 'date' | 'string';

export interface Column {
  name: string;
  dtype: ColumnDtype;
}

export type RenderParams = Readonly<Record<string, string>>;

export class RenderError extends Error {
  constructor(
    public readonly kind: 'unknown_placeholder' | 'malformed_placeholder' | 'column_index_out_of_range' | 'unknown_param',
    message: string,
  ) {
    super(message);
    this.name = 'RenderError';
  }
}

function substitute(
  inner: string,
  params: RenderParams,
  columns: readonly Column[],
  datasetSha256: string,
  releaseVersion: string,
): string {
  if (inner === 'dataset.sha256') return datasetSha256;
  if (inner === 'release.version') return releaseVersion;

  const colMatch = /^column\.(\d+)\.(name|dtype)$/.exec(inner);
  if (colMatch) {
    const index = Number.parseInt(colMatch[1]!, 10);
    if (index >= columns.length) {
      throw new RenderError(
        'column_index_out_of_range',
        `column index ${index} is out of range (have ${columns.length} columns)`,
      );
    }
    const col = columns[index]!;
    return colMatch[2] === 'name' ? col.name : col.dtype;
  }

  const paramMatch = /^params\.(.+)$/.exec(inner);
  if (paramMatch) {
    const key = paramMatch[1]!;
    const value = params[key];
    if (value === undefined) {
      throw new RenderError('unknown_param', `unknown param key: ${key}`);
    }
    return value;
  }

  throw new RenderError('unknown_placeholder', `unknown placeholder: {{${inner}}}`);
}

/** Render a template against the supplied inputs. Pure and deterministic. */
export function renderPure(
  template: string,
  params: RenderParams,
  columns: readonly Column[],
  datasetSha256: string,
  releaseVersion: string,
): string {
  let out = '';
  let i = 0;
  while (i < template.length) {
    const open = template.indexOf('{{', i);
    if (open === -1) {
      out += template.slice(i);
      break;
    }
    out += template.slice(i, open);
    const close = template.indexOf('}}', open + 2);
    if (close === -1) {
      throw new RenderError(
        'malformed_placeholder',
        `malformed placeholder fragment: ${JSON.stringify(template.slice(open))}`,
      );
    }
    const inner = template.slice(open + 2, close);
    if (inner.length === 0) {
      throw new RenderError('malformed_placeholder', 'malformed placeholder fragment: "{{}}"');
    }
    out += substitute(inner, params, columns, datasetSha256, releaseVersion);
    i = close + 2;
  }
  return out;
}

const DTYPE_TOKENS: readonly ColumnDtype[] = ['numeric', 'categorical', 'date', 'string'];

/** Build the language-agnostic sidecar header. Transcribed from header.rs. */
export function formatHeader(columns: readonly Column[], datasetSha256: string, releaseVersion: string): string {
  let out = '';
  out += '# stats-code sidecar snippet\n';
  out += `# release: ${releaseVersion}\n`;
  out += `# dataset_sha256: ${datasetSha256}\n`;
  out += '# data: data.csv\n';
  columns.forEach((column, i) => {
    out += `# column.${i}.name: ${column.name}\n`;
    out += `# column.${i}.dtype: ${column.dtype}\n`;
  });
  return out;
}

export { DTYPE_TOKENS };
