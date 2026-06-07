import { describe, it, expect } from 'vitest';
import { redactPure, redactionPolicy } from '@stats-code/engine';

describe('redactor — secret substitution', () => {
  it('empty policy returns input verbatim', () => {
    const input = 'hello world\nline 2\n';
    expect(redactPure(input, redactionPolicy())).toBe(input);
  });

  it('single secret is replaced with the redacted marker', () => {
    expect(redactPure('api_key=ABC123', redactionPolicy({ secrets: ['ABC123'] }))).toBe(
      'api_key=<redacted>',
    );
  });

  it('multiple non-overlapping secrets are each replaced', () => {
    const p = redactionPolicy({ secrets: ['sk-aaa', 'sk-bbb'] });
    expect(redactPure('first=sk-aaa,second=sk-bbb,third=sk-aaa', p)).toBe(
      'first=<redacted>,second=<redacted>,third=<redacted>',
    );
  });

  it('overlapping secrets are redacted longest-first', () => {
    const p = redactionPolicy({ secrets: ['AB', 'ABCD'] });
    expect(redactPure('xABCDy', p)).toBe('x<redacted>y');
  });

  it('empty-string secrets are silently dropped', () => {
    const p = redactionPolicy({ secrets: ['', 'sk-real'] });
    expect(redactPure('value=sk-real,trail', p)).toBe('value=<redacted>,trail');
  });

  it('multibyte text is not bisected by an ASCII secret', () => {
    const p = redactionPolicy({ secrets: ['ABC123'] });
    expect(redactPure('中文 ABC123 中文', p)).toBe('中文 <redacted> 中文');
  });
});

describe('redactor — path classification', () => {
  it('windows path outside cwd becomes external', () => {
    const p = redactionPolicy({ workingDirectory: 'D:\\proj' });
    const out = redactPure('loaded C:\\Users\\alice\\data.csv', p);
    expect(out).toContain('<external>');
    expect(out).not.toContain('C:\\Users\\alice');
  });

  it('unix path outside cwd becomes external', () => {
    const p = redactionPolicy({ workingDirectory: '/proj' });
    expect(redactPure('loaded /home/alice/data.csv', p)).toBe('loaded <external>');
  });

  it('unix path inside cwd renders relative forward-slash form', () => {
    const p = redactionPolicy({ workingDirectory: '/home/alice/proj' });
    expect(redactPure('loaded /home/alice/proj/inputs/data.csv', p)).toBe('loaded inputs/data.csv');
  });

  it('windows path inside cwd renders relative forward-slash form', () => {
    const p = redactionPolicy({ workingDirectory: 'C:\\proj' });
    const out = redactPure('loaded C:\\proj\\subdir\\data.csv', p);
    expect(out).toBe('loaded subdir/data.csv');
    expect(out).not.toContain('\\');
  });

  it('drive-letter case difference still matches cwd', () => {
    const p = redactionPolicy({ workingDirectory: 'C:\\proj' });
    expect(redactPure('opened c:/proj/data.csv', p)).toBe('opened data.csv');
  });

  it('no cwd marks every detected path external', () => {
    const p = redactionPolicy();
    expect(redactPure('a=/home/alice/x.csv b=C:\\Users\\bob\\y.csv', p)).toBe('a=<external> b=<external>');
  });

  it('URL containing /home/ is not misclassified', () => {
    const p = redactionPolicy({ workingDirectory: '/anywhere' });
    const input = 'see https://example.com/home/alice/data';
    expect(redactPure(input, p)).toBe(input);
  });

  it('relative paths are left alone', () => {
    const p = redactionPolicy({ workingDirectory: '/proj' });
    const input = 'see ./data.csv and ../parent/x.txt';
    expect(redactPure(input, p)).toBe(input);
  });

  it('projector directory is not treated as inside proj (segment boundary)', () => {
    const p = redactionPolicy({ workingDirectory: '/home/alice/proj' });
    expect(redactPure('loaded /home/alice/projector/x.csv', p)).toBe('loaded <external>');
  });

  it('detection at start and end of string works', () => {
    expect(redactPure('/home/alice/data.csv', redactionPolicy())).toBe('<external>');
    expect(redactPure('tail /var/log/x.log', redactionPolicy())).toBe('tail <external>');
  });
});

describe('redactor — structural properties', () => {
  it('is idempotent', () => {
    const p = redactionPolicy({ secrets: ['sk-XYZ'], workingDirectory: '/home/alice/proj' });
    const input = 'key=sk-XYZ outside=/Users/eve/leak.csv inside=/home/alice/proj/data.csv';
    const once = redactPure(input, p);
    expect(redactPure(once, p)).toBe(once);
  });

  it('preserves LF endings and introduces no CR', () => {
    const p = redactionPolicy({ secrets: ['sk-aaa'] });
    const input = 'line1\nkey=sk-aaa\nline3\n';
    const out = redactPure(input, p);
    expect(out).not.toContain('\r');
    expect(out).toBe('line1\nkey=<redacted>\nline3\n');
  });

  it('runs secret substitution before path classification', () => {
    const p = redactionPolicy({ secrets: ['/home/alice/proj/secret.txt'] });
    expect(redactPure('leaked=/home/alice/proj/secret.txt rest', p)).toBe('leaked=<redacted> rest');
  });

  it('classifies multiple paths in one input', () => {
    const p = redactionPolicy({ workingDirectory: '/home/alice/proj' });
    const input = 'in=/home/alice/proj/a.csv ext=/Users/bob/b.csv also=/home/alice/proj/sub/c.txt';
    expect(redactPure(input, p)).toBe('in=a.csv ext=<external> also=sub/c.txt');
  });
});
