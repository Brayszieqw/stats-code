// server/providers.ts — concrete trust-credential providers (task 13.7).
//
// Adapts the engine components (coverage matrix, sidecar generator, snapshot
// exporter) to the API_Contract DTO shapes and wires them into the HTTP_Server.
// Every provider runs inside the engine's guarded pipelines (the sidecar and
// snapshot generators wrap their bodies in guardedSpawn internally), so a
// forbidden runtime spawn aborts the request.

import { createHash } from 'node:crypto';
import { platform, release } from 'node:os';
import { coverage, sidecar as engineSidecar, snapshot as engineSnapshot } from '@stats-code/engine';
import type { z } from 'zod';
import { sidecar as sidecarContract } from './contract/index.js';
import type {
  CoverageMatrixProvider,
  DatasetStore,
  SidecarProvider,
  SnapshotProvider,
  SnapshotRunRecorder,
  SnapshotRunRegistration,
  LlmConfig,
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
  resolveRun: (
    runId: string,
  ) => engineSnapshot.RunSnapshot | undefined | Promise<engineSnapshot.RunSnapshot | undefined>,
): SnapshotProvider {
  return {
    async export(runId, destination) {
      const run = await resolveRun(runId);
      if (!run) {
        throw new Error(`run not found: ${runId}`);
      }
      const result = engineSnapshot.exportSnapshot(run, destination);
      return { snapshot_path: result.snapshotPath, sha256: result.sha256 };
    },
  };
}

export interface SnapshotRunRegistry {
  recorder: SnapshotRunRecorder;
  provider: SnapshotProvider;
  /** Last successful archive hash retained as the server-side trust anchor. */
  snapshotSha256(runId: string): string | undefined;
}

export interface CreateSnapshotRunRegistryOptions {
  llmCallsForSession?: (sessionId: string) => readonly engineSnapshot.LlmCall[];
  llmConfig?: () => LlmConfig | null;
  workingDirectory?: string;
}

function osFamily(): string {
  if (platform() === 'win32') return 'Windows';
  if (platform() === 'darwin') return 'macOS';
  return 'Linux';
}

function commitSha(): string {
  const value = process.env.STATS_CODE_COMMIT_SHA ?? '';
  return /^[0-9a-f]{40}$/i.test(value) ? value.toLowerCase() : '0'.repeat(40);
}

/**
 * Keep only lightweight run metadata in memory. Dataset bytes are reloaded
 * from the local dataset store when the user exports a snapshot, avoiding one
 * large in-memory copy per completed run.
 */
export function createSnapshotRunRegistry(
  datasetStore: DatasetStore,
  options: CreateSnapshotRunRegistryOptions = {},
): SnapshotRunRegistry {
  const records = new Map<string, SnapshotRunRegistration>();
  const snapshotHashes = new Map<string, string>();
  const enc = new TextEncoder();

  const recorder: SnapshotRunRecorder = {
    register(run) {
      records.set(run.runId, run);
    },
  };

  const materializer = createSnapshotProvider(async (runId) => {
    const record = records.get(runId);
    if (!record) return undefined;

    const datasetBytes = await datasetStore.readRawById(record.datasetSummary.dataset_id);
    const datasetHex = engineSnapshot.sha256Hex(datasetBytes);
    const protocolDocument = record.researchProtocol ?? { status: 'NotRecorded' };
    const approvalDocument = {
      schema_version: 1,
      session_id: record.sessionId,
      run_id: record.runId,
      protocol_status: record.researchProtocol?.status ?? 'NotRecorded',
      protocol_version: record.analysisPlanApproval.protocol_version,
      protocol_sha256: record.analysisPlanApproval.protocol_sha256,
      protocol_approval_id: record.analysisPlanApproval.protocol_approval_id,
      protocol_updated_at: record.researchProtocol?.updated_at ?? null,
      protocol_approved_at: record.researchProtocol?.approved_at ?? null,
      plan_id: record.analysisPlanApproval.plan_id,
      plan_approval_id: record.analysisPlanApproval.approval_id,
      plan_approved_at: record.analysisPlanApproval.approved_at,
      run_spec_sha256: record.analysisPlanApproval.run_spec_sha256,
      audit_id: record.analysisPlanApproval.audit_id,
      audit_sha256: record.analysisPlanApproval.audit_sha256,
    };
    const protocolBytes = enc.encode(JSON.stringify(protocolDocument));
    const approvalBytes = enc.encode(JSON.stringify(approvalDocument));
    const auditBytes = enc.encode(JSON.stringify(record.datasetAudit));
    const resultBytes = enc.encode(JSON.stringify(record.result));
    const resultPath = 'artifacts/step-1/result.json';
    const llmConfig = options.llmConfig?.() ?? null;

    return {
      runId: record.runId,
      status: 'completed',
      datasetSha256: createHash('sha256').update(datasetBytes).digest(),
      datasetCsvBytes: datasetBytes,
      workflow: {
        schemaVersion: 1,
        inputDataset: { path: 'data.csv', sha256: datasetHex },
        steps: [{
          id: 'step-1',
          algorithm: record.algorithmId,
          params: record.params,
          inputs: [
            { path: 'protocol.json', sha256: engineSnapshot.sha256Hex(protocolBytes) },
            { path: 'approval.json', sha256: engineSnapshot.sha256Hex(approvalBytes) },
            { path: 'dataset-audit.json', sha256: engineSnapshot.sha256Hex(auditBytes) },
          ],
          outputs: [{ path: resultPath, sha256: engineSnapshot.sha256Hex(resultBytes) }],
          startedAtUtc: record.startedAtUtc,
          endedAtUtc: record.endedAtUtc,
        }],
      },
      artifacts: [
        { path: 'protocol.json', bytes: protocolBytes },
        { path: 'approval.json', bytes: approvalBytes },
        { path: 'dataset-audit.json', bytes: auditBytes },
        { path: resultPath, bytes: resultBytes },
      ],
      llmCalls: [...(options.llmCallsForSession?.(record.sessionId) ?? [])],
      // Every production analysis is executed by the in-process TypeScript engine.
      // No external R/SAS/Python/SPSS runtime is invoked for this run.
      referenceSoftware: [],
      osFamily: osFamily(),
      osVersion: release(),
      releaseVersion: coverage.getLoadedMatrix().release_version,
      commitSha: commitSha(),
      createdAtUtc: record.endedAtUtc,
      runtimeDependencies: { node: process.versions.node },
      apiKeys: llmConfig?.api_key ? [llmConfig.api_key] : [],
      workingDirectory: options.workingDirectory,
      narrativeSteps: [{
        id: 'step-1',
        algorithm: record.algorithmId,
        displayName: record.algorithmId,
        paramsSummary: '按已审批研究协议执行本机确定性分析',
        keyMetrics: [],
      }],
    };
  });

  const provider: SnapshotProvider = {
    async export(runId, destination) {
      const result = await materializer.export(runId, destination);
      snapshotHashes.set(runId, result.sha256);
      return result;
    },
  };

  return {
    recorder,
    provider,
    snapshotSha256: (runId) => snapshotHashes.get(runId),
  };
}
