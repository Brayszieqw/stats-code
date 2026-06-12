/**
 * React context for the Algorithm Coverage Matrix.
 *
 * Fetches the matrix exactly once on provider mount and exposes it (plus the
 * loading / error state) to any descendant via the `useCoverageMatrix` hook.
 * The Rust side embeds the matrix in the binary, so the SPA never needs to
 * re-fetch within a single page lifetime.
 *
 * Usage:
 *   <CoverageMatrixProvider>
 *     <App />
 *   </CoverageMatrixProvider>
 *
 *   const { matrix, loading, error } = useCoverageMatrix();
 *
 * Validates: Requirements 6.2
 */

import {
  createContext,
  useContext,
  useEffect,
  useState,
  type ReactElement,
  type ReactNode,
} from 'react';

import {
  fetchCoverageMatrix,
  type CoverageMatrix,
} from './coverageMatrix';

// ---------------------------------------------------------------------------
// Context shape
// ---------------------------------------------------------------------------

export interface CoverageMatrixContextValue {
  /** Successfully fetched matrix, or `undefined` while loading / on error. */
  matrix?: CoverageMatrix;
  /** True until the first fetch resolves (success or failure). */
  loading: boolean;
  /** Populated when the fetch errors out; cleared on a fresh provider mount. */
  error?: Error;
}

const CoverageMatrixContext = createContext<CoverageMatrixContextValue>({
  matrix: undefined,
  loading: true,
  error: undefined,
});

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

export interface CoverageMatrixProviderProps {
  children: ReactNode;
  /**
   * Optional `fetch` override for tests. Production callers omit this and
   * the provider falls back to the global `fetch`.
   */
  fetchImpl?: typeof fetch;
}

/**
 * Fetches `/api/coverage-matrix` once on mount and exposes the result via
 * context. StrictMode-safe: dev-only effect cleanup cancels the obsolete
 * request, then the second setup starts the real request.
 */
export function CoverageMatrixProvider({
  children,
  fetchImpl,
}: CoverageMatrixProviderProps): ReactElement {
  const [value, setValue] = useState<CoverageMatrixContextValue>({
    matrix: undefined,
    loading: true,
    error: undefined,
  });

  useEffect(() => {
    let cancelled = false;
    const doFetch = fetchImpl ?? fetch;

    fetchCoverageMatrix(doFetch)
      .then((matrix) => {
        if (cancelled) return;
        setValue({ matrix, loading: false, error: undefined });
      })
      .catch((err: unknown) => {
        if (cancelled) return;
        const error = err instanceof Error ? err : new Error(String(err));
        setValue({ matrix: undefined, loading: false, error });
      });

    return () => {
      cancelled = true;
    };
  }, [fetchImpl]);

  return (
    <CoverageMatrixContext.Provider value={value}>
      {children}
    </CoverageMatrixContext.Provider>
  );
}

// ---------------------------------------------------------------------------
// Hook
// ---------------------------------------------------------------------------

/**
 * Read the coverage matrix from context. Outside a `<CoverageMatrixProvider>`
 * this returns the default `{ loading: true }` shape, which lets components
 * render their loading state without crashing in isolation.
 */
export function useCoverageMatrix(): CoverageMatrixContextValue {
  return useContext(CoverageMatrixContext);
}
