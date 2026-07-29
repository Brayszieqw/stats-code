/**
 * LLM provider catalog — single source of truth for the provider directory
 * shown in Onboarding_Card and the API settings drawer (AppShell).
 *
 * Adding/removing a provider only requires editing `LLM_PROVIDER_CATALOG`
 * below; every consumer (labels, default base URL, default model, model
 * option lists) derives from this table.
 */

import type { LlmProvider } from './types';

export interface LlmModelOption {
  value: string;
  /** Display label; may include a context-window hint, e.g. "qwen-plus(128K)". */
  label: string;
}

export interface LlmProviderCatalogEntry {
  id: LlmProvider;
  /** Chinese display label shown in provider pickers. */
  label: string;
  /** Default API base URL; empty string for `custom` (user must supply one). */
  baseUrl: string;
  /** Placeholder shown in the base-URL field when the value is empty. */
  baseUrlPlaceholder: string;
  /** Default model id; empty string for `custom` (no preset). */
  defaultModel: string;
  /** Placeholder shown in the model field when the value is empty. */
  modelPlaceholder: string;
  /** Preset model options; empty for `custom` (free text only). */
  models: LlmModelOption[];
}

export const LLM_PROVIDER_CATALOG: LlmProviderCatalogEntry[] = [
  {
    id: 'deepseek',
    label: 'DeepSeek 深度求索',
    baseUrl: 'https://api.deepseek.com/v1',
    baseUrlPlaceholder: 'https://api.deepseek.com/v1',
    defaultModel: 'deepseek-chat',
    modelPlaceholder: 'deepseek-chat',
    models: [
      { value: 'deepseek-chat', label: 'deepseek-chat(128K)' },
      { value: 'deepseek-reasoner', label: 'deepseek-reasoner(128K)' },
    ],
  },
  {
    id: 'qwen',
    label: '通义千问(DashScope)',
    baseUrl: 'https://dashscope.aliyuncs.com/compatible-mode/v1',
    baseUrlPlaceholder: 'https://dashscope.aliyuncs.com/compatible-mode/v1',
    defaultModel: 'qwen-plus',
    modelPlaceholder: 'qwen-plus',
    models: [
      { value: 'qwen-plus', label: 'qwen-plus(128K)' },
      { value: 'qwen-max', label: 'qwen-max(32K)' },
      { value: 'qwen-turbo', label: 'qwen-turbo(1M)' },
      { value: 'qwen-long', label: 'qwen-long(10M)' },
    ],
  },
  {
    id: 'kimi',
    label: 'Kimi(Moonshot)',
    baseUrl: 'https://api.moonshot.cn/v1',
    baseUrlPlaceholder: 'https://api.moonshot.cn/v1',
    defaultModel: 'kimi-latest',
    modelPlaceholder: 'kimi-latest',
    models: [
      { value: 'kimi-latest', label: 'kimi-latest(128K)' },
      { value: 'kimi-k2-turbo-preview', label: 'kimi-k2-turbo-preview(256K)' },
      { value: 'kimi-k2-thinking', label: 'kimi-k2-thinking(256K)' },
      { value: 'kimi-k2-0711-preview', label: 'kimi-k2-0711-preview(128K)' },
      { value: 'moonshot-v1-32k', label: 'moonshot-v1-32k(32K)' },
    ],
  },
  {
    id: 'zhipu',
    label: '智谱 GLM',
    baseUrl: 'https://open.bigmodel.cn/api/paas/v4',
    baseUrlPlaceholder: 'https://open.bigmodel.cn/api/paas/v4',
    defaultModel: 'glm-4.5',
    modelPlaceholder: 'glm-4.5',
    models: [
      { value: 'glm-4.7', label: 'glm-4.7(200K)' },
      { value: 'glm-4.6', label: 'glm-4.6(200K)' },
      { value: 'glm-4.5', label: 'glm-4.5(128K)' },
      { value: 'glm-4.5-air', label: 'glm-4.5-air(128K)' },
      { value: 'glm-4-flash', label: 'glm-4-flash(128K·免费)' },
    ],
  },
  {
    id: 'custom',
    label: '自定义(OpenAI 兼容 / 中转)',
    baseUrl: '',
    baseUrlPlaceholder: 'https://your-relay.example.com/v1',
    defaultModel: '',
    modelPlaceholder: '例如 gpt-4o、claude-3-5-sonnet 等',
    models: [],
  },
];

const CATALOG_BY_ID = new Map<LlmProvider, LlmProviderCatalogEntry>(
  LLM_PROVIDER_CATALOG.map((entry) => [entry.id, entry]),
);

function getCatalogEntry(provider: LlmProvider): LlmProviderCatalogEntry {
  const entry = CATALOG_BY_ID.get(provider);
  if (!entry) {
    throw new Error(`unknown LLM provider: ${String(provider)}`);
  }
  return entry;
}

/** Options for the provider picker Select. */
export const PROVIDER_OPTIONS: { value: LlmProvider; label: string }[] = LLM_PROVIDER_CATALOG.map(
  (entry) => ({ value: entry.id, label: entry.label }),
);

/** Default API base URL per provider (empty string for `custom`). */
export const DEFAULT_BASE_URLS: Record<LlmProvider, string> = Object.fromEntries(
  LLM_PROVIDER_CATALOG.map((entry) => [entry.id, entry.baseUrl]),
) as Record<LlmProvider, string>;

export function getDefaultModel(provider: LlmProvider): string {
  return getCatalogEntry(provider).defaultModel;
}

export function getModelOptions(provider: LlmProvider): LlmModelOption[] {
  return getCatalogEntry(provider).models;
}

/** `custom` has no preset list, so any non-empty model the user typed counts as known. */
export function isKnownModel(provider: LlmProvider, model: string): boolean {
  if (provider === 'custom') return true;
  return getCatalogEntry(provider).models.some((option) => option.value === model);
}

export function getBaseUrlPlaceholder(provider: LlmProvider): string {
  return getCatalogEntry(provider).baseUrlPlaceholder;
}

export function getModelPlaceholder(provider: LlmProvider): string {
  return getCatalogEntry(provider).modelPlaceholder;
}

/** Chinese display label for a provider id; falls back to a generic label when absent. */
export function getProviderLabel(provider: LlmProvider | null | undefined): string {
  if (!provider) return 'LLM';
  return getCatalogEntry(provider).label;
}
