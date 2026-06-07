// snapshot/llm_provenance.ts — `llm_provenance.json` builder (task 13.4).
// Transcribed 1:1 from crates/stats-code/src/snapshot/llm_provenance.rs.
//
// `buildLlmProvenance` is a PURE function: no clock, no environment, no random
// seeds. The struct NEVER embeds an LLM API key — the API-key check is owned by
// the Redactor and runs on every text artifact before it is written. The two
// SHA256 fields are 64-char lowercase hex digests, never plaintext.
//
// _Requirements: 6.7_

export const LLM_PROVENANCE_SCHEMA_VERSION = 1;

export interface LlmCall {
  provider: string;
  model: string;
  request_at_utc: string;
  /** 64-char lowercase hex SHA256 of the prompt bytes. */
  prompt_sha256: string;
  /** 64-char lowercase hex SHA256 of the response bytes. */
  response_sha256: string;
}

export interface LlmProvenance {
  schema_version: number;
  calls: LlmCall[];
}

/**
 * Build the `llm_provenance.json` payload. Order of `calls` is preserved as
 * given; the builder does not sort. An empty input yields an empty `calls`
 * array (the byte-stable "no LLM calls" representation).
 */
export function buildLlmProvenance(calls: readonly LlmCall[]): LlmProvenance {
  return {
    schema_version: LLM_PROVENANCE_SCHEMA_VERSION,
    calls: calls.map((c) => ({
      provider: c.provider,
      model: c.model,
      request_at_utc: c.request_at_utc,
      prompt_sha256: c.prompt_sha256,
      response_sha256: c.response_sha256,
    })),
  };
}
