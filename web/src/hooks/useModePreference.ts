/**
 * useModePreference — persistent view-mode preference (Requirement 1).
 *
 * Reads the initial mode from localStorage; only 'simple' | 'pro' are accepted,
 * otherwise falls back to 'simple'. setMode/toggleMode update state and write
 * back to localStorage. All localStorage access is wrapped in try/catch so a
 * disabled or throwing storage degrades to an in-memory preference.
 *
 * Validates: Requirements 1.1, 1.4, 1.5, 1.6
 */

import { useCallback, useState } from 'react';

export type ViewMode = 'simple' | 'pro';

export const MODE_STORAGE_KEY = 'dual-mode-frontend.mode';

export interface UseModePreferenceReturn {
  mode: ViewMode;
  setMode: (next: ViewMode) => void;
  toggleMode: () => void;
}

/** Validate an arbitrary stored value, returning a ViewMode or null. */
export function parseStoredMode(value: unknown): ViewMode | null {
  return value === 'simple' || value === 'pro' ? value : null;
}

/** Read the persisted mode, tolerating unavailable/throwing localStorage. */
function readInitialMode(): ViewMode {
  try {
    const raw = window.localStorage.getItem(MODE_STORAGE_KEY);
    return parseStoredMode(raw) ?? 'simple';
  } catch {
    return 'simple';
  }
}

export function useModePreference(): UseModePreferenceReturn {
  const [mode, setModeState] = useState<ViewMode>(readInitialMode);

  const setMode = useCallback((next: ViewMode) => {
    setModeState(next);
    try {
      window.localStorage.setItem(MODE_STORAGE_KEY, next);
    } catch {
      // localStorage unavailable — keep the in-memory state only.
    }
  }, []);

  const toggleMode = useCallback(() => {
    setModeState((prev) => {
      const next: ViewMode = prev === 'simple' ? 'pro' : 'simple';
      try {
        window.localStorage.setItem(MODE_STORAGE_KEY, next);
      } catch {
        // ignore
      }
      return next;
    });
  }, []);

  return { mode, setMode, toggleMode };
}
