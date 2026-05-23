/**
 * Vitest setup — runs before every test file.
 * Polyfills browser APIs that jsdom doesn't ship by default but Ant Design uses.
 */

import '@testing-library/jest-dom';
import { vi } from 'vitest';

// Ant Design's responsive observer relies on matchMedia.
if (typeof window !== 'undefined' && !window.matchMedia) {
  Object.defineProperty(window, 'matchMedia', {
    writable: true,
    value: vi.fn().mockImplementation((query: string) => ({
      matches: false,
      media: query,
      onchange: null,
      addListener: vi.fn(),
      removeListener: vi.fn(),
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      dispatchEvent: vi.fn(),
    })),
  });
}

// Some antd / @rc-component utilities access ResizeObserver.
if (typeof window !== 'undefined' && !window.ResizeObserver) {
  class MockResizeObserver {
    observe(): void {}
    unobserve(): void {}
    disconnect(): void {}
  }
  Object.defineProperty(window, 'ResizeObserver', {
    writable: true,
    value: MockResizeObserver,
  });
}

// scrollIntoView used by MessageList autoscroll
if (typeof Element !== 'undefined' && !Element.prototype.scrollIntoView) {
  Element.prototype.scrollIntoView = vi.fn();
}
