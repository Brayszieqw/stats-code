// @stats-code/api launcher.ts — application composition + launcher runner.
//
// Wires the engine launcher (port scan, single-instance lock, child supervisor)
// to the server router (all 13 routes + SSE + SPA fallback), producing the
// runLauncher hook the CLI delegates to. This is the real entry that the
// distribution artifact (stats-code.exe) runs for `stats-code [--no-browser]`.
//
// Requirements 9 (launcher lifecycle), 10 (single-file artifact serving the SPA
// from embedded assets), 1.1 (HTTP_Server binds loopback).

import type { AddressInfo } from 'node:net';
import {
  buildRouter,
  createDefaultAssetSource,
  createCoverageMatrixProvider,
  createSidecarProvider,
  createSnapshotRunRegistry,
  createLlmCallHistory,
  createFileLlmConfigStore,
  createLlmProbe,
  createFsDatasetStore,
  defaultDatasetRoot,
  createLlmProvider,
  createProtocolCompiler,
  createOrchestrator,
  createResearchWorkflowService,
  SkillRegistry,
  SkillRunner,
  createFileSessionStore,
  type AppState,
} from '@stats-code/server';
import { launcher as engineLauncher } from '@stats-code/engine';

export interface LauncherArgs {
  noBrowser: boolean;
}

export interface RunLauncherOptions {
  /** Build the application state (stores + providers). Defaults to a minimal in-memory state. */
  buildState?: () => AppState;
  /** Override the port scan range. Dev server uses this to stay aligned with Vite proxy. */
  portRange?: { start: number; endExclusive: number };
  /** Open the given URL in the default browser (outside any guarded scope). */
  openBrowser?: (url: string) => void;
  /** Sink for status lines. */
  log?: (line: string) => void;
}

/**
 * Minimal default state: in-memory session store + the trust-credential
 * providers (coverage matrix + sidecar) wired from the engine factories.
 *
 * Completed deterministic runs are registered for on-demand audit snapshot
 * export. Dataset bytes remain in the local dataset store until export.
 */
/**
 * Production application state: in-memory session store + the trust-credential
 * providers + the full conversation stack (dataset store, LLM config store +
 * probe, skill registry/runner, and the orchestrator MessageHandler).
 *
 * The orchestrator reads the CURRENT persisted LLM config per message via the
 * provider factory closure, so a runtime POST /api/llm-config takes effect on
 * the next message without a restart (Requirement 10.2).
 *
 * Audit snapshots are wired for completed deterministic runs. OAuth is
 * unavailable (API-key providers only) and speech-to-text remains a stub.
 */
export function defaultState(): AppState {
  const sessionStore = createFileSessionStore();
  const datasetRoot = defaultDatasetRoot();
  const datasetStore = createFsDatasetStore({ root: datasetRoot });
  const llmConfigStore = createFileLlmConfigStore();
  const llmCallHistory = createLlmCallHistory();
  const snapshotRuns = createSnapshotRunRegistry(datasetStore, {
    llmCallsForSession: (sessionId) => llmCallHistory.list(sessionId),
    llmConfig: () => llmConfigStore.read(),
    workingDirectory: datasetRoot,
  });
  const llmProbe = createLlmProbe();
  const registry = SkillRegistry.withDefaults();
  const runner = new SkillRunner(registry);
  const researchWorkflow = createResearchWorkflowService({
    sessionStore,
    datasetStore,
    registry,
    runner,
    snapshotRunRecorder: snapshotRuns.recorder,
  });

  const llmProviderFactory = (sessionId?: string) => {
    const cfg = llmConfigStore.read();
    if (!cfg?.api_key) return null;
    const provider = createLlmProvider({
      provider: cfg.provider,
      apiKey: cfg.api_key,
      baseUrl: cfg.base_url ?? undefined,
      model: cfg.model ?? undefined,
    });
    return sessionId ? llmCallHistory.wrap(sessionId, provider) : provider;
  };

  const messageHandler = createOrchestrator({
    sessionStore,
    registry,
    researchWorkflow,
    llmProviderFactory,
  });
  const protocolCompiler = createProtocolCompiler(llmProviderFactory);

  return {
    sessionStore,
    datasetStore,
    skillRunner: runner,
    skillRegistry: registry,
    researchWorkflow,
    coverageMatrixProvider: createCoverageMatrixProvider(),
    sidecarProvider: createSidecarProvider(),
    snapshotProvider: snapshotRuns.provider,
    snapshotRunRecorder: snapshotRuns.recorder,
    llmConfigStore,
    llmProbe,
    protocolCompiler,
    messageHandler,
    oauthCapability: { available: false },
  };
}

/**
 * Start the application: scan for a free loopback port (8080-8200), bind the
 * Fastify router with the SPA fallback installed, and (unless --no-browser)
 * open the browser. Returns the process exit code. Keeps the process alive
 * while the server is listening.
 */
export async function runLauncher(args: LauncherArgs, opts: RunLauncherOptions = {}): Promise<number> {
  const log = opts.log ?? ((l: string) => process.stdout.write(`${l}\n`));
  const state = (opts.buildState ?? defaultState)();

  // Find a free port using the engine's scanner, then hand the port to Fastify.
  const probe = await engineLauncher.scanFirstBindable(opts.portRange ?? engineLauncher.DEFAULT_RANGE);
  const port = (probe.address() as AddressInfo).port;
  // Release the probe socket so Fastify can bind the same port.
  await new Promise<void>((resolve) => probe.close(() => resolve()));

  const app = buildRouter({
    state,
    installSpaFallback: true,
    spaAssetSource: safeAssetSource(),
  });

  await app.listen({ host: '127.0.0.1', port });
  const url = `http://127.0.0.1:${port}/`;
  log(`stats-code listening on ${url}`);

  if (!args.noBrowser && opts.openBrowser) {
    // Browser invocation lives OUTSIDE any guarded scope (Requirement 8.5).
    opts.openBrowser(url);
  }

  // Resolve only when the server closes (keeps the artifact running).
  await new Promise<void>((resolve) => {
    const shutdown = () => {
      app.close().finally(() => resolve());
    };
    process.once('SIGINT', shutdown);
    process.once('SIGTERM', shutdown);
  });
  return 0;
}

/** Embedded SPA assets, with a minimal shell fallback if none are embedded. */
function safeAssetSource() {
  try {
    return createDefaultAssetSource();
  } catch {
    return {
      get: () => undefined,
      indexHtml: () => ({
        bytes: new TextEncoder().encode('<!doctype html><title>stats-code</title>'),
        contentType: 'text/html; charset=utf-8',
      }),
    };
  }
}
