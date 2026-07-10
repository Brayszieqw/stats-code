import { describe, it, expect } from 'vitest';
import { normalizeProviderBaseUrl, DEFAULT_BASE_URLS } from '@stats-code/server';

describe('normalizeProviderBaseUrl', () => {
  it('adds /v1 for official deepseek host', () => {
    expect(normalizeProviderBaseUrl('deepseek', 'https://api.deepseek.com')).toBe(
      'https://api.deepseek.com/v1',
    );
  });

  it('adds /v1 for official openai host', () => {
    expect(normalizeProviderBaseUrl('openai', 'https://api.openai.com')).toBe(
      'https://api.openai.com/v1',
    );
  });

  it('uses defaults when base is empty', () => {
    expect(normalizeProviderBaseUrl('deepseek', null)).toBe(DEFAULT_BASE_URLS.deepseek);
    expect(normalizeProviderBaseUrl('openai', '')).toBe(DEFAULT_BASE_URLS.openai);
  });

  it('strips trailing slash and full chat path', () => {
    expect(normalizeProviderBaseUrl('openai', 'https://proxy.example.com/v1/')).toBe(
      'https://proxy.example.com/v1',
    );
    expect(
      normalizeProviderBaseUrl('openai', 'https://proxy.example.com/v1/chat/completions'),
    ).toBe('https://proxy.example.com/v1');
  });
});
