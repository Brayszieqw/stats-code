import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    include: ['tests/**/*.{test,spec}.ts', 'packages/**/*.{test,spec}.ts'],
    environment: 'node',
    globals: false,
    pool: 'forks',
    // Property/replay suites shell out to Expand-Archive and routinely exceed
    // vitest 3's default 5s when the full suite is under load.
    testTimeout: 30_000,
    // Covers npm scripts, `npx vitest`, and IDE test runners that load this config.
    globalSetup: ['./scripts/vitest-global-setup.mjs'],
  },
});
