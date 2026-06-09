/**
 * Property-based tests for the Equivalent Code Sidecar SPA components.
 *
 * Uses fast-check to verify:
 *   - Property 8: Active-tab rendering reflects matrix coverage state.
 *   - Property 9: Copy-to-clipboard is a verbatim identity.
 *
 * Validates: Requirements 1.3, 1.4, 6.4
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, act } from '@testing-library/react';
import * as fc from 'fast-check';

// -- Hoisted spies so mocks can reference them before module load
const { useSidecarSpy, useCoverageMatrixSpy } = vi.hoisted(() => ({
  useSidecarSpy: vi.fn(),
  useCoverageMatrixSpy: vi.fn(),
}));

vi.mock('../../hooks/useSidecar', () => ({
  useSidecar: useSidecarSpy,
}));

vi.mock('../../lib/coverageMatrixContext', () => ({
  useCoverageMatrix: useCoverageMatrixSpy,
}));

import { EquivalentCodeSidecar } from '../EquivalentCodeSidecar';
import { CopyToClipboard } from '../CopyToClipboard';
import type {
  CoverageMatrix,
  CoverageState,
  ReferenceSoftware,
} from '../../lib/coverageMatrix';

// ---------------------------------------------------------------------------
// Arbitraries
// ---------------------------------------------------------------------------

const arbCoverageState: fc.Arbitrary<CoverageState> = fc.constantFrom(
  'live',
  'recorded',
  'sidecar_only',
  'none',
);

const arbSoftware: fc.Arbitrary<ReferenceSoftware> = fc.constantFrom(
  'R',
  'SAS',
  'Python',
  'SPSS',
);

const arbAlgorithmId: fc.Arbitrary<string> = fc.stringMatching(
  /^[a-z0-9_-]{1,20}$/,
);

const arbSha256: fc.Arbitrary<string> = fc.stringMatching(
  /^[0-9a-f]{64}$/,
);

const arbVersion: fc.Arbitrary<string> = fc.tuple(
  fc.integer({ min: 0, max: 99 }),
  fc.integer({ min: 0, max: 99 }),
  fc.integer({ min: 0, max: 99 }),
).map(([a, b, c]) => `${a}.${b}.${c}`);

// Snippet text: printable strings whose visible text is non-blank.
//
// `toHaveTextContent` normalizes/collapses whitespace, so a whitespace-only
// body (e.g. `" "`) has empty normalized text and cannot be asserted against
// the raw string. A real sidecar body is never whitespace-only — the snippet
// always carries the LF-terminated header banner — so restricting the
// generator to bodies with at least one non-space character keeps the oracle
// faithful to production without masking any behaviour under test.
const arbSnippetText: fc.Arbitrary<string> = fc
  .string({ minLength: 1, maxLength: 200 })
  .filter((s) => s.trim().length > 0);

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function buildMatrix(
  algorithmId: string,
  coverageByTab: Record<ReferenceSoftware, CoverageState>,
): CoverageMatrix {
  return {
    schema_version: 1,
    release_version: '0.5.0',
    algorithms: [
      {
        id: algorithmId,
        display_name: algorithmId,
        iterative: false,
        coverage: coverageByTab,
        reference: {
          R: { callable: 'fn', package: 'pkg', version: '1.0' },
          SAS: { callable: 'PROC X', version: '9.4' },
          Python: { callable: 'fn', package: 'pkg', version: '1.0' },
          SPSS: { callable: 'PROC X', version: '29.0' },
        },
      },
    ],
  };
}

// ---------------------------------------------------------------------------
// Property 8: SPA active-tab rendering reflects matrix coverage
// **Validates: Requirements 1.3, 6.4**
// ---------------------------------------------------------------------------

describe('Property 8: SPA active-tab rendering reflects matrix coverage', () => {
  beforeEach(() => {
    useSidecarSpy.mockReset();
    useCoverageMatrixSpy.mockReset();
  });

  it('for any (coverage state, active tab), rendered content matches expectations', () => {
    fc.assert(
      fc.property(
        arbAlgorithmId,
        arbSoftware,
        arbCoverageState,
        arbSnippetText,
        arbSha256,
        arbVersion,
        (algorithmId, activeTab, coverageState, snippetBody, sha256, version) => {
          // Build a matrix where the active tab has the given coverage state
          // and all other tabs are 'live' (so they don't interfere).
          const coverageByTab: Record<ReferenceSoftware, CoverageState> = {
            R: 'live',
            SAS: 'live',
            Python: 'live',
            SPSS: 'live',
          };
          coverageByTab[activeTab] = coverageState;

          const matrix = buildMatrix(algorithmId, coverageByTab);

          // Mock useCoverageMatrix to return the matrix synchronously
          useCoverageMatrixSpy.mockReturnValue({
            matrix,
            loading: false,
            error: undefined,
          });

          // Configure useSidecar mock
          useSidecarSpy.mockImplementation(
            ({ software, enabled = true }: {
              algorithmId: string;
              software: ReferenceSoftware;
              enabled?: boolean;
            }) => {
              if (!enabled) return { loading: false };
              return {
                loading: false,
                snippet: {
                  algorithm_id: algorithmId,
                  software,
                  coverage_value: coverageByTab[software],
                  text: snippetBody,
                  sha256_of_dataset: sha256,
                  release_version: version,
                },
              };
            },
          );

          const { unmount } = render(
            <EquivalentCodeSidecar
              algorithmId={algorithmId}
              columns={[{ name: 'col0', dtype: 'numeric' }]}
              datasetSha256={sha256}
              releaseVersion={version}
            />,
          );

          // Guarantee the rendered tree is torn down even when an
          // assertion throws mid-iteration; otherwise the leaked DOM from a
          // failed shrink step makes the next iteration see duplicate
          // `sidecar-*` nodes ("multiple elements found").
          try {
            // Click the target tab if it's not R (default)
            if (activeTab !== 'R') {
              act(() => {
                screen.getByTestId(`sidecar-tab-${activeTab}`).click();
              });
            }

            // Assert based on coverage state
            if (coverageState === 'none') {
              // `none` → placeholder rendered, no snippet text
              expect(
                screen.getByTestId('sidecar-placeholder'),
              ).toBeInTheDocument();
              expect(
                screen.queryByTestId('sidecar-snippet'),
              ).not.toBeInTheDocument();
              // Copy button disabled
              expect(
                screen.getByTestId('copy-to-clipboard-button'),
              ).toBeDisabled();
            } else {
              // Non-none → snippet rendered
              expect(screen.getByTestId('sidecar-snippet')).toBeInTheDocument();
              // `toHaveTextContent` normalizes/collapses whitespace on the DOM
              // side, so compare against a like-normalized body to avoid false
              // mismatches when the generated body has internal whitespace runs
              // (e.g. "!  !" renders as "! !").
              const normalizedBody = snippetBody.replace(/\s+/g, ' ').trim();
              expect(screen.getByTestId('sidecar-snippet')).toHaveTextContent(
                normalizedBody,
              );

              if (coverageState === 'sidecar_only') {
                // sidecar_only → inline notice present, names the software
                const notice = screen.getByTestId('sidecar-notice');
                expect(notice).toBeInTheDocument();
                expect(notice.textContent).toContain(activeTab);
              } else {
                // live / recorded → no notice
                expect(
                  screen.queryByTestId('sidecar-notice'),
                ).not.toBeInTheDocument();
              }

              // Copy button enabled
              expect(
                screen.getByTestId('copy-to-clipboard-button'),
              ).not.toBeDisabled();
            }
          } finally {
            unmount();
          }
        },
      ),
      { numRuns: 50 },
    );
  });
});

// ---------------------------------------------------------------------------
// Property 9: Copy-to-clipboard is a verbatim identity
// **Validates: Requirements 1.4**
// ---------------------------------------------------------------------------

describe('Property 9: Copy-to-clipboard is a verbatim identity', () => {
  let clipboardRestore: (() => void) | null = null;

  afterEach(() => {
    if (clipboardRestore) {
      clipboardRestore();
      clipboardRestore = null;
    }
  });

  it('for any snippet string, clipboard.writeText receives the exact same string', async () => {
    await fc.assert(
      fc.asyncProperty(
        // Generate arbitrary non-empty strings including unicode, whitespace, newlines
        fc.string({ minLength: 1, maxLength: 500 }),
        async (snippetText) => {
          // Install a clipboard mock that records what was written
          let writtenValue: string | undefined;
          const writeTextSpy = vi.fn(async (value: string) => {
            writtenValue = value;
          });

          const original = Object.getOwnPropertyDescriptor(
            navigator,
            'clipboard',
          );
          Object.assign(navigator, { clipboard: { writeText: writeTextSpy } });
          clipboardRestore = () => {
            if (original) {
              Object.defineProperty(navigator, 'clipboard', original);
            } else {
              Object.assign(navigator, { clipboard: undefined });
            }
          };

          const { unmount } = render(
            <CopyToClipboard text={snippetText} />,
          );

          // Click the copy button
          await act(async () => {
            screen.getByTestId('copy-to-clipboard-button').click();
            await Promise.resolve();
          });

          // Verify: writeText was called with the exact snippet
          expect(writeTextSpy).toHaveBeenCalledTimes(1);
          expect(writtenValue).toBe(snippetText);

          // Verify: the text prop is unchanged (component doesn't mutate it)
          expect(writtenValue).toStrictEqual(snippetText);

          unmount();
          clipboardRestore();
          clipboardRestore = null;
        },
      ),
      { numRuns: 50 },
    );
  });
});
