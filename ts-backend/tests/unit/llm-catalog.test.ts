// tests/unit/llm-catalog.test.ts — provider registry invariants (single
// source of truth consumed by llm-provider.ts, llm-config-store.ts, and the
// HTTP layer).

import { describe, it, expect } from 'vitest';
import { LLM_PROVIDER_IDS, LLM_PROVIDER_CATALOG } from '@stats-code/server';

describe('LLM_PROVIDER_IDS / LLM_PROVIDER_CATALOG', () => {
  it('declares exactly the five Chinese-market-plus-custom providers', () => {
    expect(LLM_PROVIDER_IDS).toEqual(['deepseek', 'qwen', 'kimi', 'zhipu', 'custom']);
    expect(Object.keys(LLM_PROVIDER_CATALOG).sort()).toEqual(
      [...LLM_PROVIDER_IDS].sort(),
    );
  });

  it('every provider has a non-empty label', () => {
    for (const id of LLM_PROVIDER_IDS) {
      const info = LLM_PROVIDER_CATALOG[id];
      expect(info.label.length).toBeGreaterThan(0);
      expect(info.id).toBe(id);
    }
  });

  it('every non-custom provider has a default model that is present in its own model list', () => {
    for (const id of LLM_PROVIDER_IDS) {
      if (id === 'custom') continue;
      const info = LLM_PROVIDER_CATALOG[id];
      expect(info.defaultModel).not.toBeNull();
      expect(info.models.length).toBeGreaterThan(0);
      expect(info.models.map((m) => m.id)).toContain(info.defaultModel);
      expect(info.baseUrl).not.toBeNull();
      expect(info.requiresBaseUrl).toBe(false);
    }
  });

  it('every model entry declares a positive context window and non-empty label', () => {
    for (const id of LLM_PROVIDER_IDS) {
      for (const model of LLM_PROVIDER_CATALOG[id].models) {
        expect(model.contextWindow).toBeGreaterThan(0);
        expect(model.label.length).toBeGreaterThan(0);
      }
    }
  });

  it('custom has no base URL, no default model, no preset models, and requiresBaseUrl', () => {
    const custom = LLM_PROVIDER_CATALOG.custom;
    expect(custom.baseUrl).toBeNull();
    expect(custom.defaultModel).toBeNull();
    expect(custom.models).toEqual([]);
    expect(custom.requiresBaseUrl).toBe(true);
  });

  it('the retired openai provider is not part of the catalog', () => {
    expect((LLM_PROVIDER_IDS as readonly string[]).includes('openai')).toBe(false);
    expect(Object.prototype.hasOwnProperty.call(LLM_PROVIDER_CATALOG, 'openai')).toBe(false);
  });
});
