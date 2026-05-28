/**
 * Tests for `SidecarFooter` — covers Requirement 1.7 ("the SHA256 of the
 * input dataset as a 64-character lowercase hexadecimal string and the
 * Stats Code release version" must appear regardless of tab state).
 */

import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';

import { SidecarFooter } from './SidecarFooter';

const SAMPLE_SHA =
  'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855';
const SAMPLE_VERSION = '0.1.0';

describe('SidecarFooter', () => {
  it('renders the dataset SHA256 verbatim', () => {
    render(
      <SidecarFooter
        datasetSha256={SAMPLE_SHA}
        releaseVersion={SAMPLE_VERSION}
      />,
    );
    // The SHA is wrapped in a <code> element; assert it appears verbatim.
    expect(screen.getByText(SAMPLE_SHA)).toBeInTheDocument();
    expect(screen.getByText(SAMPLE_SHA).tagName).toBe('CODE');
  });

  it('renders the release version verbatim', () => {
    render(
      <SidecarFooter
        datasetSha256={SAMPLE_SHA}
        releaseVersion={SAMPLE_VERSION}
      />,
    );
    expect(
      screen.getByText(`stats-code ${SAMPLE_VERSION}`),
    ).toBeInTheDocument();
  });

  it('exposes a labelled landmark for assistive tech', () => {
    render(
      <SidecarFooter
        datasetSha256={SAMPLE_SHA}
        releaseVersion={SAMPLE_VERSION}
      />,
    );
    // Per WAI-ARIA, a <footer> within the body has implicit role
    // `contentinfo`; an aria-label makes it a discoverable landmark.
    const footer = screen.getByLabelText('Sidecar provenance');
    expect(footer).toBeInTheDocument();
    expect(footer.tagName).toBe('FOOTER');
  });

  it('renders both fields together regardless of order', () => {
    render(
      <SidecarFooter
        datasetSha256={SAMPLE_SHA}
        releaseVersion="9.9.9-rc.1"
      />,
    );
    const footer = screen.getByTestId('sidecar-footer');
    expect(footer.textContent).toContain(SAMPLE_SHA);
    expect(footer.textContent).toContain('9.9.9-rc.1');
    expect(footer.textContent).toContain('data SHA256:');
  });
});
