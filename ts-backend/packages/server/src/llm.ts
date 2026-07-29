// server/llm.ts — LLM provider abstraction + config service (tasks 15.1, 15.2, 15.3).
// Transcribed from crates/agent-server/src/handlers/llm_config.rs and extended
// with the OAuth/PKCE gate (Requirement 13.4/13.5).
//
// The abstraction supports configuration of provider, API key, base URL, and
// model (Requirement 13.1). GET /api/llm-status reports status WITHOUT exposing
// the stored key (13.2); POST /api/llm-config persists config for subsequent
// sessions after a connectivity probe (13.3). OAuth is initiated ONLY for
// providers that require it, and configuring an OAuth-required provider while
// the flow is unavailable is rejected with an error (13.4/13.5). Streamed LLM
// responses are relayed as SSE frames (13.6) via the shared SSE serializer.

import { createHash, randomBytes } from 'node:crypto';
import type { LlmConfig, LlmConfigStore, LlmProbe } from './state.js';

export type LlmProvider = LlmConfig['provider'];

export interface LlmStatus {
  configured: boolean;
  provider: LlmProvider | null;
  base_url: string | null;
  model: string | null;
}

/**
 * Pure status projection (Requirement 13.2): a config with a non-empty api_key
 * is "configured"; the key value is never included in the result.
 */
export function statusFromConfig(config: LlmConfig | null): LlmStatus {
  if (config && config.api_key.length > 0) {
    return {
      configured: true,
      provider: config.provider,
      base_url: config.base_url ?? null,
      model: config.model ?? null,
    };
  }
  return { configured: false, provider: null, base_url: null, model: null };
}

// ---------------------------------------------------------------------------
// OAuth / PKCE (Requirement 13.4, 13.5)
// ---------------------------------------------------------------------------

/**
 * Providers that REQUIRE OAuth (vs API-key). The current provider set
 * (deepseek, qwen, kimi, zhipu, custom) authenticates by API key, so this set
 * is empty; the machinery is in place so adding an OAuth-only provider is a
 * one-line change.
 */
export const OAUTH_REQUIRED_PROVIDERS: ReadonlySet<string> = new Set<string>();

export function providerRequiresOAuth(provider: string): boolean {
  return OAUTH_REQUIRED_PROVIDERS.has(provider);
}

export interface PkcePair {
  /** High-entropy verifier (RFC 7636); kept server-side. */
  codeVerifier: string;
  /** S256 challenge sent in the authorization request. */
  codeChallenge: string;
  codeChallengeMethod: 'S256';
}

function base64Url(buf: Buffer): string {
  return buf.toString('base64').replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}

/** Generate a PKCE verifier/challenge pair (RFC 7636, S256). */
export function generatePkcePair(): PkcePair {
  const codeVerifier = base64Url(randomBytes(32));
  const codeChallenge = base64Url(createHash('sha256').update(codeVerifier).digest());
  return { codeVerifier, codeChallenge, codeChallengeMethod: 'S256' };
}

/** Capability flag: whether the backend can actually drive an OAuth flow. */
export interface OAuthCapability {
  available: boolean;
}

export class LlmConfigError extends Error {
  constructor(
    public readonly code: 'OAUTH_UNAVAILABLE' | 'LLM_PROBE_FAILED' | 'INVALID_CONFIG',
    message: string,
  ) {
    super(message);
    this.name = 'LlmConfigError';
  }
}

// ---------------------------------------------------------------------------
// test_and_save service (Requirement 13.3, 13.5)
// ---------------------------------------------------------------------------

export interface SaveConfigInput {
  provider: LlmProvider;
  apiKey: string;
  baseUrl?: string | null;
  model?: string | null;
}

/**
 * Probe connectivity then persist the config. Rejects an OAuth-required
 * provider when the OAuth flow is unavailable (Requirement 13.5) BEFORE any
 * probe or write. Throws LlmConfigError on rejection/probe failure.
 */
export async function testAndSaveConfig(
  probe: LlmProbe,
  store: LlmConfigStore,
  input: SaveConfigInput,
  oauth: OAuthCapability = { available: false },
): Promise<void> {
  if (input.provider === 'custom' && (!input.baseUrl || input.baseUrl.trim().length === 0)) {
    throw new LlmConfigError('INVALID_CONFIG', '自定义 provider 需要 Base URL');
  }
  if (providerRequiresOAuth(input.provider) && !oauth.available) {
    throw new LlmConfigError(
      'OAUTH_UNAVAILABLE',
      `provider '${input.provider}' requires OAuth, which is not available in this build`,
    );
  }
  try {
    await probe.probe(
      input.provider,
      input.apiKey,
      input.baseUrl ?? undefined,
      input.model ?? undefined,
    );
  } catch (err) {
    throw new LlmConfigError('LLM_PROBE_FAILED', (err as Error).message);
  }
  store.write({
    provider: input.provider,
    api_key: input.apiKey,
    base_url: input.baseUrl ?? null,
    model: input.model ?? null,
  });
}
