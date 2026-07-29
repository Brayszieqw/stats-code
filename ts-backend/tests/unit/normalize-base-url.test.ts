import { describe, it, expect } from 'vitest';
import { normalizeProviderBaseUrl, DEFAULT_BASE_URLS } from '@stats-code/server';

describe('normalizeProviderBaseUrl', () => {
  it('adds /v1 for official deepseek host', () => {
    expect(normalizeProviderBaseUrl('deepseek', 'https://api.deepseek.com')).toBe(
      'https://api.deepseek.com/v1',
    );
  });

  it('adds /compatible-mode/v1 for official dashscope (qwen) host', () => {
    expect(normalizeProviderBaseUrl('qwen', 'https://dashscope.aliyuncs.com')).toBe(
      'https://dashscope.aliyuncs.com/compatible-mode/v1',
    );
  });

  it('adds /v1 for official moonshot (kimi) host, both .cn and .ai', () => {
    expect(normalizeProviderBaseUrl('kimi', 'https://api.moonshot.cn')).toBe(
      'https://api.moonshot.cn/v1',
    );
    expect(normalizeProviderBaseUrl('kimi', 'https://api.moonshot.ai')).toBe(
      'https://api.moonshot.ai/v1',
    );
  });

  it('adds /api/paas/v4 for official bigmodel (zhipu) host', () => {
    expect(normalizeProviderBaseUrl('zhipu', 'https://open.bigmodel.cn')).toBe(
      'https://open.bigmodel.cn/api/paas/v4',
    );
  });

  it('never auto-appends a path for the custom provider, even on a known bare host', () => {
    expect(normalizeProviderBaseUrl('custom', 'https://api.deepseek.com')).toBe(
      'https://api.deepseek.com',
    );
    expect(normalizeProviderBaseUrl('custom', 'https://open.bigmodel.cn')).toBe(
      'https://open.bigmodel.cn',
    );
  });

  it('uses defaults when base is empty', () => {
    expect(normalizeProviderBaseUrl('deepseek', null)).toBe(DEFAULT_BASE_URLS.deepseek);
    expect(normalizeProviderBaseUrl('qwen', '')).toBe(DEFAULT_BASE_URLS.qwen);
    expect(normalizeProviderBaseUrl('kimi', undefined)).toBe(DEFAULT_BASE_URLS.kimi);
    expect(normalizeProviderBaseUrl('zhipu', '')).toBe(DEFAULT_BASE_URLS.zhipu);
  });

  it('throws for the custom provider when no base URL is supplied', () => {
    expect(() => normalizeProviderBaseUrl('custom', null)).toThrow('自定义 provider 需要 Base URL');
    expect(() => normalizeProviderBaseUrl('custom', '')).toThrow('自定义 provider 需要 Base URL');
  });

  it('strips trailing slash and full chat path', () => {
    expect(normalizeProviderBaseUrl('custom', 'https://proxy.example.com/v1/')).toBe(
      'https://proxy.example.com/v1',
    );
    expect(
      normalizeProviderBaseUrl('custom', 'https://proxy.example.com/v1/chat/completions'),
    ).toBe('https://proxy.example.com/v1');
  });
});
