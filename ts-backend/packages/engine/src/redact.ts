// redact.ts — shared secret + path redactor (Phase 6, task 13.2).
// Transcribed from crates/stats-code/src/redact.rs.
//
// `redact` is a pure, idempotent, side-effect-free string rewriter used by both
// the Sidecar Code Generator and the Audit Snapshot Exporter. Two passes:
//   1. secret substitution (longest-match-first → "<redacted>");
//   2. absolute-path classification (inside cwd → relative forward-slash form;
//      outside cwd or no cwd → "<external>").
//
// Purity contract: no clock, env, randomness, fs, or locks. Equal inputs on
// any host/time produce byte-identical output. Idempotent: redact(redact(s))
// == redact(s).
//
// Path detection is conservative and operates on JS string code units. All
// path-content characters are ASCII; non-ASCII characters (the JS analogue of
// multi-byte UTF-8) are never path-content, so they act as boundaries exactly
// like the Rust byte scanner.

export const REDACTED = '<redacted>';
export const EXTERNAL = '<external>';

export interface RedactionPolicy {
  /** Secrets replaced by REDACTED on every occurrence. Empty strings ignored. */
  secrets: readonly string[];
  /** Working directory for path classification; undefined → every path external. */
  workingDirectory?: string;
}

/** Builder-style helper mirroring RedactionPolicy::new().with_secrets(...). */
export function redactionPolicy(opts: {
  secrets?: readonly string[];
  workingDirectory?: string;
} = {}): RedactionPolicy {
  return {
    secrets: (opts.secrets ?? []).filter((s) => s.length > 0),
    workingDirectory: opts.workingDirectory,
  };
}

const UNIX_PREFIXES = ['/Users/', '/home/', '/root/', '/private/', '/tmp/', '/var/'];

function isPathContentChar(ch: string): boolean {
  // ASCII alphanumerics plus \ / . _ - ~
  return /^[A-Za-z0-9\\/._~-]$/.test(ch);
}

/** Replace `\` with `/` and lowercase a leading drive letter for comparison. */
function normalizePathForCompare(s: string): string {
  const hasDrive = s.length >= 2 && /[A-Za-z]/.test(s[0]!) && s[1] === ':';
  let out = '';
  for (let i = 0; i < s.length; i += 1) {
    let ch = s[i] === '\\' ? '/' : s[i]!;
    if (i === 0 && hasDrive) {
      ch = ch.toLowerCase();
    }
    out += ch;
  }
  return out;
}

function trimTrailingSlash(s: string): string {
  return s.length > 1 && s.endsWith('/') ? s.slice(0, -1) : s;
}

/** Return the suffix of `path` after `prefix` only on a `/`-segment boundary. */
function stripPathPrefix(path: string, prefix: string): string | null {
  if (!path.startsWith(prefix)) {
    return null;
  }
  const rest = path.slice(prefix.length);
  if (rest === '') {
    return '';
  }
  if (rest.startsWith('/')) {
    return rest;
  }
  return null;
}

/** Match an absolute-path run starting at index `i`; return end index or null. */
function matchAbsolutePathRun(text: string, i: number): number | null {
  // Windows drive-letter: [A-Za-z]:[\\/]<path bytes>
  if (
    i + 3 <= text.length &&
    /[A-Za-z]/.test(text[i]!) &&
    text[i + 1] === ':' &&
    (text[i + 2] === '\\' || text[i + 2] === '/')
  ) {
    let j = i + 3;
    while (j < text.length && isPathContentChar(text[j]!)) {
      j += 1;
    }
    return j;
  }

  // Unix absolute paths anchored at a closed set of prefixes.
  for (const prefix of UNIX_PREFIXES) {
    if (text.startsWith(prefix, i)) {
      let j = i + prefix.length;
      while (j < text.length && isPathContentChar(text[j]!)) {
        j += 1;
      }
      return j;
    }
  }
  return null;
}

function classifyDetectedPath(detected: string, workingDirectory: string | undefined): string {
  if (workingDirectory !== undefined) {
    const normalizedWd = trimTrailingSlash(normalizePathForCompare(workingDirectory));
    const normalizedPath = normalizePathForCompare(detected);
    const rest = stripPathPrefix(normalizedPath, normalizedWd);
    if (rest !== null) {
      return rest.replace(/^\/+/, '');
    }
  }
  return EXTERNAL;
}

function classifyPaths(text: string, workingDirectory: string | undefined): string {
  let out = '';
  let cursor = 0;
  let lastCopy = 0;

  while (cursor < text.length) {
    const atBoundary = cursor === 0 || !isPathContentChar(text[cursor - 1]!);
    if (atBoundary) {
      const end = matchAbsolutePathRun(text, cursor);
      if (end !== null) {
        out += text.slice(lastCopy, cursor);
        out += classifyDetectedPath(text.slice(cursor, end), workingDirectory);
        cursor = end;
        lastCopy = end;
        continue;
      }
    }
    cursor += 1;
  }
  out += text.slice(lastCopy);
  return out;
}

/** Rewrite `text` according to `policy`. Pure and idempotent. */
export function redactPure(text: string, policy: RedactionPolicy): string {
  // Pass 1: secret substitution, longest-first.
  let pass1 = text;
  const secrets = policy.secrets.filter((s) => s.length > 0);
  if (secrets.length > 0) {
    const sorted = [...secrets].sort((a, b) => b.length - a.length);
    for (const needle of sorted) {
      if (pass1.includes(needle)) {
        pass1 = pass1.split(needle).join(REDACTED);
      }
    }
  }

  // Pass 2: path classification.
  return classifyPaths(pass1, policy.workingDirectory);
}

/** Convenience overload matching the original engine placeholder signature. */
export function redact(text: string, policy: RedactionPolicy): string {
  return redactPure(text, policy);
}
