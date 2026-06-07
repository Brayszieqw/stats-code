import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    include: ['tests/**/*.{test,spec}.ts', 'packages/**/*.{test,spec}.ts'],
    environment: 'node',
    globals: false,
    pool: 'forks',
  },
});
