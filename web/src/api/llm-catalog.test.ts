/**
 * Tests for the LLM provider catalog — the single source of truth for
 * provider labels, default base URL/model, and model option lists consumed
 * by OnboardingCard and AppShell's settings drawer.
 */

import { describe, it, expect } from 'vitest';
import {
  LLM_PROVIDER_CATALOG,
  PROVIDER_OPTIONS,
  DEFAULT_BASE_URLS,
  getDefaultModel,
  getModelOptions,
  isKnownModel,
  getBaseUrlPlaceholder,
  getModelPlaceholder,
  getProviderLabel,
} from './llm-catalog';
import type { LlmProvider } from './types';

const NON_CUSTOM_PROVIDERS: LlmProvider[] = ['deepseek', 'qwen', 'kimi', 'zhipu'];

describe('llm-catalog', () => {
  it('lists exactly the five supported providers', () => {
    expect(LLM_PROVIDER_CATALOG.map((e) => e.id)).toEqual(['deepseek', 'qwen', 'kimi', 'zhipu', 'custom']);
    expect(PROVIDER_OPTIONS.map((o) => o.value)).toEqual(['deepseek', 'qwen', 'kimi', 'zhipu', 'custom']);
  });

  it.each(NON_CUSTOM_PROVIDERS)('%s has a non-empty default base URL and default model', (provider) => {
    expect(DEFAULT_BASE_URLS[provider]).toMatch(/^https:\/\//);
    expect(getDefaultModel(provider)).not.toBe('');
  });

  it.each(NON_CUSTOM_PROVIDERS)("%s's default model is one of its own model options", (provider) => {
    const options = getModelOptions(provider);
    const defaultModel = getDefaultModel(provider);
    expect(options.some((o) => o.value === defaultModel)).toBe(true);
  });

  it.each(NON_CUSTOM_PROVIDERS)('%s marks every listed model option as known', (provider) => {
    for (const option of getModelOptions(provider)) {
      expect(isKnownModel(provider, option.value)).toBe(true);
    }
    expect(isKnownModel(provider, 'not-a-real-model-id')).toBe(false);
  });

  it('custom has no default base URL or model, and an empty model list', () => {
    expect(DEFAULT_BASE_URLS.custom).toBe('');
    expect(getDefaultModel('custom')).toBe('');
    expect(getModelOptions('custom')).toEqual([]);
  });

  it('isKnownModel is unconditionally true for custom (free-text models)', () => {
    expect(isKnownModel('custom', 'anything-the-user-typed')).toBe(true);
    expect(isKnownModel('custom', '')).toBe(true);
  });

  it('exposes placeholders for base URL and model for every provider', () => {
    for (const entry of LLM_PROVIDER_CATALOG) {
      expect(getBaseUrlPlaceholder(entry.id)).not.toBe('');
      expect(getModelPlaceholder(entry.id)).not.toBe('');
    }
  });

  it('getProviderLabel returns the catalog label and falls back for null/undefined', () => {
    expect(getProviderLabel('qwen')).toBe('通义千问(DashScope)');
    expect(getProviderLabel(null)).toBe('LLM');
    expect(getProviderLabel(undefined)).toBe('LLM');
  });

  it('model option labels carry a context-window hint', () => {
    const qwenPlus = getModelOptions('qwen').find((o) => o.value === 'qwen-plus');
    expect(qwenPlus?.label).toBe('qwen-plus(128K)');
  });
});
