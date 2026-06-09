/**
 * Tests for useModePreference.
 *
 * Property 1: preference round-trip (1.4, 1.5)
 * Property 2: illegal/missing value falls back to 'simple' (1.1, 1.6)
 * Unit: localStorage throwing degrades to in-memory state (1.4)
 */

import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import fc from 'fast-check';
import { useModePreference, MODE_STORAGE_KEY, type ViewMode } from './useModePreference';

beforeEach(() => {
  window.localStorage.clear();
  vi.restoreAllMocks();
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe('Property 1: mode preference round-trips (Requirements 1.4, 1.5)', () => {
  it('setMode(m) persists so a remount returns m', () => {
    fc.assert(
      fc.property(fc.constantFrom<ViewMode>('simple', 'pro'), (m) => {
        window.localStorage.clear();
        const first = renderHook(() => useModePreference());
        act(() => first.result.current.setMode(m));
        expect(first.result.current.mode).toBe(m);
        first.unmount();
        // Remount reads the persisted value.
        const second = renderHook(() => useModePreference());
        expect(second.result.current.mode).toBe(m);
        second.unmount();
      }),
      { numRuns: 20 },
    );
  });
});

describe('Property 2: illegal preference falls back to simple (Requirements 1.1, 1.6)', () => {
  it('any non-simple/pro stored value initializes to simple', () => {
    fc.assert(
      fc.property(fc.string(), (raw) => {
        fc.pre(raw !== 'simple' && raw !== 'pro');
        window.localStorage.clear();
        window.localStorage.setItem(MODE_STORAGE_KEY, raw);
        const { result, unmount } = renderHook(() => useModePreference());
        expect(result.current.mode).toBe('simple');
        unmount();
      }),
      { numRuns: 30 },
    );
  });

  it('missing value initializes to simple', () => {
    const { result } = renderHook(() => useModePreference());
    expect(result.current.mode).toBe('simple');
  });
});

describe('useModePreference — localStorage unavailable', () => {
  it('degrades to in-memory state when setItem throws; setMode still updates', () => {
    const setItem = vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
      throw new Error('quota exceeded');
    });
    const { result } = renderHook(() => useModePreference());
    expect(result.current.mode).toBe('simple');
    act(() => result.current.setMode('pro'));
    expect(result.current.mode).toBe('pro');
    expect(setItem).toHaveBeenCalled();
  });

  it('toggleMode flips simple <-> pro', () => {
    const { result } = renderHook(() => useModePreference());
    expect(result.current.mode).toBe('simple');
    act(() => result.current.toggleMode());
    expect(result.current.mode).toBe('pro');
    act(() => result.current.toggleMode());
    expect(result.current.mode).toBe('simple');
  });
});
