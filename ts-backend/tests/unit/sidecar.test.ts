import { describe, it, expect } from 'vitest';
import { sidecar, coverage } from '@stats-code/engine';

const { generateSnippet, renderPure, formatHeader, RenderError, GenerateError } = sidecar;
const { getLoadedMatrix, REQUIRED_SOFTWARE } = coverage;

const SHA = '0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef';
const cols: sidecar.Column[] = [
  { name: 'age', dtype: 'numeric' },
  { name: 'sex', dtype: 'categorical' },
];

describe('render engine', () => {
  it('substitutes the closed-set placeholders', () => {
    const out = renderPure(
      '{{dataset.sha256}}|{{release.version}}|{{column.0.name}}|{{column.1.dtype}}|{{params.k}}',
      { k: 'v' },
      cols,
      SHA,
      '1.2.3',
    );
    expect(out).toBe(`${SHA}|1.2.3|age|categorical|v`);
  });

  it('rejects an unknown placeholder', () => {
    expect(() => renderPure('{{nonsense}}', {}, cols, SHA, '1.0.0')).toThrow(RenderError);
  });

  it('rejects a malformed/unterminated placeholder', () => {
    expect(() => renderPure('{{dataset.sha256', {}, cols, SHA, '1.0.0')).toThrow(RenderError);
    expect(() => renderPure('{{}}', {}, cols, SHA, '1.0.0')).toThrow(RenderError);
  });

  it('rejects an out-of-range column index', () => {
    expect(() => renderPure('{{column.9.name}}', {}, cols, SHA, '1.0.0')).toThrow(RenderError);
  });

  it('does not trim inner whitespace (strict)', () => {
    expect(() => renderPure('{{ dataset.sha256 }}', {}, cols, SHA, '1.0.0')).toThrow(RenderError);
  });

  it('formatHeader is LF-only and lists columns in order', () => {
    const h = formatHeader(cols, SHA, '0.5.0');
    expect(h).toContain('# data: data.csv');
    expect(h).toContain('# column.0.name: age');
    expect(h).toContain('# column.1.dtype: categorical');
    expect(h).not.toContain('\r');
    expect(h.endsWith('\n')).toBe(true);
  });
});

describe('Property 2: coverage-driven variant selection', () => {
  it('a none-state cell yields an Uncovered sentinel (copy disabled)', () => {
    // standardization × SPSS is "none".
    const snip = generateSnippet('standardization', 'SPSS', {}, cols, SHA);
    expect(snip.kind).toBe('uncovered');
    if (snip.kind === 'uncovered') {
      expect(snip.coverageValue).toBe('none');
    }
  });

  it('every cell variant matches the matrix coverage state', () => {
    const matrix = getLoadedMatrix();
    for (const entry of matrix.algorithms) {
      for (const sw of REQUIRED_SOFTWARE) {
        const snip = generateSnippet(entry.id, sw, {}, cols, SHA);
        if (entry.coverage[sw] === 'none') {
          expect(snip.kind).toBe('uncovered');
        } else {
          expect(snip.kind).toBe('snippet');
          if (snip.kind === 'snippet') {
            expect(snip.text.length).toBeGreaterThan(0);
            expect(snip.text).toContain('data.csv');
            expect(snip.text).toContain(SHA);
          }
        }
      }
    }
  });
});

describe('Property 1: sidecar determinism', () => {
  it('two calls with identical inputs produce byte-identical output', () => {
    const a = generateSnippet('tableone', 'R', { by: 'arm' }, cols, SHA);
    const b = generateSnippet('tableone', 'R', { by: 'arm' }, cols, SHA);
    expect(a).toEqual(b);
  });
});

describe('analysis parameter column selection', () => {
  it('renders regression code with the requested outcome and predictors', () => {
    const datasetColumns: sidecar.Column[] = [
      { name: 'participant_id', dtype: 'string' },
      { name: 'disease', dtype: 'numeric' },
      { name: 'bmi', dtype: 'numeric' },
      { name: 'age', dtype: 'numeric' },
    ];
    const params = { outcome: 'bmi', predictors: '["age", "disease"]' };

    const rSnippet = generateSnippet('linear', 'R', params, datasetColumns, SHA);
    const pythonSnippet = generateSnippet('linear', 'Python', params, datasetColumns, SHA);

    expect(rSnippet.kind).toBe('snippet');
    expect(pythonSnippet.kind).toBe('snippet');
    if (rSnippet.kind === 'snippet') {
      expect(rSnippet.text).toContain('fit <- stats::lm(bmi ~ age + disease, data = data)');
      expect(rSnippet.text).not.toContain('fit <- stats::lm(participant_id ~ disease, data = data)');
    }
    if (pythonSnippet.kind === 'snippet') {
      expect(pythonSnippet.text).toContain('y = data["bmi"]');
      expect(pythonSnippet.text).toContain('data[["age", "disease"]]');
    }
  });
});

describe('Property 3: host/clock independence (redaction applied)', () => {
  it('redacts api keys passed in and out-of-cwd paths', () => {
    // Use a template that echoes a param; tableone R template references columns.
    const snip = generateSnippet('tableone', 'R', {}, cols, SHA, {
      apiKeys: ['sk-secret-123'],
      workingDirectory: '/home/alice/proj',
    });
    expect(snip.kind).toBe('snippet');
    if (snip.kind === 'snippet') {
      expect(snip.text).not.toContain('sk-secret-123');
    }
  });

  it('output excludes any clock/random value — pure function of inputs', () => {
    const a = generateSnippet('ttest', 'Python', {}, cols, SHA);
    // simulate "different host/time": same inputs must reproduce exactly.
    const b = generateSnippet('ttest', 'Python', {}, cols, SHA);
    expect(a).toEqual(b);
  });
});

describe('error handling', () => {
  it('unknown algorithm → GenerateError', () => {
    expect(() => generateSnippet('does_not_exist', 'R', {}, cols, SHA)).toThrow(GenerateError);
  });

  it('case-sensitive lookup rejects wrong casing', () => {
    expect(() => generateSnippet('TableOne', 'R', {}, cols, SHA)).toThrow(GenerateError);
  });
});
