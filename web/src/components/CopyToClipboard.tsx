/**
 * CopyToClipboard — control that copies a snippet body verbatim to the
 * system clipboard.
 *
 * Per Requirement 1.4 the bytes written to the clipboard must be exactly
 * equal to the snippet text displayed to the user. This component therefore
 * forwards `text` to `navigator.clipboard.writeText` without normalisation,
 * trimming, or transformation of any kind.
 *
 * Per Requirement 1.8 the button must be disabled whenever there is nothing
 * meaningful to copy. The component treats `text === undefined`, the empty
 * string, and the parent-supplied `disabled` flag as equivalent triggers
 * for the disabled state. Click handling is short-circuited as a defence
 * in depth so a programmatic click on a disabled button still cannot reach
 * the clipboard.
 *
 * Per Requirement 1.9 a failed clipboard write must surface a visible
 * error banner next to the button without mutating the displayed snippet
 * text. The banner is rendered as a sibling of the button by
 * `<ClipboardErrorBanner>`. The component owns no copy of the snippet body
 * and therefore cannot — and does not — alter it on failure.
 *
 * On success the component renders a brief inline "Copied!" confirmation
 * for 2 seconds, then reverts to the default label. The 2 s timer is
 * cleared on unmount or on a subsequent click so concurrent activations
 * collapse cleanly.
 *
 * Validates: Requirements 1.4, 1.8, 1.9
 */

import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type ReactElement,
} from 'react';

// ---------------------------------------------------------------------------
// ClipboardErrorBanner
// ---------------------------------------------------------------------------

export interface ClipboardErrorBannerProps {
  /** Human-readable message describing why the write failed. */
  message?: string;
}

/**
 * Visible alert rendered next to the copy button when
 * `navigator.clipboard.writeText` rejects. The displayed snippet is owned
 * by the parent and is intentionally not threaded through this component
 * so it cannot be mutated on failure (Requirement 1.9).
 */
export function ClipboardErrorBanner({
  message,
}: ClipboardErrorBannerProps): ReactElement {
  return (
    <span
      className="clipboard-error-banner"
      data-testid="clipboard-error-banner"
      role="alert"
    >
      Copy failed{message ? `: ${message}` : '.'}
    </span>
  );
}

// ---------------------------------------------------------------------------
// CopyToClipboard
// ---------------------------------------------------------------------------

export interface CopyToClipboardProps {
  /**
   * Snippet body to copy. `undefined` (or `''`) signals "nothing to copy"
   * and forces the button into the disabled state (Requirement 1.8).
   */
  text: string | undefined;
  /** Explicit parent override that disables the control. */
  disabled?: boolean;
  /** Optional button label. Defaults to "Copy". */
  label?: string;
}

/** Window during which the success confirmation is visible, in ms. */
const COPIED_FLASH_MS = 2000;

/**
 * Resolve the navigator-clipboard write function lazily so the component
 * picks up test-time monkey patches like
 * `Object.assign(navigator, { clipboard: { writeText: vi.fn() } })`.
 */
function resolveWriteText(): ((value: string) => Promise<void>) | undefined {
  if (typeof navigator === 'undefined') {
    return undefined;
  }
  const clip = (navigator as Navigator & { clipboard?: Clipboard }).clipboard;
  if (clip === undefined || typeof clip.writeText !== 'function') {
    return undefined;
  }
  return clip.writeText.bind(clip);
}

export function CopyToClipboard(props: CopyToClipboardProps): ReactElement {
  const { text, disabled = false, label = 'Copy' } = props;

  const [copied, setCopied] = useState(false);
  const [errorMessage, setErrorMessage] = useState<string | undefined>(
    undefined,
  );

  // Track the success-flash timer so we can clear it on unmount or on a
  // subsequent click. `setTimeout` returns different types in the browser
  // (number) and node (Timeout); `ReturnType` keeps both happy.
  const flashTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    return () => {
      if (flashTimer.current !== null) {
        clearTimeout(flashTimer.current);
        flashTimer.current = null;
      }
    };
  }, []);

  const isEffectivelyDisabled =
    disabled || text === undefined || text === '';

  const handleClick = useCallback(async () => {
    // Short-circuit: a programmatic click on a disabled control must not
    // reach the clipboard (Requirement 1.8).
    if (isEffectivelyDisabled || text === undefined) {
      return;
    }

    // Cancel any in-flight success flash so concurrent clicks don't leave
    // the label stuck in "Copied!".
    if (flashTimer.current !== null) {
      clearTimeout(flashTimer.current);
      flashTimer.current = null;
    }

    const writeText = resolveWriteText();
    if (writeText === undefined) {
      setCopied(false);
      setErrorMessage('Clipboard API unavailable');
      return;
    }

    try {
      // Forward the exact bytes (Requirement 1.4). No normalisation.
      await writeText(text);
      setErrorMessage(undefined);
      setCopied(true);
      flashTimer.current = setTimeout(() => {
        setCopied(false);
        flashTimer.current = null;
      }, COPIED_FLASH_MS);
    } catch (err: unknown) {
      // Failure must not mutate the displayed snippet (Requirement 1.9).
      // We only surface a sibling banner; the parent's `text` prop is
      // untouched by definition.
      setCopied(false);
      const msg =
        err instanceof Error && err.message.length > 0
          ? err.message
          : undefined;
      setErrorMessage(msg);
    }
  }, [isEffectivelyDisabled, text]);

  const buttonLabel = copied ? 'Copied!' : label;

  return (
    <span className="copy-to-clipboard" data-testid="copy-to-clipboard">
      <button
        type="button"
        className="copy-to-clipboard-button"
        data-testid="copy-to-clipboard-button"
        disabled={isEffectivelyDisabled}
        aria-live="polite"
        onClick={() => {
          // Fire-and-forget: React onClick doesn't await the promise.
          void handleClick();
        }}
      >
        {buttonLabel}
      </button>
      {errorMessage !== undefined && (
        <ClipboardErrorBanner message={errorMessage} />
      )}
    </span>
  );
}

export default CopyToClipboard;
