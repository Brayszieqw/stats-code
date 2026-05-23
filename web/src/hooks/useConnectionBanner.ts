/**
 * useConnectionBanner — glue between the chat hook (`useSseChat`) and the
 * Connection_Banner state machine (`bannerReducer`).
 *
 * Responsibilities:
 *   1. Observe chat-time errors and dispatch `Error` events into the
 *      reducer when an LLM upstream failure occurs (Requirement 12.1).
 *   2. Observe successful streams and dispatch `Success` to hide the
 *      banner (Requirement 12.5).
 *   3. Provide `handleRetry` that resends the last user message
 *      (Requirement 12.3).
 *   4. Provide `handleEditKey` that drives the Onboarding_Card back on
 *      screen via `useLlmStatus.requireReconfigure` (Requirement 12.4).
 *
 * The hook is intentionally decoupled from `useSseChat` itself: the chat
 * hook stays untouched and we observe its outputs through arguments. This
 * keeps task 11.7 a pure additive change — no existing UI surface is
 * modified.
 *
 * Validates: Requirements 12.1, 12.2, 12.3, 12.4, 12.5
 */

import { useCallback, useEffect, useReducer, useRef } from 'react';
import {
  bannerReducer,
  initialBannerState,
  type BannerView,
} from '../lib/bannerReducer';
import type {
  ConnectionStatus,
  UseSseChatReturn,
} from './useSseChat';
import type { ErrorPayload, LlmProvider } from '../api/types';

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/**
 * Subset of `useSseChat`'s return value that this hook needs to observe.
 * Accepting the smaller shape keeps the test surface small and avoids
 * forcing every caller to pass the full chat-hook API.
 */
export interface ChatLike {
  status: ConnectionStatus;
  error: ErrorPayload | null;
  sendMessage: UseSseChatReturn['sendMessage'];
}

/**
 * Subset of `useLlmStatus` that the banner needs in order to drive the
 * "修改 Key" recovery path (Requirement 12.4).
 */
export interface LlmStatusLike {
  provider: LlmProvider | null;
  requireReconfigure: () => void;
  setRuntimeError: (err: import('../api/types').LlmRuntimeError | null) => void;
  clearRuntimeError: () => void;
}

export interface UseConnectionBannerArgs {
  chat: ChatLike;
  llmStatus: LlmStatusLike;
  /**
   * The last message the user sent. The "重试" button will re-issue this
   * exact text (Requirement 12.3). When `null`, retry is a no-op.
   */
  lastUserMessage: string | null;
}

export interface UseConnectionBannerReturn {
  /** View model to feed into `<ConnectionBanner bannerView={...} />`. */
  bannerView: BannerView;
  /** Wire to `<ConnectionBanner onRetry={...} />`. */
  handleRetry: () => void;
  /** Wire to `<ConnectionBanner onEditKey={...} />`. */
  handleEditKey: () => void;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/**
 * Decide whether an `ErrorPayload` represents the kind of LLM upstream
 * failure that should surface the Connection_Banner.
 *
 * Per Requirement 12.1 ("网络错误、鉴权失败、上游 5xx 等"), and the design
 * doc's mapping `RuntimeLlmError → code: LLM_UPSTREAM_ERROR`, this is any
 * error code that signals the LLM call itself failed — currently
 * `LlmUnavailable` in the existing wire format, plus the design-doc
 * `LLM_UPSTREAM_ERROR` constant for forward compatibility.
 */
function isLlmUpstreamError(payload: ErrorPayload | null): boolean {
  if (!payload) return false;
  const code = payload.error_code as string;
  return code === 'LlmUnavailable' || code === 'LLM_UPSTREAM_ERROR';
}

function summarizeError(payload: ErrorPayload): string {
  // Use the backend's human-readable message when present; fall back to a
  // generic phrase so the banner is never blank.
  if (payload.message && payload.message.trim().length > 0) {
    return payload.message;
  }
  return '与 AI 服务通信失败';
}

// ---------------------------------------------------------------------------
// Hook
// ---------------------------------------------------------------------------

/**
 * Wire chat-hook outputs into the banner state machine and produce the
 * view model + action handlers consumed by `<ConnectionBanner>`.
 */
export function useConnectionBanner({
  chat,
  llmStatus,
  lastUserMessage,
}: UseConnectionBannerArgs): UseConnectionBannerReturn {
  const [state, dispatch] = useReducer(bannerReducer, initialBannerState);

  // Track which `error` reference we've already converted into a banner
  // event so we don't re-dispatch on every render.
  const lastErrorRef = useRef<ErrorPayload | null>(null);
  // Track whether we've ever seen an Error; we only dispatch Success when
  // the banner is currently visible (avoids spurious dispatches before any
  // failure has happened).
  const hasErroredRef = useRef(false);

  // ── Error → banner ─────────────────────────────────────────────────────
  useEffect(() => {
    const err = chat.error;
    if (err && err !== lastErrorRef.current && isLlmUpstreamError(err)) {
      lastErrorRef.current = err;
      hasErroredRef.current = true;
      dispatch({
        type: 'Error',
        provider: llmStatus.provider,
        summary: summarizeError(err),
      });
      llmStatus.setRuntimeError({
        provider: llmStatus.provider,
        summary: summarizeError(err),
        last_message_id: null,
      });
    } else if (!err && lastErrorRef.current) {
      // Error cleared by chat hook (e.g. new send attempt) — let the
      // Success transition below handle banner hiding once we observe a
      // successful stream.
      lastErrorRef.current = null;
    }
  }, [chat.error, llmStatus]);

  // ── Success → banner hide ──────────────────────────────────────────────
  // The chat hook transitions: idle → connecting → streaming → idle on a
  // successful round-trip. We dispatch `Success` once we see streaming
  // start with no current error, indicating the upstream LLM is reachable
  // again. This satisfies R12.5 ("一次后续 LLM 调用成功完成 → 隐藏 banner").
  useEffect(() => {
    if (
      hasErroredRef.current &&
      state.banner.visible &&
      chat.status === 'streaming' &&
      !chat.error
    ) {
      dispatch({ type: 'Success' });
      llmStatus.clearRuntimeError();
      hasErroredRef.current = false;
    }
  }, [chat.status, chat.error, state.banner.visible, llmStatus]);

  // ── Action handlers ────────────────────────────────────────────────────
  const handleRetry = useCallback(() => {
    if (!lastUserMessage) return;
    chat.sendMessage(lastUserMessage);
  }, [chat, lastUserMessage]);

  const handleEditKey = useCallback(() => {
    dispatch({ type: 'EditKey' });
    llmStatus.clearRuntimeError();
    llmStatus.requireReconfigure();
    hasErroredRef.current = false;
  }, [llmStatus]);

  return {
    bannerView: state.banner,
    handleRetry,
    handleEditKey,
  };
}
