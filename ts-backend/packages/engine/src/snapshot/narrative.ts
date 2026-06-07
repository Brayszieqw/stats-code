// snapshot/narrative.ts — `narrative.md` builder for the Audit_Snapshot (task 13.4).
// Transcribed 1:1 from crates/stats-code/src/snapshot/narrative.rs.
//
// `buildNarrative` is a PURE function: no clock, no environment, no I/O. Every
// numeric value the prose states is followed by an inline citation
// `[<artifact_path>#<json_pointer>]`. All cited paths are validated against the
// snapshot file index BEFORE any output byte is emitted; on the first miss the
// builder throws NarrativeError and produces no partial string.
//
// Output is plain UTF-8 markdown, LF-only line endings, single trailing LF.
//
// _Requirements: 6.7_

export interface KeyMetric {
  label: string;
  value: string;
  /** Path inside the snapshot, e.g. "artifacts/step-1/result.json". */
  artifactPath: string;
  /** JSON Pointer fragment, e.g. "estimate" or "ci/0". */
  jsonPointer: string;
}

export interface NarrativeStep {
  id: string;
  algorithm: string;
  displayName: string;
  paramsSummary: string;
  keyMetrics: KeyMetric[];
}

export class NarrativeError extends Error {
  constructor(public readonly path: string) {
    super(`narrative cites unknown artifact path: ${path}`);
    this.name = 'NarrativeError';
  }
}

/**
 * Build the `narrative.md` markdown body. `artifactsIndex` is the set of every
 * file path the snapshot will contain. Throws NarrativeError on the first
 * citation that does not resolve in the index (Requirement 6.7 integrity).
 */
export function buildNarrative(
  steps: readonly NarrativeStep[],
  artifactsIndex: ReadonlySet<string>,
): string {
  // Validate every citation up-front so a violation aborts before any byte
  // of markdown is materialised.
  for (const step of steps) {
    for (const metric of step.keyMetrics) {
      if (!artifactsIndex.has(metric.artifactPath)) {
        throw new NarrativeError(metric.artifactPath);
      }
    }
  }

  let out = '# Audit Snapshot Narrative\n';

  // Empty case: exactly the heading + single trailing LF, no blank line.
  if (steps.length === 0) {
    return out;
  }

  // Blank line between heading and first step.
  out += '\n';

  for (let i = 0; i < steps.length; i += 1) {
    const step = steps[i]!;
    if (i > 0) {
      // Blank-line separator between consecutive steps.
      out += '\n';
    }

    out += `## Step ${step.id}: ${step.displayName}\n`;
    out += '\n';
    out += `Algorithm \`${step.algorithm}\` with parameters: ${step.paramsSummary}.\n`;
    out += '\n';

    for (const metric of step.keyMetrics) {
      out += `- ${metric.label}: ${metric.value} [${metric.artifactPath}#${metric.jsonPointer}]\n`;
    }
  }

  return out;
}
