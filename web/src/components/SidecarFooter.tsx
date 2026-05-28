/**
 * SidecarFooter — provenance footer rendered beneath the active tab content
 * of `<EquivalentCodeSidecar>`.
 *
 * Per Requirement 1.7 the footer is unconditional: it shows the input
 * dataset SHA256 (64-character lowercase hexadecimal string) and the Stats
 * Code release version that produced the snippet, regardless of which tab
 * is active or whether that tab is rendering a snippet, a sidecar-only
 * notice, or a `none`-coverage placeholder. The component is intentionally
 * compact so it does not crowd the snippet body.
 *
 * The component is purely presentational — it accepts both values as props
 * and performs no I/O. The `aria-label` makes the region discoverable to
 * assistive tech as the snippet's provenance strip; the SHA256 is wrapped
 * in a `<code>` element so screen readers and styling treat it as a
 * monospaced literal, not prose.
 *
 * Validates: Requirements 1.7
 */

import React from 'react';

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

export interface SidecarFooterProps {
  /** 64-character lowercase hexadecimal SHA256 of the input dataset. */
  datasetSha256: string;
  /** Stats Code release version that emitted the snippet. */
  releaseVersion: string;
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function SidecarFooter({
  datasetSha256,
  releaseVersion,
}: SidecarFooterProps): React.ReactElement {
  return (
    <footer
      className="sidecar-footer"
      aria-label="Sidecar provenance"
      data-testid="sidecar-footer"
    >
      <span className="sidecar-footer-sha">
        data SHA256: <code>{datasetSha256}</code>
      </span>
      <span className="sidecar-footer-version">
        stats-code {releaseVersion}
      </span>
    </footer>
  );
}

export default SidecarFooter;
