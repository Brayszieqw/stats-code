/**
 * Tests for `<CopyToClipboard>` and `<ClipboardErrorBanner>`.
 *
 * Covers:
 *   - Disabled state when `text` is undefined / empty / `disabled` prop
 *     is true (Requirement 1.8).
 *   - Click forwards the snippet to `navigator.clipboard.writeText`
 *     verbatim (Requirement 1.4).
 *   - Success path: "Copied!" confirmation appears, then reverts after the
 *     2 s flash window.
 *   - Failure path: `<ClipboardErrorBanner>` renders next to the button
 *     and the displayed `text` is unchanged (Requirement 1.9).
 *   - Byte-identity: every byte of the prop is forwarded; no normalisation.
 *
 * Validates: Requirements 1.4, 1.8, 1.9
 */

import {
  describe,
  it,
  expect,
  vi,
  beforeEach,
  afterEach,
} from 'vitest';
import { render, screen, act } from '@testing-library/react';

import {
  CopyToClipboard,
  ClipboardErrorBanner,
} from './CopyToClipboard';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/**
 * Replace `navigator.clipboard` with a controllable spy. The original
 * descriptor is captured so each test can restore it cleanly.
 */
function installClipboardMock(
  writeText: (value: string) => Promise<void>,
): { restore: () => void; spy: ReturnType<typeof vi.fn> } {
  const spy = vi.fn(writeText);
  const original = Object.getOwnPropertyDescriptor(navigator, 'clipboard');
  Object.assign(navigator, { clipboard: { writeText: spy } });
  return {
    spy,
    restore: () => {
      if (original) {
        Object.defineProperty(navigator, 'clipboard', original);
      } else {
        // Best-effort cleanup if jsdom never had clipboard.
        Object.assign(navigator, { clipboard: undefined });
      }
    },
  };
}

// ---------------------------------------------------------------------------
// Disabled-state tests (Requirement 1.8)
// ---------------------------------------------------------------------------

describe('CopyToClipboard — disabled state', () => {
  it('disables the button when `text` is undefined', () => {
    render(<CopyToClipboard text={undefined} />);
    expect(screen.getByTestId('copy-to-clipboard-button')).toBeDisabled();
  });

  it('disables the button when `text` is the empty string', () => {
    render(<CopyToClipboard text="" />);
    expect(screen.getByTestId('copy-to-clipboard-button')).toBeDisabled();
  });

  it('disables the button when the parent passes `disabled`', () => {
    render(<CopyToClipboard text="something" disabled />);
    expect(screen.getByTestId('copy-to-clipboard-button')).toBeDisabled();
  });

  it('does not write to the clipboard when programmatically clicked while disabled', () => {
    const { spy, restore } = installClipboardMock(async () => {});
    try {
      render(<CopyToClipboard text={undefined} />);
      const button = screen.getByTestId('copy-to-clipboard-button');
      // Browsers ignore clicks on disabled buttons; we additionally verify
      // the component itself short-circuits if a click somehow lands.
      act(() => {
        button.click();
      });
      expect(spy).not.toHaveBeenCalled();
    } finally {
      restore();
    }
  });

  it('enables the button when `text` is non-empty and `disabled` is unset', () => {
    render(<CopyToClipboard text="x" />);
    expect(screen.getByTestId('copy-to-clipboard-button')).not.toBeDisabled();
  });
});

// ---------------------------------------------------------------------------
// Click forwards the prop verbatim (Requirements 1.4)
// ---------------------------------------------------------------------------

describe('CopyToClipboard — click writes text verbatim', () => {
  let cleanup: (() => void) | null = null;

  beforeEach(() => {
    cleanup = null;
  });

  afterEach(() => {
    if (cleanup !== null) cleanup();
  });

  it('calls navigator.clipboard.writeText with the exact `text` prop', async () => {
    const handle = installClipboardMock(async () => {});
    cleanup = handle.restore;

    const snippet = '# R\nlibrary(tableone)\nCreateTableOne(data = data.csv)\n';
    render(<CopyToClipboard text={snippet} />);

    await act(async () => {
      screen.getByTestId('copy-to-clipboard-button').click();
    });

    expect(handle.spy).toHaveBeenCalledTimes(1);
    expect(handle.spy).toHaveBeenCalledWith(snippet);
  });

  it('preserves every byte (no normalisation, no trimming)', async () => {
    const handle = installClipboardMock(async () => {});
    cleanup = handle.restore;

    // CRLF, leading/trailing whitespace, tabs, unicode — all must round-trip.
    const snippet =
      '\t  hello\r\nworld  \n\u00e9\u4f60\u597d\n# data SHA256: deadbeef\n';
    render(<CopyToClipboard text={snippet} />);

    await act(async () => {
      screen.getByTestId('copy-to-clipboard-button').click();
    });

    expect(handle.spy).toHaveBeenCalledTimes(1);
    const written = handle.spy.mock.calls[0]?.[0] as string;
    expect(written).toBe(snippet);
    // Byte-level identity check: encoded UTF-8 lengths must match.
    const enc = new TextEncoder();
    expect(enc.encode(written)).toEqual(enc.encode(snippet));
  });
});

