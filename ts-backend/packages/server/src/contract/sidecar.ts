// contract/sidecar.ts — zod transcription of crates/api/src/sidecar.rs.
//
// Wire DTOs for the Equivalent Code Sidecar, Algorithm Coverage Matrix, and
// Audit Snapshot Export endpoints. Rust BTreeMap<ReferenceSoftware, _> keyed
// by the enum serializes as a JSON object with "R"|"SAS"|"Python"|"SPSS" keys.

import { z } from 'zod';

export const referenceSoftware = z.enum(['R', 'SAS', 'Python', 'SPSS']);
export type ReferenceSoftware = z.infer<typeof referenceSoftware>;

// CoverageValueDto: lowercase/renamed tokens.
export const coverageValue = z.enum(['live', 'recorded', 'sidecar_only', 'none']);
export type CoverageValue = z.infer<typeof coverageValue>;

export const referenceImpl = z.object({
  callable: z.string(),
  package: z.string().nullable().optional(),
  version: z.string(),
});
export type ReferenceImpl = z.infer<typeof referenceImpl>;

// BTreeMap<ReferenceSoftware, V> → partial record keyed by the 4 tokens.
const bySoftware = <T extends z.ZodTypeAny>(value: T) =>
  z.record(referenceSoftware, value);

export const algorithmEntry = z.object({
  id: z.string(),
  display_name: z.string(),
  iterative: z.boolean(),
  /**
   * True when the algorithm is runnable from the shipped UI/HTTP surface;
   * false = engine-level verified only (parity/oracle), no run entry point
   * yet (G2 分层标注).
   */
  ui_runnable: z.boolean(),
  coverage: bySoftware(coverageValue),
  reference: bySoftware(referenceImpl),
});
export type AlgorithmEntry = z.infer<typeof algorithmEntry>;

export const coverageMatrix = z.object({
  schema_version: z.number().int().nonnegative(),
  release_version: z.string(),
  algorithms: z.array(algorithmEntry),
});
export type CoverageMatrix = z.infer<typeof coverageMatrix>;

export const sidecarSnippet = z.object({
  algorithm_id: z.string(),
  software: referenceSoftware,
  coverage_value: coverageValue,
  // serde(skip_serializing_if = Option::is_none): absent for "none".
  text: z.string().optional(),
  sha256_of_dataset: z.string(),
  release_version: z.string(),
});
export type SidecarSnippet = z.infer<typeof sidecarSnippet>;

export const sidecarColumn = z.object({
  name: z.string(),
  dtype: z.string(),
});

export const sidecarRenderRequest = z.object({
  software: referenceSoftware,
  dataset_sha256: z.string(),
  columns: z.array(sidecarColumn).default([]),
  params: z.record(z.string(), z.string()).default({}),
});
export type SidecarRenderRequest = z.infer<typeof sidecarRenderRequest>;

export const snapshotExportRequest = z.object({
  run_id: z.string(),
  destination: z.string(),
  /**
   * SPA 下载模式：导出完成后以 application/zip 流回浏览器。
   * 缺省 false，保持契约测试与桌面侧 JSON 响应不变。
   */
  download: z.boolean().optional().default(false),
});
export type SnapshotExportRequest = z.infer<typeof snapshotExportRequest>;

export const snapshotExportResponse = z.object({
  snapshot_path: z.string(),
  sha256: z.string(),
});
export type SnapshotExportResponse = z.infer<typeof snapshotExportResponse>;
