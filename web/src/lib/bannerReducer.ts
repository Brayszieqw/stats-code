/**
 * Connection_Banner reducer — pure state machine for the runtime LLM-error
 * banner described by Requirement 12.
 *
 * Inputs (events):
 *   - Error        : an LLM call failed; banner should appear with provider + summary.
 *   - Success      : a subsequent LLM call succeeded; banner hides (R12.5).
 *   - RetrySuccess : the "重试" button's resend succeeded; banner hides (R12.3).
 *   - EditKey      : user clicked "修改 Key"; banner hides while Onboarding_Card
 *                    takes over (R12.4).
 *
 * Output:
 *   `{ banner: { visible, provider?, summary? } }` — `provider` / `summary` are
 *   only populated while `visible === true`.
 *
 * The reducer is a pure function: same `(state, event)` always yields the same
 * next state, no side effects, suitable for `useReducer` and for fast-check
 * property testing (see Task 11.6).
 *
 * Feature: single-command-launcher, Property 12:
 *   For any sequence [Error, ..., Success] the final state has
 *   `state.banner.visible === false`.
 *
 * Validates: Requirements 12.5
 */

import type { LlmProvider } from '../api/types';

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

/** A chat-time LLM call failed. The UI should surface the failure. */
export interface BannerErrorEvent {
  type: 'Error';
  /** Provider that failed; `null` when the failing call had no provider info. */
  provider: LlmProvider | null;
  /** Short, human-readable description of the failure (R12.1). */
  summary: string;
}

/** Any LLM call (not necessarily a retry) succeeded. */
export interface BannerSuccessEvent {
  type: 'Success';
}

/** The "重试" button's resend succeeded. */
export interface BannerRetrySuccessEvent {
  type: 'RetrySuccess';
}

/** The user clicked "修改 Key" — Onboarding_Card will replace the banner. */
export interface BannerEditKeyEvent {
  type: 'EditKey';
}

export type BannerEvent =
  | BannerErrorEvent
  | BannerSuccessEvent
  | BannerRetrySuccessEvent
  | BannerEditKeyEvent;

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

/**
 * Connection_Banner view model.
 *
 * Invariants:
 *   - `visible === true`  → `provider` and `summary` are defined.
 *   - `visible === false` → `provider` and `summary` are `undefined`.
 */
export interface BannerView {
  visible: boolean;
  provider?: LlmProvider | null;
  summary?: string;
}

export interface BannerState {
  banner: BannerView;
}

export const initialBannerState: BannerState = {
  banner: { visible: false },
};

// ---------------------------------------------------------------------------
// Reducer
// ---------------------------------------------------------------------------

/**
 * Pure reducer mapping `(state, event) -> nextState`.
 *
 * The `state` argument is accepted for the conventional `useReducer` signature
 * but the next state is fully determined by the incoming event — the banner is
 * stateless beyond "what was the most recent event".
 */
export function bannerReducer(
  _state: BannerState,
  event: BannerEvent,
): BannerState {
  switch (event.type) {
    case 'Error':
      return {
        banner: {
          visible: true,
          provider: event.provider,
          summary: event.summary,
        },
      };

    case 'Success':
    case 'RetrySuccess':
    case 'EditKey':
      return { banner: { visible: false } };
  }
}
