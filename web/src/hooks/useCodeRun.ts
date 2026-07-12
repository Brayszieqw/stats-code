/**
 * useCodeRun — drives the Pro-mode RunControls run state machine
 * (idle → running → success | error), wrapping the runSkill API call.
 * Supports reset() and an AbortController-based stop().
 *
 * Validates: Requirements 7.4, 12.7
 */

import { useCallback, useRef, useState } from 'react';
import { runSkill, ApiError } from '../api/client';
import type { RunRequest, SkillResult } from '../api/types';

export type RunState =
  | { status: 'idle' }
  | { status: 'running' }
  | { status: 'success'; result: SkillResult }
  | { status: 'error'; message: string; code?: string };

export interface UseCodeRunReturn {
  state: RunState;
  run: (sessionId: string, body: RunRequest) => Promise<SkillResult | null>;
  reset: () => void;
  stop: () => void;
}

export function useCodeRun(): UseCodeRunReturn {
  const [state, setState] = useState<RunState>({ status: 'idle' });
  const abortRef = useRef<AbortController | null>(null);

  const run = useCallback(async (sessionId: string, body: RunRequest) => {
    // Abort any in-flight run before starting a new one.
    if (abortRef.current) {
      abortRef.current.abort();
    }
    const controller = new AbortController();
    abortRef.current = controller;
    setState({ status: 'running' });
    try {
      const result = await runSkill(sessionId, body, controller.signal);
      // Ignore the result if this run was superseded/aborted.
      if (controller.signal.aborted) return null;
      setState({ status: 'success', result });
      return result;
    } catch (err) {
      if (controller.signal.aborted) return null;
      if (err instanceof ApiError) {
        setState({ status: 'error', message: err.payload.message, code: err.payload.error_code });
      } else {
        setState({ status: 'error', message: err instanceof Error ? err.message : '运行失败' });
      }
      return null;
    } finally {
      if (abortRef.current === controller) {
        abortRef.current = null;
      }
    }
  }, []);

  const reset = useCallback(() => {
    setState({ status: 'idle' });
  }, []);

  const stop = useCallback(() => {
    if (abortRef.current) {
      abortRef.current.abort();
      abortRef.current = null;
    }
    setState({ status: 'idle' });
  }, []);

  return { state, run, reset, stop };
}
