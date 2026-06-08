// @stats-code/server — Fastify router, handlers, middleware, CORS, load
// shedding, request-id. Maps to the Rust `agent-server` crate.
// Depends on: @stats-code/engine.

import type { FastifyInstance } from 'fastify';
import { buildRouter, type BuildRouterOptions } from './router.js';

export * as contract from './contract/index.js';
export { buildRouter, type BuildRouterOptions } from './router.js';
export { MemSessionStore } from './mem-store.js';
export {
  StoreError,
  type AppState,
  type SessionStore,
  type Session,
  type DatasetSummary,
  type SessionSettings,
  type AgentEvent,
  type UserMessageInput,
  type MessageHandler,
  type LlmConfig,
  type LlmConfigStore,
  type LlmProbe,
  type CoverageMatrixProvider,
  type SidecarProvider,
  type SnapshotProvider,
} from './state.js';
export { serializeSseFrame } from './sse.js';
export {
  installSpaFallback,
  ASSET_PREFIXES,
  ROOT_ASSET_FILES,
  type ServedAsset,
  type SpaAssetSource,
} from './spa.js';
export {
  createSeaAssetSource,
  createDiskAssetSource,
  createDefaultAssetSource,
} from './spa-assets.js';
export {
  toWireMatrix,
  createCoverageMatrixProvider,
  createSidecarProvider,
  createSnapshotProvider,
} from './providers.js';
export {
  type LlmProvider,
  type LlmStatus,
  type PkcePair,
  type OAuthCapability,
  type SaveConfigInput,
  statusFromConfig,
  providerRequiresOAuth,
  generatePkcePair,
  testAndSaveConfig,
  LlmConfigError,
  OAUTH_REQUIRED_PROVIDERS,
} from './llm.js';

export * as conversation from './conversation/index.js';
export {
  createLlmProvider,
  DEFAULT_BASE_URLS,
  DEFAULT_MODELS,
  type LlmEvent,
  type LlmMessage,
  type LlmRequest,
  type LlmProviderOptions,
} from './conversation/index.js';
export type { LlmProvider as LlmStreamProvider } from './conversation/index.js';
export {
  createFileLlmConfigStore,
  defaultLlmConfigPath,
  type FileLlmConfigStoreOptions,
} from './conversation/index.js';
export { createLlmProbe, type CreateLlmProbeOptions } from './conversation/index.js';
export {
  skillToAlgorithm,
  SkillRegistry,
  SkillRunner,
  SkillRunErrorException,
  createFsDatasetStore,
  defaultDatasetRoot,
  detectRiskSignals,
  type SkillDescriptor,
  type SkillInvoker,
  type SkillContext,
  type SkillRunnerOptions,
  type SkillRunError,
  type DatasetStore,
  type FsDatasetStoreOptions,
  type AnalysisResultMeta,
} from './conversation/index.js';
export {
  createOrchestrator,
  type OrchestratorDeps,
  type IntentResult,
} from './conversation/index.js';

export interface HttpServer {
  start(opts: { host: '127.0.0.1'; port: number }): Promise<{ url: string }>;
  stop(): Promise<void>;
}

/** Create an HttpServer backed by the Fastify router (binds loopback only). */
export function createHttpServer(opts: BuildRouterOptions): HttpServer & { app: FastifyInstance } {
  const app = buildRouter(opts);
  return {
    app,
    async start({ host, port }) {
      await app.listen({ host, port });
      return { url: `http://${host}:${port}/` };
    },
    async stop() {
      await app.close();
    },
  };
}
