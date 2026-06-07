// server/state.ts — application state: injectable stores + providers.
// Mirrors crates/agent-server/src/state.rs (AppState). Providers are optional;
// routes that need an absent provider return 503, matching the Rust handlers.

import type { domain, sidecar } from './contract/index.js';
import type { z } from 'zod';

export type Session = z.infer<typeof domain.session>;
export type DatasetSummary = z.infer<typeof domain.datasetSummary>;
export type SessionSettings = z.infer<typeof domain.sessionSettings>;

export class StoreError extends Error {
  constructor(
    public readonly kind: 'not_found' | 'archived' | 'internal',
    message: string,
  ) {
    super(message);
    this.name = 'StoreError';
  }
}

/** Session persistence abstraction (in-memory default provided). */
export interface SessionStore {
  create(): Promise<Session>;
  get(id: string): Promise<Session>;
  updateSettings(id: string, settings: SessionSettings): Promise<void>;
  appendDataset(id: string, dataset: DatasetSummary): Promise<void>;
}

export interface LlmConfig {
  provider: 'deepseek' | 'openai';
  api_key: string;
  base_url?: string | null;
  model?: string | null;
}

export interface LlmConfigStore {
  read(): LlmConfig | null;
  write(config: LlmConfig): void;
}

export interface LlmProbe {
  probe(provider: 'deepseek' | 'openai', apiKey: string, baseUrl?: string, model?: string): Promise<void>;
}

export interface CoverageMatrixProvider {
  get(): z.infer<typeof sidecar.coverageMatrix>;
}

export interface SidecarProvider {
  generate(
    algorithmId: string,
    request: z.infer<typeof sidecar.sidecarRenderRequest>,
  ): z.infer<typeof sidecar.sidecarSnippet>;
}

export interface SnapshotProvider {
  export(runId: string, destination: string): z.infer<typeof sidecar.snapshotExportResponse>;
}

export interface AppState {
  sessionStore: SessionStore;
  llmConfigStore?: LlmConfigStore;
  llmProbe?: LlmProbe;
  coverageMatrixProvider?: CoverageMatrixProvider;
  sidecarProvider?: SidecarProvider;
  snapshotProvider?: SnapshotProvider;
}
