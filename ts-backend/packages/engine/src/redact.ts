// redact.ts — shared secret + path redactor (Phase 6, task 13.2).
// Pure and idempotent: output depends only on input text and policy.

export interface RedactionPolicy {
  secrets: readonly string[];
  cwd: string;
}

/** Placeholder; implemented in task 13.2. */
export function redact(text: string, _policy: RedactionPolicy): string {
  return text;
}
