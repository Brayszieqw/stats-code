// snapshot/versions.ts — `versions.json` builder for the Audit_Snapshot (task 13.4).
// Transcribed 1:1 from crates/stats-code/src/snapshot/versions.rs.
//
// `buildVersions` is a PURE function: it reads no clock, no host environment,
// no random seeds. Privacy invariants (Requirement 6.6): host_name, user_name,
// and home-rooted absolute paths are never read here — they are simply not in
// the accepted input set. `os_version` is truncated to at most 32 Unicode
// scalar values on a character boundary.
//
// `runtime_dependencies` is emitted as a lexicographically-key-sorted object so
// serialization order is host-independent (mirrors Rust's BTreeMap).
//
// _Requirements: 6.6, 6.7_

export const VERSIONS_SCHEMA_VERSION = 1;

/** Max length, in Unicode scalar values, of `os_version`. */
export const OS_VERSION_MAX_CHARS = 32;

export interface ReferenceSoftwareVersion {
  name: string;
  version: string;
}

export interface Versions {
  schema_version: number;
  os_family: string;
  os_version: string;
  version_truncated: boolean;
  reference_software: ReferenceSoftwareVersion[];
  /** name → version, key-sorted for deterministic serialization. */
  runtime_dependencies: Record<string, string>;
}

/**
 * Truncate `raw` to at most OS_VERSION_MAX_CHARS Unicode scalar values on a
 * character boundary. Returns the value and whether truncation occurred.
 */
export function truncateOsVersion(raw: string): { value: string; truncated: boolean } {
  // Spread iterates by code point, never splitting a surrogate pair.
  const chars = [...raw];
  if (chars.length <= OS_VERSION_MAX_CHARS) {
    return { value: raw, truncated: false };
  }
  return { value: chars.slice(0, OS_VERSION_MAX_CHARS).join(''), truncated: true };
}

/** Return a new object with keys sorted lexicographically (BTreeMap analogue). */
function sortKeys(map: Record<string, string>): Record<string, string> {
  const out: Record<string, string> = {};
  for (const key of Object.keys(map).sort()) {
    out[key] = map[key]!;
  }
  return out;
}

/**
 * Build the `versions.json` payload. Pure: every dynamic input flows in via
 * arguments.
 *
 * @param osFamily one of "Windows" | "Linux" | "macOS" (caller-validated)
 * @param rawOsVersion host OS version string (truncated to 32 chars)
 * @param referenceSoftware reference software actually invoked, caller order
 * @param runtimeDeps direct runtime dependency name → version map
 */
export function buildVersions(
  osFamily: string,
  rawOsVersion: string,
  referenceSoftware: readonly ReferenceSoftwareVersion[],
  runtimeDeps: Record<string, string>,
): Versions {
  const { value: osVersion, truncated } = truncateOsVersion(rawOsVersion);
  return {
    schema_version: VERSIONS_SCHEMA_VERSION,
    os_family: osFamily,
    os_version: osVersion,
    version_truncated: truncated,
    reference_software: referenceSoftware.map((r) => ({ name: r.name, version: r.version })),
    runtime_dependencies: sortKeys(runtimeDeps),
  };
}
