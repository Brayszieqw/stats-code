/**
 * useLlmStatus — single source of truth for LLM configuration state in the UI.
 *
 * Responsibilities:
 *   - On mount, fetch `GET /api/llm-status` (Requirement 10.1) and seed the
 *     `configured` / `provider` flags. This drives the Onboarding_Card mask
 *     (Requirement 11.1).
 *   - Expose mutators that downstream components can call without re-querying
 *     the backend:
 *       * `setConfigured(provider)` — invoked by Onboarding_Card after a
 *         successful `POST /api/llm-config` (Requirement 11.4).
 *       * `requireReconfigure()` — invoked by Connection_Banner's "修改 Key"
 *         action to force `configured = false` and re-show the card
 *         (Requirement 12.4).
 *       * `setRuntimeError(err)` — invoked by chat hooks when an LLM call
 *         returns `error.code === "LLM_UPSTREAM_ERROR"` (Requirement 12.1).
 *       * `clearRuntimeError()` — invoked when a subsequent LLM call
 *         succeeds (Requirement 12.5).
 *       * `refresh()` — manual re-fetch for diagnostic flows.
 *
 * Validates: Requirements 10.1, 11.1
 */

import { useCallback, useEffect, useState } from 'react';
import { ApiError, getLlmStatus } from '../api/client';
import type { LlmProvider, LlmRuntimeError } from '../api/types';

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export type LlmStatusFetchState = 'loading' | 'ready' | 'error';

export interface UseLlmStatusReturn {
  /** True once `/api/llm-status` returned `configured: true`. */
  configured: boolean;
  /** Provider reported by the backend; null while unconfigured. */
  provider: LlmProvider | null;
  /** Most recent runtime LLM error, populated by chat hooks. */
  runtime_error: LlmRuntimeError | null;
  /** Configured base URL returned by the backend status api. */
  base_url: string | null;
  model: string | null;
  /** Providers with a cached API key on the backend (empty when absent). */
  cached_providers: LlmProvider[];
  /** Lifecycle of the initial fetch. */
  fetchState: LlmStatusFetchState;
  /** Backend-side fetch error, if the initial GET failed. */
  fetchError: string | null;

  // -------------------------------------------------------------------------
  // Mutators
  // -------------------------------------------------------------------------

  /** Mark the LLM as configured (invoked after a successful save). */
  setConfigured: (provider: LlmProvider, baseUrl?: string | null, model?: string | null) => void;
  /** Force `configured = false` to re-surface the Onboarding_Card. */
  requireReconfigure: () => void;
  /** Record a runtime LLM error to drive Connection_Banner. */
  setRuntimeError: (err: LlmRuntimeError | null) => void;
  /** Clear any pending runtime error (Requirement 12.5). */
  clearRuntimeError: () => void;
  /** Manually re-query `/api/llm-status`. */
  refresh: () => Promise<void>;
}

// ---------------------------------------------------------------------------
// Hook
// ---------------------------------------------------------------------------

export function useLlmStatus(): UseLlmStatusReturn {
  const [configured, setConfiguredState] = useState(false);
  const [provider, setProviderState] = useState<LlmProvider | null>(null);
  const [baseUrl, setBaseUrlState] = useState<string | null>(null);
  const [model, setModelState] = useState<string | null>(null);
  const [cachedProviders, setCachedProvidersState] = useState<LlmProvider[]>([]);
  const [runtimeError, setRuntimeErrorState] =
    useState<LlmRuntimeError | null>(null);
  const [fetchState, setFetchState] = useState<LlmStatusFetchState>('loading');
  const [fetchError, setFetchError] = useState<string | null>(null);

  const refresh = useCallback(async (): Promise<void> => {
    setFetchState('loading');
    setFetchError(null);
    try {
      const status = await getLlmStatus();
      setConfiguredState(status.configured);
      setProviderState(status.provider);
      setBaseUrlState(status.base_url ?? null);
      setModelState(status.model ?? null);
      setCachedProvidersState(status.cached_providers ?? []);
      setFetchState('ready');
    } catch (err) {
      // Network failure / 5xx — leave configured=false so the card still
      // surfaces; expose the error for diagnostic UI.
      setConfiguredState(false);
      setProviderState(null);
      setBaseUrlState(null);
      setModelState(null);
      setCachedProvidersState([]);
      setFetchError(
        err instanceof ApiError
          ? err.payload.message
          : err instanceof Error
            ? err.message
            : '获取 LLM 配置状态失败',
      );
      setFetchState('error');
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const setConfigured = useCallback((next: LlmProvider, nextUrl?: string | null, nextModel?: string | null) => {
    setConfiguredState(true);
    setProviderState(next);
    setBaseUrlState(nextUrl ?? null);
    setModelState(nextModel ?? null);
    setRuntimeErrorState(null);
    setFetchState('ready');
    setFetchError(null);
    // A successful save/activate always means the backend now has this
    // provider's key cached, even before the next /api/llm-status refresh.
    setCachedProvidersState((prev) => (prev.includes(next) ? prev : [...prev, next]));
  }, []);

  const requireReconfigure = useCallback(() => {
    setConfiguredState(false);
  }, []);

  const setRuntimeError = useCallback(
    (err: LlmRuntimeError | null) => {
      setRuntimeErrorState(err);
    },
    [],
  );

  const clearRuntimeError = useCallback(() => {
    setRuntimeErrorState(null);
  }, []);

  return {
    configured,
    provider,
    base_url: baseUrl,
    model,
    cached_providers: cachedProviders,
    runtime_error: runtimeError,
    fetchState,
    fetchError,
    setConfigured,
    requireReconfigure,
    setRuntimeError,
    clearRuntimeError,
    refresh,
  };
}
