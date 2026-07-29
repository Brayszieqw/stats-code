// server/conversation/llm-catalog.ts — LLM provider registry (single source of
// truth for provider ids, base URLs, default models, and model lists).
//
// Chinese-market providers only (Requirement: remove foreign providers):
//   deepseek (DeepSeek 深度求索), qwen (通义千问 / DashScope), kimi (Kimi / Moonshot),
//   zhipu (智谱 GLM), custom (OpenAI-compatible endpoint / relay, user-supplied
//   base URL and model — no defaults).
//
// Consumed by llm-provider.ts (base URL / model defaults + host normalization),
// llm-config-store.ts (v2 per-provider cache), and the HTTP layer (llm-status
// cached_providers, POST /api/llm-config/activate).

export type LlmProviderId = 'deepseek' | 'qwen' | 'kimi' | 'zhipu' | 'custom';

export interface LlmModelInfo {
  id: string;
  label: string;
  /** Context window size, in tokens. */
  contextWindow: number;
  note?: string;
}

export interface LlmProviderInfo {
  id: LlmProviderId;
  label: string;
  /** null for `custom` — the user must supply a base URL. */
  baseUrl: string | null;
  /** null for `custom` — the user must supply a model. */
  defaultModel: string | null;
  models: LlmModelInfo[];
  /** Only true for `custom`: no built-in base URL to fall back on. */
  requiresBaseUrl: boolean;
}

export const LLM_PROVIDER_IDS = ['deepseek', 'qwen', 'kimi', 'zhipu', 'custom'] as const;

export const LLM_PROVIDER_CATALOG: Record<LlmProviderId, LlmProviderInfo> = {
  deepseek: {
    id: 'deepseek',
    label: 'DeepSeek 深度求索',
    baseUrl: 'https://api.deepseek.com/v1',
    defaultModel: 'deepseek-chat',
    requiresBaseUrl: false,
    models: [
      { id: 'deepseek-chat', label: 'deepseek-chat', contextWindow: 131072 },
      { id: 'deepseek-reasoner', label: 'deepseek-reasoner', contextWindow: 131072 },
    ],
  },
  qwen: {
    id: 'qwen',
    label: '通义千问（阿里云百炼 DashScope）',
    baseUrl: 'https://dashscope.aliyuncs.com/compatible-mode/v1',
    defaultModel: 'qwen-plus',
    requiresBaseUrl: false,
    models: [
      { id: 'qwen-plus', label: 'qwen-plus', contextWindow: 131072 },
      { id: 'qwen-max', label: 'qwen-max', contextWindow: 32768 },
      { id: 'qwen-turbo', label: 'qwen-turbo', contextWindow: 1000000 },
      { id: 'qwen-long', label: 'qwen-long', contextWindow: 10000000 },
    ],
  },
  kimi: {
    id: 'kimi',
    label: 'Kimi（月之暗面 Moonshot）',
    baseUrl: 'https://api.moonshot.cn/v1',
    defaultModel: 'kimi-latest',
    requiresBaseUrl: false,
    models: [
      { id: 'kimi-latest', label: 'kimi-latest', contextWindow: 131072 },
      { id: 'kimi-k2-turbo-preview', label: 'kimi-k2-turbo-preview', contextWindow: 262144 },
      { id: 'kimi-k2-thinking', label: 'kimi-k2-thinking', contextWindow: 262144 },
      { id: 'kimi-k2-0711-preview', label: 'kimi-k2-0711-preview', contextWindow: 131072 },
      { id: 'moonshot-v1-32k', label: 'moonshot-v1-32k', contextWindow: 32768 },
    ],
  },
  zhipu: {
    id: 'zhipu',
    label: '智谱 GLM（bigmodel.cn）',
    baseUrl: 'https://open.bigmodel.cn/api/paas/v4',
    defaultModel: 'glm-4.5',
    requiresBaseUrl: false,
    models: [
      { id: 'glm-4.7', label: 'glm-4.7', contextWindow: 204800 },
      { id: 'glm-4.6', label: 'glm-4.6', contextWindow: 204800 },
      { id: 'glm-4.5', label: 'glm-4.5', contextWindow: 131072 },
      { id: 'glm-4.5-air', label: 'glm-4.5-air', contextWindow: 131072 },
      { id: 'glm-4-flash', label: 'glm-4-flash', contextWindow: 131072, note: '免费' },
    ],
  },
  custom: {
    id: 'custom',
    label: '自定义（OpenAI 兼容 / 中转）',
    baseUrl: null,
    defaultModel: null,
    requiresBaseUrl: true,
    models: [],
  },
};
