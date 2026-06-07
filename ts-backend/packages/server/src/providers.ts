// server/providers.ts — concrete trust-credential providers (task 13.7).
//
// Adapts the engine components (coverage matrix, sidecar generator, snapshot
// exporter) to the API_Contract DTO shapes and wires them into the HTTP_Server.
// Every provider runs inside the engine's guarded pipelines (the sidecar and
// snapshot generators wrap their bodies in guardedSpawn internally), so a
// forbidden runtime spawn aborts the request.

import { coverage, sidecar as engineSidecar, snapshot as engineSnapshot } from '@stats-code/engine';
import type { z } from 'zod';
import { sidecar as sidecarContract } from './contract/index.js';
import type {
  CoverageMatrixProvider,
  SidecarProvider,
  SnapshotProvider,
} from './state.js';

type WireMatrix = z.infer<typeof sidecarContract.coverageMatrix>;
type WireReferenceSoftware = z.infer<typeof sidecarContract.referenceSoftware>;
type WireSnippet = z.infer<typeof sidecarContract.sidecarSnippet>;

/**
 * Map the engine's internal coverage matrix (reference cells use fn/proc/pkg)
 * to the wire DTO (callable/package/version). `GET /api/coverage-matrix`
 * returns this verbatim.
 */
export function toWireMatrix(matrix: coverage.CoverageMatrix): WireMatrix {
  return {
    schema_version: matrix.schema_version,
    release_version: matrix.release_version,
    algorithms: matrix.algorithms.map((entry) => ({
      id: entry.id,
      display_name: entry.display_name,
      iterative: entry.iterative,
      coverage: { ...entry.coverage },
      reference: Object.fromEntries(
        coverage.REQUIRED_SOFTWARE.map((sw) => {
          const impl = entry.reference[sw];
          return [
            sw,
            {
              callable: impl.fn ?? impl.proc ?? '',
              package: impl.pkg ?? null,
              version: impl.version,
            },
          ];
        }),
      ),
    })),
  };
}

/** Coverage matrix provider backed by the engine's loaded matrix. */
export function createCoverageMatrixProvider(): CoverageMatrixProvider {
  return { get: () => toWireMatrix(coverage.getLoadedMatrix()) };
}

/**
 * Sidecar provider backed by the engine generator. Maps the rendered snippet
 * (or uncovered sentinel) to the wire DTO; `none` cells omit `text` (copy
 * disabled) and report coverage_value="none".
 */
export function createSidecarProvider(): SidecarProvider {
  return {
    generate(algorithmId, request): WireSnippet {
      const software = request.software as WireReferenceSoftware;
      const columns: engineSidecar.Column[] = (request.columns ?? []).map((c) => ({
        name: c.name,
        dtype: c.dtype as engineSidecar.ColumnDtype,
      }));
      const snip = engineSidecar.generateSnippet(
        algorithmId,
        software,
        request.params ?? {},
        columns,
        request.dataset_sha256,
      );
      if (snip.kind === 'uncovered') {
        return {
          algorithm_id: algorithmId,
          software,
          coverage_value: 'none',
          sha256_of_dataset: request.dataset_sha256,
          release_version: coverage.getLoadedMatrix().release_version,
        };
      }
      const state = coverage.coverageState(coverage.getLoadedMatrix(), algorithmId, software);
      return {
        algorithm_id: algorithmId,
        software,
        coverage_value: (state ?? 'live') as WireSnippet['coverage_value'],
        text: snip.text,
        sha256_of_dataset: snip.sha256OfDataset,
        release_version: snip.releaseVersion,
      };
    },
  };
}

/**
 * Snapshot provider that builds a RunSnapshot from a registered run and exports
 * it. The run resolver is injected so the route layer stays decoupled from the
 * session/run store; absence of a run surfaces as an error (HTTP 500 per route).
 */
export function createSnapshotProvider(
  resolveRun: (runId: string) => engineSnapshot.RunSnapshot | undefined,
): SnapshotProvider {
  return {
    export(runId, destination) {
      const run = resolveRun(runId);
      if (!run) {
        throw new Error(`run not found: ${runId}`);
      }
      const result = engineSnapshot.exportSnapshot(run, destination);
      return { snapshot_path: result.snapshotPath, sha256: result.sha256 };
    },
  };
}