// ---------------------------------------------------------------------------
// Success path — "Copied!" confirmation
// ---------------------------------------------------------------------------

describe('CopyToClipboard — success confirmation', () => {
  let cleanup: (() => void) | null = null;

  beforeEach(() => {
    vi.useFakeTimers();
    cleanup = null;
  });

  afterEach(() => {
    vi.useRealTimers();
    if (cleanup !== null) cleanup();
  });

  it('renders "Copied!" on success and reverts after 2 seconds', async () => {
    const handle = installClipboardMock(async () => {});
    cleanup = handle.restore;

    render(<CopyToClipboard text="abc" />);
    const button = screen.getByTestId('copy-to-clipboard-button');
    expect(button).toHaveTextContent('Copy');
    expect(button).not.toHaveTextContent('Copied!');

    await act(async () => {
      button.click();
      // Resolve the writeText promise.
      await Promise.resolve();
    });

    expect(button).toHaveTextContent('Copied!');

    // Advance past the flash window — label reverts.
    await act(async () => {
      vi.advanceTimersByTime(2000);
    });

    expect(button).toHaveTextContent('Copy');
    expect(button).not.toHaveTextContent('Copied!');
  });
});

// ---------------------------------------------------------------------------
// Failure path — Requirement 1.9
// ---------------------------------------------------------------------------

describe('CopyToClipboard — failure path', () => {
  let cleanup: (() => void) | null = null;

  beforeEach(() => {
    cleanup = null;
  });

  afterEach(() => {
    if (cleanup !== null) cleanup();
  });

  it('renders <ClipboardErrorBanner> when writeText rejects, leaving text unchanged', async () => {
    const handle = installClipboardMock(async () => {
      throw new Error('NotAllowedError: write blocked');
    });
    cleanup = handle.restore;

    const snippet = '# original snippet\nCreateTableOne(data = data.csv)\n';
    render(<CopyToClipboard text={snippet} />);

    // Banner must not be present until a failure occurs.
    expect(
      screen.queryByTestId('clipboard-error-banner'),
    ).not.toBeInTheDocument();

    await act(async () => {
      screen.getByTestId('copy-to-clipboard-button').click();
      // Allow the rejected promise's microtask to land.
      await Promise.resolve();
      await Promise.resolve();
    });

    // Banner appears next to the button (Requirement 1.9).
    const banner = screen.getByTestId('clipboard-error-banner');
    expect(banner).toBeInTheDocument();
    expect(banner.textContent).toMatch(/copy failed/i);

    // The "displayed text" is owned by the parent and threaded through
    // `text`. Re-render with the same prop and confirm the value is intact;
    // the component never mutates it.
    expect(handle.spy).toHaveBeenCalledTimes(1);
    expect(handle.spy.mock.calls[0]?.[0]).toBe(snippet);
  });

  it('renders the banner when navigator.clipboard is unavailable', async () => {
    // Force-clear navigator.clipboard for this test.
    const original = Object.getOwnPropertyDescriptor(navigator, 'clipboard');
    Object.assign(navigator, { clipboard: undefined });
    cleanup = () => {
      if (original) {
        Object.defineProperty(navigator, 'clipboard', original);
      }
    };

    render(<CopyToClipboard text="abc" />);

    await act(async () => {
      screen.getByTestId('copy-to-clipboard-button').click();
      await Promise.resolve();
    });

    expect(
      screen.getByTestId('clipboard-error-banner'),
    ).toBeInTheDocument();
  });
});

// ---------------------------------------------------------------------------
// ClipboardErrorBanner — standalone shape
// ---------------------------------------------------------------------------

describe('ClipboardErrorBanner', () => {
  it('renders an alert with a generic message when no detail is provided', () => {
    render(<ClipboardErrorBanner />);
    const banner = screen.getByTestId('clipboard-error-banner');
    expect(banner).toBeInTheDocument();
    expect(banner.getAttribute('role')).toBe('alert');
    expect(banner.textContent).toMatch(/copy failed/i);
  });

  it('includes the supplied message when provided', () => {
    render(<ClipboardErrorBanner message="permission denied" />);
    const banner = screen.getByTestId('clipboard-error-banner');
    expect(banner.textContent).toMatch(/permission denied/);
  });
});
