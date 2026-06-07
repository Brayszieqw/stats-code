// tests/property/redaction.property.test.ts — Properties 4 & 5 (tasks 13.11, 13.12).
//
// Property 4 (Redaction soundness): for ALL inputs, no configured secret and no
// out-of-cwd absolute path survives in the redacted output; the marker tokens
// appear where they were (Requirements 4.1-4.4).
//
// Property 5 (Redaction idempotence): redact(redact(s)) === redact(s) for all
// inputs and policies, and redaction never introduces CR characters
// (Requirements 4.5, 4.6).
//
// Validates: Requirements 4.1, 4.2, 4.3, 4.4, 4.5, 4.6

import { describe, it, expect } from 'vitest';
import fc from 'fast-check';
import { redactPure, redactionPolicy, REDACTED, EXTERNAL } from '@stats-code/engine';

// Secrets: non-empty, no whitespace, unlikely to be substrings of the marker.
const secretArb = fc.stringMatching(/^[A-Za-z0-9_-]{4,24}$/).filter((s) => s !== REDACTED && s !== EXTERNAL);
const secretsArb = fc.array(secretArb, { minLength: 0, maxLength: 4 });

// Free text plus interleaved tokens that may include secrets and abs paths.
const wordArb = fc.constantFrom('alpha', 'beta', 'value=', 'path', 'data.csv', '中文', './rel', '\n', ' ');
const textArb = fc.array(wordArb, { maxLength: 30 }).map((ws) => ws.join(' '));

const wdArb = fc.option(fc.constantFrom('/home/alice/proj', 'C:\\proj', '/srv/run'), { nil: undefined });

describe('Property 4: redaction soundness (Requirements 4.1-4.4)', () => {
  it('no configured secret survives the redacted output', () => {
    fc.assert(
      fc.property(textArb, secretsArb, wdArb, (text, secrets, wd) => {
        // Inject the secrets into the text so there is something to remove.
        const withSecrets = secrets.length > 0 ? `${text} ${secrets.join(' ')} tail` : text;
        const policy = redactionPolicy({ secrets, workingDirectory: wd });
        const out = redactPure(withSecrets, policy);
        for (const s of secrets) {
          if (s.length === 0) continue;
          expect(out.includes(s)).toBe(false);
        }
      }),
      { numRuns: 400 },
    );
  });

  it('out-of-cwd absolute paths are replaced by the external marker', () => {
    fc.assert(
      fc.property(
        fc.constantFrom('/home/eve/leak.csv', '/Users/bob/secret.txt', '/var/data/x.bin'),
        (extPath) => {
          const policy = redactionPolicy({ workingDirectory: '/home/alice/proj' });
          const out = redactPure(`opened ${extPath} done`, policy);
          expect(out).toContain(EXTERNAL);
          expect(out.includes(extPath)).toBe(false);
        },
      ),
      { numRuns: 100 },
    );
  });

  it('paths inside cwd render as relative forward-slash form (no leak of the prefix)', () => {
    const policy = redactionPolicy({ workingDirectory: '/home/alice/proj' });
    fc.assert(
      fc.property(fc.stringMatching(/^[a-z]{1,8}(\/[a-z]{1,8}){0,3}\.csv$/), (rel) => {
        const out = redactPure(`loaded /home/alice/proj/${rel}`, policy);
        expect(out).toBe(`loaded ${rel}`);
        expect(out.includes('/home/alice/proj')).toBe(false);
      }),
      { numRuns: 150 },
    );
  });
});

describe('Property 5: redaction idempotence (Requirements 4.5, 4.6)', () => {
  it('redact(redact(s)) === redact(s) for arbitrary inputs and policies', () => {
    fc.assert(
      fc.property(textArb, secretsArb, wdArb, (text, secrets, wd) => {
        const withStuff = `${text} ${secrets.join(' ')} /home/eve/x.csv C:\\Users\\bob\\y.csv`;
        const policy = redactionPolicy({ secrets, workingDirectory: wd });
        const once = redactPure(withStuff, policy);
        const twice = redactPure(once, policy);
        expect(twice).toBe(once);
      }),
      { numRuns: 400 },
    );
  });

  it('redaction never introduces CR characters', () => {
    fc.assert(
      fc.property(textArb, secretsArb, wdArb, (text, secrets, wd) => {
        const policy = redactionPolicy({ secrets, workingDirectory: wd });
        const out = redactPure(text, policy);
        expect(out.includes('\r')).toBe(false);
      }),
      { numRuns: 200 },
    );
  });

  it('empty policy is the identity function', () => {
    fc.assert(
      fc.property(textArb, (text) => {
        expect(redactPure(text, redactionPolicy())).toBe(text);
      }),
      { numRuns: 100 },
    );
  });
});
